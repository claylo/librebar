//! Graceful shutdown with signal handling.
//!
//! Provides [`ShutdownHandle`] for triggering shutdown and [`ShutdownToken`]
//! for waiting on the shutdown signal. Uses `tokio::sync::watch` so multiple
//! consumers can await shutdown without ownership issues.
//!
//! # Usage
//!
//! ```ignore
//! let app = rebar::init("myapp").shutdown().start()?;
//! let mut token = app.shutdown_token();
//!
//! tokio::select! {
//!     _ = do_work() => {},
//!     _ = token.cancelled() => { /* cleanup */ },
//! }
//! ```

use tokio::sync::watch;

/// Handle for triggering and observing shutdown.
///
/// Stored in [`App`](crate::App). Clone is cheap (Arc internally via watch).
#[derive(Clone, Debug)]
pub struct ShutdownHandle {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl ShutdownHandle {
    /// Create a new shutdown handle (not yet shutting down).
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    /// Trigger shutdown. All tokens will be notified.
    ///
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn shutdown(&self) {
        // Receiver may be dropped if no tokens are outstanding — that's fine.
        let _ = self.sender.send(true);
    }

    /// Check if shutdown has been triggered.
    pub fn is_shutting_down(&self) -> bool {
        *self.receiver.borrow()
    }

    /// Create a token for waiting on shutdown.
    pub fn token(&self) -> ShutdownToken {
        ShutdownToken {
            receiver: self.receiver.clone(),
        }
    }

    /// Register OS signal handlers (SIGTERM, SIGINT) that trigger shutdown.
    ///
    /// Spawns a tokio task that listens for signals. The task exits when
    /// a signal is received or when the handle is dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if signal handler registration fails.
    pub fn register_signals(&self) -> crate::Result<()> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|e| crate::Error::NoRuntime(Box::new(e)))?;

        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|e| crate::Error::ShutdownInit(Box::new(e)))?;

        let handle = self.clone();

        tracing::debug!("registering shutdown signal handlers");
        runtime.spawn(async move {
            let ctrl_c = tokio::signal::ctrl_c();

            #[cfg(unix)]
            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }

            #[cfg(not(unix))]
            ctrl_c.await.ok();

            tracing::info!("shutdown signal received");
            handle.shutdown();
        });

        Ok(())
    }
}

impl Default for ShutdownHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Token for waiting on shutdown. Cloneable and cheap.
#[derive(Clone, Debug)]
pub struct ShutdownToken {
    receiver: watch::Receiver<bool>,
}

impl ShutdownToken {
    /// Wait until shutdown is triggered.
    ///
    /// Resolves immediately if shutdown has already been triggered.
    /// If the [`ShutdownHandle`] is dropped without triggering shutdown,
    /// this future will remain pending (never resolves spuriously).
    pub async fn cancelled(&mut self) {
        loop {
            if *self.receiver.borrow_and_update() {
                return;
            }
            // If all senders dropped without setting true, the channel is
            // dead — return pending forever rather than treating it as shutdown.
            if self.receiver.changed().await.is_err() {
                tracing::warn!("shutdown handle dropped without triggering shutdown");
                std::future::pending::<()>().await;
            }
        }
    }

    /// Check if shutdown has been triggered (non-async).
    pub fn is_shutting_down(&self) -> bool {
        *self.receiver.borrow()
    }
}
