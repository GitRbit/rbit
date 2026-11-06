//! Single-start cooperative background work with observable, cancellation-safe drain.

use std::{
    future::Future,
    sync::{Arc, Mutex, PoisonError},
};

use tokio_util::{sync::CancellationToken, task::TaskTracker};

/// Lifecycle of one statically composed background task, not feature readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// The owner has not started a task.
    Idle,
    Running,
    /// Cancellation was requested, but task completion has not been observed.
    Stopping,
    Stopped,
    /// The task unwound or was dropped before normal completion.
    Failed,
}

/// Read-only cooperative stop signal. Holding it does not keep its owner alive.
#[derive(Debug, Clone)]
pub struct TaskStop(CancellationToken);

impl TaskStop {
    /// Resolves once cancellation has been requested, including before this call.
    pub async fn cancelled(&self) {
        self.0.cancelled().await;
    }
}

/// Owns exactly one asynchronous background task across clones.
///
/// Start and stop are serialized; a stopped owner cannot be restarted. Tasks choose
/// cancellation-safe boundaries using [`TaskStop`], and finish in-flight effects
/// before returning. This primitive never aborts a worker or interprets a timeout as
/// completion. The host may time out waiting; later waits still observe the same task.
/// It stores no task registry, protocol policy, feature handle or dynamic dispatch.
#[derive(Debug, Clone)]
pub struct TaskLifecycle(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    gate: Mutex<()>,
    state: Arc<Mutex<TaskState>>,
    cancel: CancellationToken,
    tasks: TaskTracker,
}

impl TaskLifecycle {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            gate: Mutex::new(()),
            state: Arc::new(Mutex::new(TaskState::Idle)),
            cancel: CancellationToken::new(),
            tasks: TaskTracker::new(),
        }))
    }

    /// A snapshot for health reporting; it must not be used as a synchronization gate.
    pub fn state(&self) -> TaskState {
        *self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn stop_signal(&self) -> TaskStop {
        TaskStop(self.0.cancel.clone())
    }

    /// Starts once across clones; returns false after any prior start or stop.
    /// Rejected futures are dropped without being polled.
    ///
    /// # Panics
    /// Panics outside a Tokio runtime, without changing lifecycle state.
    pub fn start(&self, task: impl Future<Output = ()> + Send + 'static) -> bool {
        let _runtime = tokio::runtime::Handle::current();
        let _gate = self.0.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        if *state != TaskState::Idle {
            return false;
        }
        *state = TaskState::Running;
        drop(state);
        let completion = Completion { state: Arc::clone(&self.0.state), completed: false };
        self.0.tasks.spawn(async move {
            let mut completion = completion;
            task.await;
            completion.completed = true;
        });
        true
    }

    /// Stops new starts and signals cooperative cancellation without waiting.
    pub fn request_stop(&self) {
        let _gate = self.0.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        *state = match *state {
            TaskState::Idle => TaskState::Stopped,
            TaskState::Running => TaskState::Stopping,
            other => other,
        };
        self.0.cancel.cancel();
        self.0.tasks.close();
    }

    /// Requests stop and waits for real task completion, even across concurrent callers.
    /// Cancelling this wait leaves task ownership and the stop request intact.
    /// No deadline is chosen here; a caller's timeout does not change `Stopping` to `Stopped`.
    pub async fn shutdown(&self) {
        self.request_stop();
        self.0.tasks.wait().await;
    }
}

impl Default for TaskLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.tasks.close();
    }
}

struct Completion {
    state: Arc<Mutex<TaskState>>,
    completed: bool,
}

impl Drop for Completion {
    fn drop(&mut self) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = if self.completed { TaskState::Stopped } else { TaskState::Failed };
    }
}
