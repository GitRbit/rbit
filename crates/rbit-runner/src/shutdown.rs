//! Graceful shutdown (§12.5.4 "Shutdown").
//!
//! The first SIGTERM or Ctrl-C (on Windows also closing the console or shutting the system
//! down) stops polling: the runner takes no new task. Running tasks get `shutdown_grace`
//! to finish; then they are cancelled, which reports them as cancelled to the server and
//! runs their `always()` steps within the cancel budget. A second signal cancels them at
//! once.

use std::{future::Future, time::Duration};

use tokio_util::sync::CancellationToken;

/// What a shutdown does, in two steps.
#[derive(Debug, Clone, Default)]
pub struct Shutdown {
    /// Fires at the first signal: no new tasks.
    pub stop_polling: CancellationToken,
    /// Fires after the grace period or at the second signal: cancel running tasks.
    pub cancel_tasks: CancellationToken,
}

impl Shutdown {
    /// Listens for the process's shutdown signals.
    pub fn listen(grace: Duration) -> Self {
        let shutdown = Self::default();
        let tokens = shutdown.clone();
        tokio::spawn(async move {
            let mut signals = Signals::new();
            signals.next().await;
            tokens.drive(async {}, grace, signals.next()).await;
        });
        shutdown
    }

    /// Fires the tokens as `first` and `second` complete, with `grace` in between.
    async fn drive(&self, first: impl Future<Output = ()>, grace: Duration, second: impl Future<Output = ()>) {
        first.await;
        tracing::info!(grace = ?grace, "stopping: no new tasks are taken; running tasks get the grace period (signal again to cancel them now)");
        self.stop_polling.cancel();
        tokio::select! {
            () = second => tracing::warn!("second signal: cancelling running tasks"),
            () = tokio::time::sleep(grace) => tracing::warn!("the shutdown grace period is over: cancelling running tasks"),
        }
        self.cancel_tasks.cancel();
    }
}

/// The signals that ask the runner to stop.
struct Signals {
    #[cfg(unix)]
    terminate: Option<tokio::signal::unix::Signal>,
    #[cfg(windows)]
    close: Option<tokio::signal::windows::CtrlClose>,
    #[cfg(windows)]
    shutdown: Option<tokio::signal::windows::CtrlShutdown>,
}

impl Signals {
    fn new() -> Self {
        Self {
            #[cfg(unix)]
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok(),
            #[cfg(windows)]
            close: tokio::signal::windows::ctrl_close().ok(),
            #[cfg(windows)]
            shutdown: tokio::signal::windows::ctrl_shutdown().ok(),
        }
    }

    /// The next signal of any kind.
    async fn next(&mut self) {
        #[cfg(unix)]
        let platform = async {
            match &mut self.terminate {
                Some(terminate) => _ = terminate.recv().await,
                None => std::future::pending().await,
            }
        };
        #[cfg(windows)]
        let platform = async {
            let close = async {
                match &mut self.close {
                    Some(close) => _ = close.recv().await,
                    None => std::future::pending().await,
                }
            };
            let shutdown = async {
                match &mut self.shutdown {
                    Some(shutdown) => _ = shutdown.recv().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = close => {}
                () = shutdown => {}
            }
        };
        #[cfg(not(any(unix, windows)))]
        let platform = std::future::pending::<()>();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = platform => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_grace_period_follows_the_first_signal() {
        let shutdown = Shutdown::default();
        let (first, first_rx) = tokio::sync::oneshot::channel::<()>();
        let driver = {
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                shutdown.drive(async { _ = first_rx.await }, Duration::from_millis(200), std::future::pending()).await;
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!shutdown.stop_polling.is_cancelled());
        first.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), shutdown.stop_polling.cancelled()).await.unwrap();
        assert!(!shutdown.cancel_tasks.is_cancelled(), "running tasks get the grace period");
        tokio::time::timeout(Duration::from_secs(2), shutdown.cancel_tasks.cancelled()).await.unwrap();
        driver.await.unwrap();
    }

    #[tokio::test]
    async fn a_second_signal_cancels_at_once() {
        let shutdown = Shutdown::default();
        let started = tokio::time::Instant::now();
        shutdown.drive(async {}, Duration::from_secs(600), async {}).await;
        assert!(shutdown.stop_polling.is_cancelled() && shutdown.cancel_tasks.is_cancelled());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
