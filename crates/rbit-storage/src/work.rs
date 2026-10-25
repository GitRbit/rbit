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
/// onto other tasks is not counted). A nested call counts its future only for itself.
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
