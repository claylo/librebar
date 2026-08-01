use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use opentelemetry_otlp::{SpanExporter, WithExportConfig as _};
use tokio::sync::oneshot;

pub(super) struct BlockingTonicRuntime {
    shutdown: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl BlockingTonicRuntime {
    pub(super) fn build_exporter(endpoint: &str) -> crate::Result<(SpanExporter, Self)> {
        let endpoint = endpoint.to_owned();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<crate::Result<SpanExporter>>(0);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let worker = thread::Builder::new()
            .name("librebar-otel-grpc".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.into()));
                        return;
                    }
                };

                runtime.block_on(async move {
                    let exporter = SpanExporter::builder()
                        .with_tonic()
                        .with_endpoint(endpoint)
                        .build()
                        .map_err(|error| crate::Error::OtelInit(crate::error::boxed_error(error)));

                    match exporter {
                        Ok(exporter) => {
                            if ready_tx.send(Ok(exporter)).is_ok() {
                                let _ = shutdown_rx.await;
                            }
                        }
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                        }
                    }
                });
            })?;

        match ready_rx.recv() {
            Ok(Ok(exporter)) => Ok((
                exporter,
                Self {
                    shutdown: Some(shutdown_tx),
                    worker: Some(worker),
                },
            )),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "OTLP gRPC runtime stopped during initialization",
                )
                .into())
            }
        }
    }
}

impl Drop for BlockingTonicRuntime {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
