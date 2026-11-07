//! Bounded fail-closed retention when native worker destruction is not proved.
use super::WikiCatalogHandle;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::Notify;

/// Overflow-refusing increment returning the previous value; an exhausted counter
/// is never wrapped and reports its current value as the error.
fn checked_increment(counter: &AtomicU64) -> Result<u64, u64> {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        let next = current.checked_add(1).ok_or(current)?;
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(previous) => return Ok(previous),
            Err(observed) => current = observed,
        }
    }
}

#[derive(Default)]
pub(super) struct NativeState {
    opened: AtomicU64,
    closed: AtomicU64,
    initializing: AtomicBool,
    uncertain: AtomicBool,
    changed: Notify,
}
impl NativeState {
    pub(super) fn is_uncertain(&self) -> bool {
        self.uncertain.load(Ordering::Acquire)
    }
    pub(super) fn fail(&self) {
        self.uncertain.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub(super) async fn unproved(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.is_uncertain() {
                return;
            }
            changed.await;
        }
    }
    pub(super) fn opened(&self) -> Result<(), sqlx::Error> {
        let previous = checked_increment(&self.opened);
        if self.is_uncertain() || previous.ok() != Some(self.closed.load(Ordering::Acquire)) {
            self.fail();
            return Err(sqlx::Error::Protocol("unaccounted wiki SQLite worker generation".into()));
        }
        Ok(())
    }
    pub(super) fn accounted(&self) -> bool {
        !self.is_uncertain()
            && !self.initializing.load(Ordering::Acquire)
            && self.opened.load(Ordering::Acquire) == self.closed.load(Ordering::Acquire)
    }
    pub(super) async fn close_physical(&self, connection: sqlx::pool::PoolConnection<sqlx::Sqlite>) -> Result<(), sqlx::Error> {
        use sqlx::Connection;
        // min_connections=0: detaching cannot launch another replacement worker.
        let result = connection.detach().close().await;
        match result {
            Ok(()) => {
                if checked_increment(&self.closed).is_err() {
                    self.fail();
                }
                Ok(())
            }
            Err(error) => {
                self.fail();
                Err(error)
            }
        }
    }
}

/// Setup cancellation can discard a worker before `after_connect` even observes it.
pub(super) struct InitializationAttempt {
    state: Arc<NativeState>,
    complete: bool,
}
impl InitializationAttempt {
    pub(super) fn begin(state: Arc<NativeState>) -> Self {
        if state.initializing.swap(true, Ordering::AcqRel) {
            state.fail();
        }
        Self { state, complete: false }
    }
    pub(super) fn complete(mut self) {
        self.complete = true;
    }
}
impl Drop for InitializationAttempt {
    fn drop(&mut self) {
        if !self.complete {
            self.state.fail();
        }
        self.state.initializing.store(false, Ordering::Release);
    }
}

/// Exactly one per consumed physical catalog lease. No registry/task-loop/reaper.
pub(super) struct NativeRetention {
    catalog: Option<WikiCatalogHandle>,
    verified: bool,
}
impl NativeRetention {
    pub(super) fn new(catalog: WikiCatalogHandle) -> Self {
        Self { catalog: Some(catalog), verified: false }
    }
    pub(super) fn verified(&mut self) {
        self.verified = true;
    }
}
impl Drop for NativeRetention {
    fn drop(&mut self) {
        if !self.verified
            && let Some(catalog) = self.catalog.take()
        {
            // Intentionally retain the actual exclusive file lease even across
            // supervisor/runtime abort. Only process exit can recover this
            // unknown native lifetime; another owner cannot allocate again.
            std::mem::forget(catalog);
        }
    }
}
