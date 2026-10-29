//! Graceful shutdown (§12.5.4 "Shutdown").
//!
//! The first SIGTERM or Ctrl-C (on Windows also closing the console, logging off or
//! shutting the system down), the Windows service manager's stop ([`request`]), or a stop
//! request in the work directory (`rbit-runner stop` of a logon task, see
//! [`crate::service::request_stop`]) stops polling: the runner takes no new task. Running
//! tasks get `shutdown_grace` to finish; then they are cancelled, which reports them as
//! cancelled to the server and runs their `always()` steps within the cancel budget. A
//! second signal or request cancels them at once.

use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Stop requests that come from neither a signal nor the console.
static REQUESTS: Notify = Notify::const_new();

/// Asks the runner to stop, as a signal does.
pub fn request() {
    REQUESTS.notify_one();
}

/// What a shutdown does, in two steps.
#[derive(Debug, Clone, Default)]
pub struct Shutdown {
    /// Fires at the first signal: no new tasks.
    pub stop_polling: CancellationToken,
    /// Fires after the grace period or at the second signal: cancel running tasks.
    pub cancel_tasks: CancellationToken,
}

impl Shutdown {
    /// Listens for the process's shutdown signals (with `logoff`, also the end of the
    /// desktop session it runs in) and for stop requests in `work_dir`.
    pub fn listen(grace: Duration, work_dir: &Path, logoff: bool) -> Self {
        let shutdown = Self::default();
        let tokens = shutdown.clone();
        tokio::spawn(async move {
            let mut signals = Signals::new(logoff);
            signals.next().await;
            tokens.drive(async {}, grace, signals.next()).await;
        });
        watch_stop_file(crate::service::stop_file(work_dir));
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

/// Turns `path` into stop requests: it is checked every two seconds, and holds `now` to
/// cancel running tasks at once. One left from before the runner started is removed.
fn watch_stop_file(path: PathBuf) {
    let _ = std::fs::remove_file(&path);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let taken = path.with_extension("taken");
            if std::fs::rename(&path, &taken).is_err() {
                continue;
            }
            let text = std::fs::read_to_string(&taken).unwrap_or_default();
            let _ = std::fs::remove_file(&taken);
            tracing::info!(file = %path.display(), "stop requested");
            request();
            if text.trim() == "now" {
                tokio::time::sleep(Duration::from_millis(100)).await;
                request();
            }
        }
    });
}

/// The signals that ask the runner to stop.
struct Signals {
    #[cfg(unix)]
    terminate: Option<tokio::signal::unix::Signal>,
    #[cfg(windows)]
    close: Option<tokio::signal::windows::CtrlClose>,
    #[cfg(windows)]
    shutdown: Option<tokio::signal::windows::CtrlShutdown>,
    #[cfg(windows)]
    logoff: Option<tokio::signal::windows::CtrlLogoff>,
}

impl Signals {
    /// A service is told of every user's logoff, and must not stop for it: `logoff` is
    /// for a runner in a desktop session.
    #[cfg_attr(not(windows), allow(unused_variables))]
    fn new(logoff: bool) -> Self {
        Self {
            #[cfg(unix)]
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok(),
            #[cfg(windows)]
            close: tokio::signal::windows::ctrl_close().ok(),
            #[cfg(windows)]
            shutdown: tokio::signal::windows::ctrl_shutdown().ok(),
            #[cfg(windows)]
            logoff: if logoff { tokio::signal::windows::ctrl_logoff().ok() } else { None },
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
            let logoff = async {
                match &mut self.logoff {
                    Some(logoff) => _ = logoff.recv().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = close => {}
                () = shutdown => {}
                () = logoff => {}
            }
        };
        #[cfg(not(any(unix, windows)))]
        let platform = std::future::pending::<()>();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = platform => {}
            () = REQUESTS.notified() => {}
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
    async fn a_stop_file_is_a_request() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(crate::service::stop_file(dir.path()), "stop").unwrap();
        let shutdown = Shutdown::listen(Duration::from_secs(600), dir.path(), false);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!shutdown.stop_polling.is_cancelled(), "a request left from before the start is dropped");
        crate::service::request_stop(dir.path(), true).unwrap();
        tokio::time::timeout(Duration::from_secs(10), shutdown.cancel_tasks.cancelled()).await.unwrap();
        assert!(shutdown.stop_polling.is_cancelled() && !crate::service::stop_file(dir.path()).exists());
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
