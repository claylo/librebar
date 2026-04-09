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
        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|e| crate::Error::ShutdownInit(Box::new(e)))?;

        let handle = self.clone();

        tokio::spawn(async move {
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
    pub async fn cancelled(&mut self) {
        if *self.receiver.borrow_and_update() {
            return;
        }
        self.receiver.changed().await.ok();
    }

    /// Check if shutdown has been triggered (non-async).
    pub fn is_shutting_down(&self) -> bool {
        *self.receiver.borrow()
    }
}
