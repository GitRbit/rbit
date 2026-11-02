//! Two command cells, retained on unknown; no SQL or external destructors under locks.
use super::{RepositoryCommandDrainError, RepositorySqlProgress};
use crate::{AdmissionReason, StorageError};
use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::{
    sync::Notify,
    time::{Instant, timeout_at},
};

const CAPACITY: usize = 2;
#[derive(Default)]
pub(in crate::sqlite) struct Coordinator {
    inner: Arc<Inner>,
}
impl Clone for Coordinator {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}
#[derive(Default)]
struct Registry {
    stopped: bool,
    unknown: bool,
    logical_failure: bool,
    cells: [Option<Arc<Cell>>; CAPACITY],
}
#[derive(Default)]
struct Inner {
    state: Mutex<Registry>,
    changed: Notify,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Life {
    Reserved,
    Acquiring,
    Ready,
    Busy,
    Terminal,
}
#[derive(Clone, Copy)]
pub(super) struct Status {
    pub life: Life,
    pub unknown: bool,
    pub logical_failure: bool,
    pub progress: RepositorySqlProgress,
}
pub(super) struct Cell {
    inner: Arc<Inner>,
    index: usize,
    state: Mutex<Status>,
}
impl fmt::Debug for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (state, newly_unknown) = match self.state.lock() {
            Ok(state) => (*state, false),
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                let newly_unknown = !state.unknown;
                state.unknown = true;
                (*state, newly_unknown)
            }
        };
        // Notify can synchronously invoke arbitrary registered Wakers.
        if newly_unknown {
            self.unknown();
        }
        f.debug_struct("RepositoryCommandCell")
            .field("index", &self.index)
            .field("life", &state.life)
            .field("unknown", &state.unknown)
            .field("progress", &state.progress)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for Coordinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = {
            let state = self.inner.lock();
            (state.stopped, state.cells.iter().flatten().count(), state.unknown)
        };
        f.debug_tuple("RepositoryCommandCoordinator").field(&snapshot).finish()
    }
}
impl Inner {
    fn lock(&self) -> MutexGuard<'_, Registry> {
        self.state.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.stopped = true;
            state.unknown = true;
            state
        })
    }
}
impl Coordinator {
    pub(super) fn reserve(&self) -> Result<Arc<Cell>, StorageError> {
        let mut state = self.inner.lock();
        if state.stopped {
            return Err(admission(AdmissionReason::Stopped));
        }
        let index = state.cells.iter().position(Option::is_none).ok_or_else(|| admission(AdmissionReason::Capacity))?;
        let cell = Arc::new(Cell {
            inner: self.inner.clone(),
            index,
            state: Mutex::new(Status {
                life: Life::Reserved,
                unknown: false,
                logical_failure: false,
                progress: RepositorySqlProgress::default(),
            }),
        });
        state.cells[index] = Some(cell.clone());
        Ok(cell)
    }
    pub(in crate::sqlite) fn stop(&self) {
        self.inner.lock().stopped = true;
        self.inner.changed.notify_waiters();
    }
    pub(in crate::sqlite) async fn drain(&self, deadline: Option<Instant>) -> Result<(), RepositoryCommandDrainError> {
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if deadline.is_some_and(|at| Instant::now() >= at) {
                return Err(RepositoryCommandDrainError::Deadline);
            }
            let (busy, unknown, failure) = {
                let state = self.inner.lock();
                let (mut busy, mut unknown) = (false, state.unknown);
                for cell in state.cells.iter().flatten() {
                    let selected = cell.lock();
                    busy |= matches!(selected.life, Life::Acquiring | Life::Busy) || (selected.life == Life::Ready && !selected.unknown);
                    unknown |= selected.unknown;
                }
                (busy, unknown, state.logical_failure)
            };
            if !busy {
                if unknown {
                    // Publish terminal poison to other parked drains outside both locks,
                    // even if this observer's own deadline has just elapsed.
                    self.inner.changed.notify_waiters();
                }
                if deadline.is_some_and(|at| Instant::now() >= at) {
                    return Err(RepositoryCommandDrainError::Deadline);
                }
                return if unknown {
                    Err(RepositoryCommandDrainError::Unconfirmed)
                } else if failure {
                    Err(RepositoryCommandDrainError::FailureRecorded)
                } else {
                    Ok(())
                };
            }
            if let Some(at) = deadline {
                timeout_at(at, changed).await.map_err(|_| RepositoryCommandDrainError::Deadline)?;
            } else {
                changed.await;
            }
        }
    }
}
pub(super) fn admission(reason: AdmissionReason) -> StorageError {
    StorageError::AdmissionUnavailable { scope: "repository command", reason }
}
impl Cell {
    pub(super) fn lock(&self) -> MutexGuard<'_, Status> {
        self.state.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.unknown = true;
            state
        })
    }
    pub(super) fn start(&self) -> Result<(), StorageError> {
        let registry = self.inner.lock();
        if registry.stopped {
            return Err(admission(AdmissionReason::Stopped));
        }
        let mut selected = self.lock();
        if selected.life != Life::Reserved || selected.unknown {
            return Err(StorageError::Invariant("repository reservation state"));
        }
        selected.life = Life::Acquiring;
        Ok(())
    }
    pub(super) fn unknown(&self) {
        // Uniform registry -> cell order; never hold cell while acquiring registry.
        let mut registry = self.inner.lock();
        registry.unknown = true;
        self.lock().unknown = true;
        drop(registry);
        self.inner.changed.notify_waiters();
    }
    pub(super) fn changed(&self) {
        self.inner.changed.notify_waiters();
    }
    pub(super) fn release(&self, failure: bool) -> bool {
        let removed = {
            let mut registry = self.inner.lock();
            let mut selected = self.lock();
            if selected.unknown {
                registry.unknown = true;
                selected.life = Life::Ready;
                drop(selected);
                drop(registry);
                self.changed();
                return false;
            }
            selected.life = Life::Terminal;
            registry.logical_failure |= failure;
            registry.cells[self.index].take()
        };
        drop(removed);
        self.changed();
        true
    }
    pub(super) fn finish_phase(&self) {
        self.lock().life = Life::Ready;
        self.changed();
    }
}

pub(super) struct Acquisition {
    pub cell: Arc<Cell>,
    pub acquired: bool,
    pub done: bool,
}
impl Drop for Acquisition {
    fn drop(&mut self) {
        if !self.done {
            if self.acquired {
                self.cell.unknown();
                self.cell.finish_phase();
            } else {
                self.cell.release(false);
            }
        }
    }
}
pub(super) struct PhaseOwner {
    pub cell: Arc<Cell>,
    pub done: bool,
}
impl Drop for PhaseOwner {
    fn drop(&mut self) {
        if !self.done {
            self.cell.unknown();
            self.cell.finish_phase();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Database,
        sqlite::repository_commands::{RepositoryDeleteSource, RepositoryPlan},
    };
    use rbit_core::RepoId;

    #[tokio::test]
    async fn successful_terminal_breaks_cycle_and_unknown_survives_last_database_handle() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("clean.db")).await.unwrap();
        let factory = db.repository_commands();
        let weak = Arc::downgrade(&factory.coordinator.inner);
        let admitted = factory
            .reserve(RepositoryPlan::Delete { repo: RepoId(-1), via: RepositoryDeleteSource::Repositories })
            .unwrap()
            .acquire()
            .await
            .unwrap();
        admitted.command.cancel_before_effects(admitted.owner).await.unwrap();
        drop(factory);
        drop(db);
        assert!(weak.upgrade().is_none());
        let db = Database::open(&directory.path().join("unknown.db")).await.unwrap();
        let factory = db.repository_commands();
        let weak = Arc::downgrade(&factory.coordinator.inner);
        let admitted = factory
            .reserve(RepositoryPlan::Delete { repo: RepoId(-1), via: RepositoryDeleteSource::Settings })
            .unwrap()
            .acquire()
            .await
            .unwrap();
        drop(admitted.owner);
        drop(admitted.command);
        drop(factory);
        drop(db);
        assert!(weak.upgrade().is_some());
    }
    #[tokio::test]
    async fn observed_cell_poison_wakes_actual_waiting_drain_without_waiting_for_deadline() {
        use crate::sqlite::repository_commands::{AdmittedRepositoryCommand, RepositoryPhaseResult};
        use std::{
            future::{Future, poll_fn},
            panic::{AssertUnwindSafe, catch_unwind},
            task::Poll,
            time::Duration,
        };
        use tokio::{sync::oneshot, time::timeout};
        const WAIT: Duration = Duration::from_secs(2);

        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("poison.db")).await.unwrap();
        let AdmittedRepositoryCommand { mut command, owner } = db
            .repository_commands()
            .reserve(RepositoryPlan::Delete { repo: RepoId(-1), via: RepositoryDeleteSource::Repositories })
            .unwrap()
            .acquire()
            .await
            .unwrap();
        let cell = command.cell.clone();
        let draining_db = db.clone();
        let (entered, entry) = oneshot::channel();
        let drain = tokio::spawn(async move {
            let mut future = Box::pin(draining_db.drain_repository_commands(Instant::now() + WAIT));
            let mut entered = Some(entered);
            poll_fn(|cx| {
                assert!(future.as_mut().poll(cx).is_pending());
                entered.take().unwrap().send(()).unwrap();
                Poll::Ready(())
            })
            .await;
            future.await
        });
        timeout(WAIT, entry).await.unwrap().unwrap();
        let poison = catch_unwind(AssertUnwindSafe(|| {
            let _selected = cell.state.lock().unwrap();
            panic!("injected idle repository cell mutex poison");
        }));
        assert!(poison.is_err());
        assert!(cell.state.is_poisoned());
        let snapshot = format!("{cell:?}"); // Actual public Debug observes poison and must wake the parked task.
        assert!(snapshot.contains("unknown: true"));
        assert_eq!(timeout(WAIT, drain).await.unwrap().unwrap(), Err(RepositoryCommandDrainError::Unconfirmed));
        let (result, work) = crate::count_work(command.execute_next()).await;
        assert!(matches!(result, RepositoryPhaseResult::Unconfirmed { .. }));
        assert_eq!(work, crate::Work { acquisitions: 0, statements: 0, cache_writes: 0 });
        drop(command);
        drop(owner);
        timeout(WAIT, db.close()).await.unwrap();
    }
    #[tokio::test]
    async fn observed_poison_notifies_registered_drain_waker_outside_both_state_locks() {
        use std::{
            future::Future,
            panic::{AssertUnwindSafe, catch_unwind},
            sync::{
                TryLockError,
                atomic::{AtomicBool, AtomicUsize, Ordering},
            },
            task::{Context, Wake, Waker},
            time::Duration,
        };
        struct Probe {
            cell: Arc<Cell>,
            wakes: AtomicUsize,
            held: AtomicBool,
        }
        fn available<T>(lock: &Mutex<T>) -> bool {
            match lock.try_lock() {
                Ok(guard) => {
                    drop(guard);
                    true
                }
                Err(TryLockError::Poisoned(poisoned)) => {
                    drop(poisoned.into_inner());
                    true
                }
                Err(TryLockError::WouldBlock) => false,
            }
        }
        impl Wake for Probe {
            fn wake(self: Arc<Self>) {
                self.wake_by_ref();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                // Record without blocking or panicking inside the synchronous Waker.
                let cell_available = available(&self.cell.state);
                let registry_available = available(&self.cell.inner.state);
                self.held.fetch_or(!cell_available || !registry_available, Ordering::SeqCst);
                self.wakes.fetch_add(1, Ordering::SeqCst);
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("waker.db")).await.unwrap();
        let admitted = db
            .repository_commands()
            .reserve(RepositoryPlan::Delete { repo: RepoId(-1), via: RepositoryDeleteSource::Repositories })
            .unwrap()
            .acquire()
            .await
            .unwrap();
        let cell = admitted.command.cell.clone();
        let probe = Arc::new(Probe { cell: cell.clone(), wakes: AtomicUsize::new(0), held: AtomicBool::new(false) });
        let waker = Waker::from(probe.clone());
        let mut drain = Box::pin(db.drain_repository_commands(Instant::now() + Duration::from_secs(2)));
        assert!(drain.as_mut().poll(&mut Context::from_waker(&waker)).is_pending());
        assert_eq!(probe.wakes.load(Ordering::SeqCst), 0);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _selected = cell.state.lock().unwrap();
                panic!("injected idle cell poison for registered Waker");
            }))
            .is_err()
        );
        assert!(format!("{cell:?}").contains("unknown: true"));
        assert!(probe.wakes.load(Ordering::SeqCst) > 0);
        assert!(!probe.held.load(Ordering::SeqCst));
        assert_eq!(drain.await, Err(RepositoryCommandDrainError::Unconfirmed));
        drop(admitted);
        db.close().await;
    }
}
