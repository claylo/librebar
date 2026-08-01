use std::fmt;
use std::io;
use std::sync::Mutex;
use std::sync::mpsc::{self, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use async_trait::async_trait;
use opentelemetry_http::hyper::HyperClient;
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};

type HttpResult = Result<Response<Bytes>, HttpError>;

struct Work {
    request: Request<Bytes>,
    response: SyncSender<HttpResult>,
}

/// Runs Hyper on a private Tokio runtime while presenting a blocking client
/// to OpenTelemetry's dedicated-thread batch processor.
pub(super) struct BlockingHyperClient {
    work: Option<SyncSender<Work>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl BlockingHyperClient {
    pub(super) fn new(timeout: Duration) -> io::Result<Self> {
        let (work, receiver) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let worker = thread::Builder::new()
            .name("librebar-otel-hyper".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let client = HyperClient::with_default_connector(timeout, None);
                if ready_tx.send(Ok(())).is_err() {
                    return;
                }

                while let Ok(Work { request, response }) = receiver.recv() {
                    let result = runtime.block_on(client.send_bytes(request));
                    let _ = response.send(result);
                }
            })?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                work: Some(work),
                worker: Mutex::new(Some(worker)),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "OTLP Hyper runtime stopped during initialization",
                ))
            }
        }
    }
}

impl fmt::Debug for BlockingHyperClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingHyperClient")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl HttpClient for BlockingHyperClient {
    async fn send_bytes(&self, request: Request<Bytes>) -> HttpResult {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        self.work
            .as_ref()
            .expect("HTTP client cannot send after drop")
            .send(Work {
                request,
                response: response_tx,
            })
            .map_err(|_| worker_stopped("before accepting an OTLP request"))?;
        response_rx
            .recv()
            .map_err(|_| worker_stopped("before completing an OTLP request"))?
    }
}

impl Drop for BlockingHyperClient {
    fn drop(&mut self) {
        self.work.take();
        let worker = self
            .worker
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }
}

fn worker_stopped(context: &'static str) -> HttpError {
    Box::new(io::Error::new(
        io::ErrorKind::BrokenPipe,
        format!("OTLP Hyper runtime stopped {context}"),
    ))
}
