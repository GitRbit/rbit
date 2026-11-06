//! Counting the storage work one future does, for regression tests that pin how many
//! statements a page takes (`rbit-web/tests/work_counts.rs`).
//!
//! The counter lives in a Tokio task-local set by [`count_work`]; outside it, counting
//! is one failed task-local lookup per acquisition, statement or cache write.

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// The storage work a future did, counted by [`count_work`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Pooled `SQLite` connections taken: one per statement run on the pool, one per
    /// held connection or transaction.
    pub acquisitions: u64,
    /// `SQLite` statements run, on the pool or on a held connection or transaction
    /// (`BEGIN` and `COMMIT` are not counted).
    pub statements: u64,
    /// redb write transactions of the insights cache (`put_*`).
    pub cache_writes: u64,
}

#[derive(Debug, Default)]
struct Counter {
    acquisitions: AtomicU64,
    statements: AtomicU64,
    cache_writes: AtomicU64,
}

tokio::task_local! {
    static COUNTER: Arc<Counter>;
}

/// Runs `future`, counting the storage work it does on its own task (work it spawns
/// onto other tasks is not counted unless explicitly carried with [`capture_work`]).
/// A nested call counts its future only for itself.
pub async fn count_work<F: Future>(future: F) -> (F::Output, Work) {
    let counter = Arc::new(Counter::default());
    let output = COUNTER.scope(Arc::clone(&counter), future).await;
    let work = Work {
        acquisitions: counter.acquisitions.load(Ordering::Relaxed),
        statements: counter.statements.load(Ordering::Relaxed),
        cache_writes: counter.cache_writes.load(Ordering::Relaxed),
    };
    (output, work)
}

fn add(field: impl FnOnce(&Counter) -> &AtomicU64) {
    let _ = COUNTER.try_with(|counter| field(counter).fetch_add(1, Ordering::Relaxed));
}

pub(crate) fn acquisition() {
    add(|counter| &counter.acquisitions);
}

pub(crate) fn statement() {
    add(|counter| &counter.statements);
}

pub(crate) fn cache_write() {
    add(|counter| &counter.cache_writes);
}

/// Explicit instrumentation handoff, not ownership of business work. It carries only
/// counters, never a service, task handle, credentials or request context.
#[derive(Clone, Debug)]
pub struct CapturedWork(Option<Arc<Counter>>);

/// Capture the current measurement before submitting explicitly owned child work.
/// Outside count_work this performs no allocation and preserves no business owner.
pub fn capture_work() -> CapturedWork {
    CapturedWork(COUNTER.try_with(Arc::clone).ok())
}
impl CapturedWork {
    pub async fn scope<F: Future>(self, future: F) -> F::Output {
        match self.0 {
            Some(counter) => COUNTER.scope(counter, future).await,
            None => future.await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn explicit_owned_handoff_counts_actual_work_without_inheriting_unrelated_tasks() {
        let ((), work) = count_work(async {
            tokio::spawn(async {
                acquisition();
                statement();
            })
            .await
            .unwrap();
            let captured = capture_work();
            tokio::spawn(captured.scope(async {
                acquisition();
                statement();
                statement();
            }))
            .await
            .unwrap();
        })
        .await;
        assert_eq!(work, Work { acquisitions: 1, statements: 2, cache_writes: 0 });
    }
}
