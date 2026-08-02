//! Graceful shutdown with signal handling.
//!
//! Provides [`ShutdownHandle`] for triggering shutdown and [`ShutdownToken`]
//! for waiting on the shutdown signal. Uses `tokio::sync::watch` so multiple
//! consumers can await shutdown without ownership issues.
//!
//! # Signal behavior
//!
//! Registering signal handlers permanently replaces the platform's default
//! SIGINT and SIGTERM behavior for the process. The first signal requests
//! graceful shutdown; a later signal forces an immediate exit with the
//! conventional signal-derived status code.
//!
//! # Usage
//!
//! ```no_run
//! # async fn do_work() {}
//! # async fn example() -> librebar::Result<()> {
//! let app = librebar::init("myapp").shutdown().start()?;
//! let mut token = app.shutdown_token().expect("shutdown() was called on the builder");
//!
//! tokio::select! {
//!     _ = do_work() => {},
//!     _ = token.cancelled() => { /* cleanup */ },
//! }
//! # Ok(())
//! # }
//! ```

use std::sync::OnceLock;
use tokio::sync::watch;

static SIGNAL_REGISTERED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownSignal {
    Interrupt,
    #[cfg(unix)]
    Terminate,
}

impl ShutdownSignal {
    const fn name(self) -> &'static str {
        match self {
            Self::Interrupt => "SIGINT",
            #[cfg(unix)]
            Self::Terminate => "SIGTERM",
        }
    }

    const fn exit_code(self) -> i32 {
        match self {
            Self::Interrupt => 130,
            #[cfg(unix)]
            Self::Terminate => 143,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SignalAction {
    Shutdown,
    Exit(i32),
}

fn action_for_signal(handle: &ShutdownHandle, signal: ShutdownSignal) -> SignalAction {
    if handle.is_shutting_down() {
        SignalAction::Exit(signal.exit_code())
    } else {
        handle.shutdown();
        SignalAction::Shutdown
    }
}

#[cfg(any(test, not(unix)))]
fn ctrl_c_signal(result: std::io::Result<()>) -> Option<ShutdownSignal> {
    match result {
        Ok(()) => Some(ShutdownSignal::Interrupt),
        Err(error) => {
            tracing::error!(%error, "failed to listen for Ctrl-C; signal task exiting");
            None
        }
    }
}

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
    /// Spawns a tokio task that remains active after the first signal. The
    /// first signal requests graceful shutdown; a later signal forces an
    /// immediate exit with status 130 for SIGINT or 143 for SIGTERM.
    /// Registering these handlers permanently replaces the platform's default
    /// signal behavior for the process.
    ///
    /// # Errors
    ///
    /// Returns an error if signal handler registration fails.
    pub fn register_signals(&self) -> crate::Result<()> {
        if SIGNAL_REGISTERED.set(()).is_err() {
            tracing::warn!("signal handlers already registered; ignoring duplicate registration");
            return Ok(());
        }

        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|error| crate::Error::NoRuntime(crate::error::boxed_error(error)))?;

        #[cfg(unix)]
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .map_err(crate::Error::ShutdownInit)?;

        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(crate::Error::ShutdownInit)?;

        let handle = self.clone();

        tracing::debug!("registering shutdown signal handlers");
        runtime.spawn(async move {
            loop {
                #[cfg(unix)]
                let signal = tokio::select! {
                    received = sigint.recv() => received.map(|()| ShutdownSignal::Interrupt),
                    received = sigterm.recv() => received.map(|()| ShutdownSignal::Terminate),
                };

                #[cfg(not(unix))]
                let signal = ctrl_c_signal(tokio::signal::ctrl_c().await);

                let Some(signal) = signal else {
                    tracing::error!("shutdown signal stream closed; signal task exiting");
                    return;
                };

                match action_for_signal(&handle, signal) {
                    SignalAction::Shutdown => {
                        tracing::info!(signal = signal.name(), "shutdown signal received");
                    }
                    SignalAction::Exit(exit_code) => {
                        tracing::warn!(
                            signal = signal.name(),
                            exit_code,
                            "repeated shutdown signal received; forcing exit"
                        );
                        std::process::exit(exit_code);
                    }
                }
            }
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

#[cfg(test)]
mod tests {
    use std::io;

    use super::{ShutdownHandle, ShutdownSignal, SignalAction, action_for_signal, ctrl_c_signal};

    #[test]
    fn first_signal_requests_shutdown() {
        let handle = ShutdownHandle::new();

        assert_eq!(
            action_for_signal(&handle, ShutdownSignal::Interrupt),
            SignalAction::Shutdown
        );
        assert!(handle.is_shutting_down());
    }

    #[test]
    fn signal_during_shutdown_requests_conventional_exit() {
        let handle = ShutdownHandle::new();
        handle.shutdown();

        assert_eq!(
            action_for_signal(&handle, ShutdownSignal::Interrupt),
            SignalAction::Exit(130)
        );

        #[cfg(unix)]
        assert_eq!(
            action_for_signal(&handle, ShutdownSignal::Terminate),
            SignalAction::Exit(143)
        );
    }

    #[test]
    fn ctrl_c_registration_errors_are_not_signals() {
        let error = io::Error::other("registration failed");

        assert_eq!(ctrl_c_signal(Err(error)), None);
        assert_eq!(ctrl_c_signal(Ok(())), Some(ShutdownSignal::Interrupt));
    }
}
