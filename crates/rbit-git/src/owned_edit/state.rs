//! Fixed two-cell custody, intentionally retained on unknown ownership.
use super::{GitAdmissionError, WorkerLoss};
use crate::lease::ExclusiveLease;
use std::{
    any::Any,
    fmt,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{runtime::Handle, sync::Notify};

pub(super) const CAPACITY: usize = 2;
#[derive(Default)]
pub(crate) struct Registry {
    state: Mutex<RegistryState>,
    pub(super) changed: Notify,
    unknown: AtomicBool,
}
#[derive(Default)]
struct RegistryState {
    stopped: bool,
    poisoned: bool,
    slots: [Option<Arc<Custody>>; CAPACITY],
}
impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (stopped, occupied, unknown) = self.snapshot();
        f.debug_struct("OwnedEditRegistry").field("stopped", &stopped).field("occupied", &occupied).field("unknown", &unknown).finish()
    }
}
impl Registry {
    fn lock(&self) -> MutexGuard<'_, RegistryState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                self.unknown.store(true, Ordering::Release);
                let mut state = poisoned.into_inner();
                state.stopped = true;
                state.poisoned = true;
                state
            }
        }
    }
    pub(super) fn reserve(self: &Arc<Self>, runtime: Handle) -> Result<Arc<Custody>, GitAdmissionError> {
        let mut state = self.lock();
        if state.poisoned {
            return Err(GitAdmissionError::CoordinatorUnavailable);
        }
        if state.stopped {
            return Err(GitAdmissionError::Stopped);
        }
        let index = state.slots.iter().position(Option::is_none).ok_or(GitAdmissionError::Capacity)?;
        // This deliberate registry <-> custody retention is broken only by clean release.
        // There are at most two cells; unknown cells have no public release API.
        let custody = Arc::new(Custody { registry: self.clone(), index, runtime, state: Mutex::default() });
        state.slots[index] = Some(custody.clone());
        Ok(custody)
    }
    pub(super) fn attach(&self, custody: &Custody) -> Result<(), GitAdmissionError> {
        let state = self.lock();
        if state.poisoned {
            return Err(GitAdmissionError::CoordinatorUnavailable);
        }
        if state.stopped {
            return Err(GitAdmissionError::Stopped);
        }
        let mut selected = custody.lock();
        if selected.unknown.is_some()
            || selected.released
            || selected.attached
            || selected.worker != WorkerState::Idle
            || selected.lease.is_some()
            || selected.retained.is_some()
        {
            return Err(GitAdmissionError::CoordinatorUnavailable);
        }
        selected.attached = true;
        Ok(())
    }
    pub(super) fn stop(&self) {
        self.lock().stopped = true;
        self.changed.notify_waiters();
    }
    pub(super) fn snapshot(&self) -> (bool, usize, bool) {
        let state = self.lock();
        (state.stopped, state.slots.iter().flatten().count(), self.is_unknown())
    }
    pub(super) fn is_unknown(&self) -> bool {
        self.unknown.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum WorkerState {
    #[default]
    Idle,
    Scheduled,
    Running,
    Finished,
}
#[derive(Default)]
struct CustodyState {
    attached: bool,
    released: bool,
    unknown: Option<WorkerLoss>,
    worker: WorkerState,
    lease: Option<ExclusiveLease>,
    // Only explicit quarantine of a recoverable envelope retains K. Panic/payload
    // destruction may drop K; the lease and finite credit still remain in this cell.
    retained: Option<Box<dyn Any + Send>>,
}
pub(super) struct Custody {
    pub(super) registry: Arc<Registry>,
    pub(super) index: usize,
    pub(super) runtime: Handle,
    state: Mutex<CustodyState>,
}
impl fmt::Debug for Custody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (unknown, worker) = {
            let state = self.lock();
            (state.unknown, state.worker)
        };
        f.debug_struct("EditCustody")
            .field("index", &self.index)
            .field("unknown", &unknown)
            .field("worker", &worker)
            .finish_non_exhaustive()
    }
}
impl Custody {
    fn lock(&self) -> MutexGuard<'_, CustodyState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                self.registry.unknown.store(true, Ordering::Release);
                let mut state = poisoned.into_inner();
                state.unknown.get_or_insert(WorkerLoss::StateUnavailable);
                state
            }
        }
    }
    pub(super) fn unknown(&self, loss: WorkerLoss) {
        self.lock().unknown.get_or_insert(loss);
        self.registry.unknown.store(true, Ordering::Release);
        self.registry.changed.notify_waiters();
    }
    pub(super) fn loss(&self) -> Option<WorkerLoss> {
        self.lock().unknown
    }
    pub(super) fn has_lease(&self) -> bool {
        self.lock().lease.is_some()
    }
    pub(super) fn install_lease(&self, lease: ExclusiveLease) {
        let previous = self.lock().lease.replace(lease);
        if previous.is_some() {
            self.unknown(WorkerLoss::StateUnavailable);
        }
        drop(previous);
    }
    pub(super) fn worker(&self, state: WorkerState) {
        self.lock().worker = state;
        self.registry.changed.notify_waiters();
    }
    pub(super) fn retain<K: Send + 'static>(&self, context: Box<K>) {
        self.unknown(WorkerLoss::OwnerLost);
        // Unsizing the already allocated box does not allocate payload storage.
        let erased: Box<dyn Any + Send> = context;
        let previous = self.lock().retained.replace(erased);
        if previous.is_some() {
            self.unknown(WorkerLoss::StateUnavailable);
        }
        drop(previous); // Never run an arbitrary K destructor under a state mutex.
    }
    pub(super) fn release(&self, reserved_only: bool) -> bool {
        let removed;
        let lease;
        {
            // Same ordering as attach: registry -> custody. No destructor under either.
            let mut registry = self.registry.lock();
            let mut state = self.lock();
            if registry.poisoned
                || state.unknown.is_some()
                || state.released
                || matches!(state.worker, WorkerState::Scheduled | WorkerState::Running)
                || (reserved_only && state.attached)
            {
                return false;
            }
            state.released = true;
            lease = state.lease.take();
            removed = registry.slots[self.index].take();
        }
        drop(lease);
        drop(removed);
        self.registry.changed.notify_waiters();
        true
    }
}

pub(super) struct WorkerActivity(Arc<Custody>);
impl WorkerActivity {
    pub(super) fn start(custody: Arc<Custody>) -> Self {
        custody.worker(WorkerState::Running);
        Self(custody)
    }
}
impl Drop for WorkerActivity {
    fn drop(&mut self) {
        self.0.worker(WorkerState::Finished);
    }
}
pub(super) struct Owner {
    custody: Arc<Custody>,
    acknowledged: bool,
}
impl Owner {
    pub(super) fn new(custody: Arc<Custody>) -> Self {
        Self { custody, acknowledged: false }
    }
    pub(super) fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.acknowledged {
            self.custody.unknown(WorkerLoss::OwnerLost);
        }
    }
}

#[cfg(test)]
impl Custody {
    pub(super) fn worker_finished(&self) -> bool {
        self.lock().worker == WorkerState::Finished
    }
}

#[cfg(test)]
mod tests {
    use crate::{RepositoryStorage, owned_edit::GitAdmissionError};
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::Arc,
    };

    #[tokio::test]
    async fn poisoned_selected_cell_refuses_attach_and_returns_original_context() {
        let directory = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(directory.path());
        let reservation = storage.reserve_edit().unwrap();
        let custody = reservation.custody.as_ref().unwrap().clone();
        let poison = catch_unwind(AssertUnwindSafe(|| {
            let _selected = custody.state.lock().unwrap();
            panic!("injected selected custody mutex poison");
        }));
        assert!(poison.is_err());
        assert!(custody.state.is_poisoned());
        let context = Box::new(String::from("original opaque context"));
        let address = std::ptr::from_ref(&*context);
        let refused = reservation.attach(context).unwrap_err();
        assert_eq!(refused.reason, GitAdmissionError::CoordinatorUnavailable);
        assert_eq!(std::ptr::from_ref(&*refused.context), address);
        assert_eq!(storage.owned_edits.snapshot(), (false, 1, true));
        assert!(!directory.path().join("1").exists());
        drop(refused);
        drop(custody);
        let weak = Arc::downgrade(&storage.owned_edits);
        drop(storage);
        assert!(weak.upgrade().is_some(), "unknown selected cell cannot release cleanly");
    }

    #[tokio::test]
    async fn explicit_quarantine_moves_existing_box_to_erased_payload_without_reboxing() {
        let directory = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(directory.path());
        let mut command = storage.reserve_edit().unwrap().attach(String::from("preboxed payload")).unwrap();
        let original = std::ptr::from_mut(command.context_mut()).addr();
        let custody = command.core.custody.clone();
        let retained = command.quarantine();
        let erased = {
            let state = custody.state.lock().unwrap();
            let payload = state.retained.as_ref().unwrap().downcast_ref::<String>().unwrap();
            std::ptr::from_ref(payload).addr()
        };
        assert_eq!(erased, original, "same preallocated Box payload address survives Any coercion");
        drop(retained);
        assert_eq!(storage.owned_edits.snapshot(), (false, 1, true));
    }
}
