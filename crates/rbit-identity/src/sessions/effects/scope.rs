//! Per-instance ownership for rare password/session finalization; never a request registry.

use std::{
    fmt,
    future::Future,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::task::JoinHandle;
use tokio_util::task::{TaskTracker, task_tracker::TaskTrackerToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEffectsError {
    Closed,
    NoRuntime,
    WrongScope,
}
impl fmt::Display for SessionEffectsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Closed => "session finalization admission is closed",
            Self::NoRuntime => "session finalization has no runtime",
            Self::WrongScope => "session finalization belongs to another instance",
        })
    }
}
impl std::error::Error for SessionEffectsError {}

#[derive(Debug, Default)]
pub(super) struct Statistics {
    closed: AtomicBool,
    completed: AtomicU64,
    returned_errors: AtomicU64,
    abnormal: AtomicU64,
}

/// Snapshot only. Ownership units include admitted guards/leases and tracked tasks;
/// they are not an independently measured task count or an entire-feature health check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionEffectsReport {
    pub closed: bool,
    pub active_owners: usize,
    pub completed: u64,
    pub returned_errors: u64,
    pub abnormal: u64,
}
impl SessionEffectsReport {
    pub fn drained(self) -> bool {
        self.closed && self.active_owners == 0
    }
}

#[derive(Debug)]
struct Inner {
    gate: Mutex<bool>,
    tasks: TaskTracker,
    statistics: Arc<Statistics>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.statistics.closed.store(true, Ordering::SeqCst);
        self.tasks.close();
    }
}

#[derive(Debug, Clone)]
pub struct SessionEffectScope(Arc<Inner>);
impl SessionEffectScope {
    pub fn new() -> Self {
        Self(Arc::new(Inner { gate: Mutex::new(false), tasks: TaskTracker::new(), statistics: Arc::default() }))
    }
    /// Reserve synchronously before the first async wait of a mutation or guard.
    pub fn reserve(&self) -> Result<SessionMutationPermit, SessionEffectsError> {
        let closed = self.0.gate.lock().unwrap_or_else(PoisonError::into_inner);
        if *closed {
            return Err(SessionEffectsError::Closed);
        }
        Ok(SessionMutationPermit {
            tracker: self.0.tasks.clone(),
            ownership: self.0.tasks.token(),
            statistics: Arc::clone(&self.0.statistics),
        })
    }
    pub fn close(&self) {
        let mut closed = self.0.gate.lock().unwrap_or_else(PoisonError::into_inner);
        *closed = true;
        self.0.statistics.closed.store(true, Ordering::SeqCst);
        self.0.tasks.close();
    }
    pub fn report(&self) -> SessionEffectsReport {
        SessionEffectsReport {
            closed: self.0.statistics.closed.load(Ordering::SeqCst),
            active_owners: self.0.tasks.len(),
            completed: self.0.statistics.completed.load(Ordering::SeqCst),
            returned_errors: self.0.statistics.returned_errors.load(Ordering::SeqCst),
            abnormal: self.0.statistics.abnormal.load(Ordering::SeqCst),
        }
    }
    pub async fn shutdown(&self) -> SessionEffectsReport {
        self.close();
        self.0.tasks.wait().await;
        self.report()
    }
    pub(super) fn authority(&self) -> Arc<Statistics> {
        Arc::clone(&self.0.statistics)
    }
}
impl Default for SessionEffectScope {
    fn default() -> Self {
        Self::new()
    }
}

/// Non-cloneable admission. Dropping before submission simply abandons uncommitted work.
/// It owns tracker state/statistics only, never the supervisor or a broad service handle.
pub struct SessionMutationPermit {
    tracker: TaskTracker,
    ownership: TaskTrackerToken,
    statistics: Arc<Statistics>,
}
impl fmt::Debug for SessionMutationPermit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionMutationPermit").finish_non_exhaustive()
    }
}

/// Evidence of an admitted mutation, retained through the whole owned finalization.
/// Not cloneable; keeping it in a descendant preserves tracked ownership as well.
pub struct SessionMutationContext {
    _ownership: TaskTrackerToken,
    statistics: Arc<Statistics>,
}
impl fmt::Debug for SessionMutationContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionMutationContext").finish_non_exhaustive()
    }
}
impl SessionMutationContext {
    pub(super) fn belongs_to(&self, authority: &Arc<Statistics>) -> bool {
        Arc::ptr_eq(&self.statistics, authority)
    }
}

struct Completion {
    statistics: Arc<Statistics>,
    completed: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.completed {
            self.statistics.abnormal.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl SessionMutationPermit {
    /// Eagerly submits exactly one tracked finalization. Existing admission may adopt
    /// work after close; its token prevents wait/zero from racing this handoff.
    /// Dropping the returned `JoinHandle` does not cancel commit/revocation/audit.
    pub fn submit<E: Send + 'static, F, U>(self, update: U) -> Result<JoinHandle<Result<(), E>>, SessionEffectsError>
    where
        F: Future<Output = Result<(), E>> + Send + 'static,
        U: FnOnce(SessionMutationContext) -> F + Send + 'static,
    {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            self.statistics.abnormal.fetch_add(1, Ordering::SeqCst);
            SessionEffectsError::NoRuntime
        })?;
        let Self { tracker, ownership, statistics } = self;
        let context = SessionMutationContext { _ownership: ownership, statistics: Arc::clone(&statistics) };
        // Construct outside the async block so even an unpolled dropped task records loss.
        let completion = Completion { statistics: Arc::clone(&statistics), completed: false };
        Ok(tracker.spawn_on(
            async move {
                let mut completion = completion;
                let result = update(context).await;
                statistics.completed.fetch_add(1, Ordering::SeqCst);
                if result.is_err() {
                    statistics.returned_errors.fetch_add(1, Ordering::SeqCst);
                }
                completion.completed = true;
                result
            },
            &runtime,
        ))
    }
}
