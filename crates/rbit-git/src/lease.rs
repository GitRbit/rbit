//! Coordinating work inside a repository's directory with moving that directory.
//!
//! Windows cannot rename a directory while any file inside it is open, and gitoxide keeps
//! pack and index files open (memory-mapped) for as long as a repository handle lives. So
//! a transfer that starts while a request or a background job is reading the repository
//! fails with "Access is denied" — typically right after a push, when the cache warm-up
//! triggered by the push is still computing language statistics.
//!
//! Work inside a repository's directory therefore holds a shared [`RepositoryLease`] for
//! its duration: every [`RepositoryReader`] operation does so automatically when the
//! reader came from [`RepositoryStorage::reader`], and a pinned reader holds one for as
//! long as its opened repository lives. Moving the directory waits until no lease is held
//! and keeps new ones waiting until the move is done.
//!
//! New leases wait only for a move that is actually under way, never for one that is
//! merely waiting. Leases may therefore be taken while holding another (a pinned page
//! reader next to a plain one, say) without any risk of deadlock. In exchange a steady
//! stream of overlapping work could keep a move waiting, so a move waits at most
//! [`MAX_WAIT`] and then goes ahead regardless (the rename is then retried and may fail,
//! which the transfer rolls back cleanly).
//!
//! [`RepositoryReader`]: crate::RepositoryReader
//! [`RepositoryStorage::reader`]: crate::RepositoryStorage::reader

use std::{
    collections::HashMap,
    pin::pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
    time::Duration,
};

use rbit_core::RepoId;
use tokio::sync::Notify;

/// The longest a move waits for work in progress before it goes ahead anyway.
pub const MAX_WAIT: Duration = Duration::from_secs(10);

/// One repository's lock: how many leases are held and whether a move is under way.
#[derive(Debug, Default)]
pub(crate) struct RepositoryLock {
    state: Mutex<LockState>,
    /// Woken whenever the last lease is released or a move ends.
    changed: Notify,
}

#[derive(Debug, Default)]
struct LockState {
    leases: usize,
    moving: bool,
}

impl RepositoryLock {
    fn state(&self) -> MutexGuard<'_, LockState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until `ready` holds for the state, then lets `take` update it.
    async fn wait_until(&self, ready: impl Fn(&LockState) -> bool, take: impl FnOnce(&mut LockState)) {
        loop {
            // Registered before the check, so a release in between is not missed.
            let mut changed = pin!(self.changed.notified());
            changed.as_mut().enable();
            {
                let mut state = self.state();
                if ready(&state) {
                    take(&mut state);
                    return;
                }
            }
            changed.await;
        }
    }

    /// A shared lease, once no move is under way.
    pub(crate) async fn shared(self: Arc<Self>) -> RepositoryLease {
        self.wait_until(|state| !state.moving, |state| state.leases += 1).await;
        RepositoryLease { lock: self }
    }

    /// Exclusive permission to move the repository, once no lease is held (or after
    /// `max_wait`, whichever comes first) and no other move is under way.
    async fn exclusive(self: Arc<Self>, max_wait: Duration) -> ExclusiveLease {
        let drained = self.wait_until(|state| !state.moving && state.leases == 0, |state| state.moving = true);
        if tokio::time::timeout(max_wait, drained).await.is_err() {
            tracing::warn!(leases = self.state().leases, "repository still in use after {max_wait:?}; moving it anyway");
            self.wait_until(|state| !state.moving, |state| state.moving = true).await;
        }
        ExclusiveLease { lock: self }
    }
}

/// Shared permission to work inside a repository's directory; it is not moved while
/// this is held. Dropping it releases the permission.
#[derive(Debug)]
pub struct RepositoryLease {
    lock: Arc<RepositoryLock>,
}

impl Drop for RepositoryLease {
    fn drop(&mut self) {
        let mut state = self.lock.state();
        state.leases -= 1;
        if state.leases == 0 {
            drop(state);
            self.lock.changed.notify_waiters();
        }
    }
}

/// Exclusive permission to move a repository's directory.
#[derive(Debug)]
pub(crate) struct ExclusiveLease {
    lock: Arc<RepositoryLock>,
}

impl Drop for ExclusiveLease {
    fn drop(&mut self) {
        self.lock.state().moving = false;
        self.lock.changed.notify_waiters();
    }
}

/// The per-repository locks of one storage root. Clones share the same locks.
#[derive(Debug, Clone, Default)]
pub(crate) struct RepositoryLocks {
    /// Locks currently in use. Entries whose lock nobody holds a reference to any more are
    /// dropped lazily, so the table stays as small as the set of busy repositories.
    locks: Arc<Mutex<HashMap<RepoId, Weak<RepositoryLock>>>>,
}

impl RepositoryLocks {
    /// The lock for `repo`, created on first use.
    pub(crate) fn lock_for(&self, repo: RepoId) -> Arc<RepositoryLock> {
        let mut locks = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(lock) = locks.get(&repo).and_then(Weak::upgrade) {
            return lock;
        }
        locks.retain(|_, lock| lock.strong_count() > 0);
        let lock = Arc::new(RepositoryLock::default());
        locks.insert(repo, Arc::downgrade(&lock));
        lock
    }

    pub(crate) async fn shared(&self, repo: RepoId) -> RepositoryLease {
        self.lock_for(repo).shared().await
    }

    pub(crate) async fn exclusive(&self, repo: RepoId) -> ExclusiveLease {
        self.lock_for(repo).exclusive(MAX_WAIT).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTLE: Duration = Duration::from_millis(50);
    const LONG: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn a_move_waits_for_leases_and_new_leases_wait_for_the_move() {
        let locks = RepositoryLocks::default();
        let first = locks.shared(RepoId(1)).await;
        let second = locks.shared(RepoId(1)).await;
        // Other repositories are independent.
        let _other = locks.exclusive(RepoId(2)).await;

        let waiting = tokio::spawn({
            let locks = locks.clone();
            async move { locks.exclusive(RepoId(1)).await }
        });
        tokio::time::sleep(SETTLE).await;
        assert!(!waiting.is_finished(), "the move waits while the repository is in use");
        drop(first);
        tokio::time::sleep(SETTLE).await;
        assert!(!waiting.is_finished(), "and until the last lease is gone");
        drop(second);
        let exclusive = tokio::time::timeout(LONG, waiting).await.unwrap().unwrap();

        let blocked = tokio::spawn({
            let locks = locks.clone();
            async move { locks.shared(RepoId(1)).await }
        });
        tokio::time::sleep(SETTLE).await;
        assert!(!blocked.is_finished(), "new work waits for the move under way");
        drop(exclusive);
        tokio::time::timeout(LONG, blocked).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_waiting_move_never_blocks_further_leases() {
        let locks = RepositoryLocks::default();
        let held = locks.shared(RepoId(1)).await;
        let waiting = tokio::spawn({
            let locks = locks.clone();
            async move { locks.exclusive(RepoId(1)).await }
        });
        tokio::time::sleep(SETTLE).await;
        // Work that already holds a lease takes another: no deadlock.
        let nested = tokio::time::timeout(LONG, locks.shared(RepoId(1))).await.expect("not blocked by the waiting move");
        drop(nested);
        drop(held);
        tokio::time::timeout(LONG, waiting).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_move_goes_ahead_after_waiting_its_limit() {
        let lock = Arc::new(RepositoryLock::default());
        let _stuck = Arc::clone(&lock).shared().await;
        let moving = tokio::time::timeout(LONG, Arc::clone(&lock).exclusive(SETTLE)).await.expect("the wait is bounded");
        assert!(lock.state().moving);
        drop(moving);
        assert!(!lock.state().moving);
    }

    #[tokio::test]
    async fn unused_locks_are_forgotten() {
        let locks = RepositoryLocks::default();
        drop(locks.shared(RepoId(1)).await);
        let _held = locks.shared(RepoId(2)).await;
        assert_eq!(locks.locks.lock().unwrap().len(), 1, "only the lock still in use remains");
    }
}
