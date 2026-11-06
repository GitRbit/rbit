//! SSH-specific descendant ownership; closed admission and drain are separate steps.

use std::{
    future::Future,
    sync::{Arc, Mutex, PoisonError},
};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::{
    sync::CancellationToken,
    task::{TaskTracker, task_tracker::TaskTrackerToken},
};

/// One server's descendant scope, including russh's internally spawned handlers.
#[derive(Debug, Clone)]
pub(crate) struct SshTasks {
    admission: Arc<Mutex<bool>>,
    tracker: TaskTracker,
    stop: CancellationToken,
}

impl SshTasks {
    pub(crate) fn new() -> Self {
        Self { admission: Arc::new(Mutex::new(true)), tracker: TaskTracker::new(), stop: CancellationToken::new() }
    }

    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.stop.clone()
    }

    pub(crate) fn handler_token(&self) -> Option<TaskTrackerToken> {
        let open = self.admission.lock().unwrap_or_else(PoisonError::into_inner);
        open.then(|| self.tracker.token())
    }

    pub(crate) fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) -> Option<JoinHandle<()>> {
        let open = self.admission.lock().unwrap_or_else(PoisonError::into_inner);
        if !*open {
            return None;
        }
        Some(self.tracker.spawn(future))
    }

    pub(crate) fn reply(&self, future: impl Future<Output = ()> + Send + 'static) {
        let stop = self.cancellation();
        let _ = self.spawn(async move {
            tokio::select! { biased; () = stop.cancelled() => {}, () = future => {} }
        });
    }

    pub(crate) fn connection(&self, set: &mut JoinSet<()>, future: impl Future<Output = ()> + Send + 'static) {
        let open = self.admission.lock().unwrap_or_else(PoisonError::into_inner);
        if *open {
            set.spawn(self.tracker.track_future(future));
        }
    }

    /// Called only by an already-counted connection, before it can drop its token.
    /// Adopt russh's existing task even during shutdown; its join must not detach.
    pub(crate) fn session(&self, future: impl Future<Output = ()> + Send + 'static) {
        // The tracker retains drain ownership after this scheduling handle is dropped.
        drop(self.tracker.spawn(future));
    }

    pub(crate) fn stop(&self) {
        self.stop.cancel();
    }

    pub(crate) fn close(&self) {
        let mut open = self.admission.lock().unwrap_or_else(PoisonError::into_inner);
        *open = false;
        self.stop.cancel();
        self.tracker.close();
    }

    pub(crate) async fn wait(&self) {
        self.tracker.wait().await;
    }

    /// Makes listener panic/drop stop descendants too. Never claims they drained.
    pub(crate) fn stop_on_drop(&self) -> StopOnDrop {
        StopOnDrop(self.clone())
    }
}

pub(crate) struct StopOnDrop(SshTasks);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.close();
    }
}

#[cfg(test)]
mod tests;
