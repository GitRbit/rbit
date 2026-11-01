//! Typed, retained SQL-to-session continuation. No response ledger or service adoption.
//!
//! These application leases do not certify crash atomicity, physical ownership or
//! all driver settlement. Raw suspended enrollments never cross the Storage API.
use std::sync::atomic::{AtomicU8, Ordering};

use rbit_authority_fence::{BeginError, SuspendedMutation};

use super::{Arc, Inner, Mutation, ProfileStore, SessionStore, StorageError, WriterRegistration, oneshot};
use rbit_core::AccountId;

/// Independent of forced/reset capacity; shared by clones of one Database only.
#[derive(Debug, Clone, Default)]
pub(in crate::sqlite) struct SelfCoordinator {
    inner: Arc<Inner>,
}
impl SelfCoordinator {
    pub(in crate::sqlite) fn bind(
        &self,
        profiles: ProfileStore,
        sessions: SessionStore,
        writer: WriterRegistration,
    ) -> SelfPasswordChanges {
        SelfPasswordChanges { coordinator: self.clone(), profiles, sessions, writer }
    }
    pub(in crate::sqlite) fn stop(&self) {
        self.inner.state().stopped = true;
        self.inner.changed.notify_waiters();
    }
    pub(in crate::sqlite) async fn drain(&self) -> Result<(), SelfPasswordDrainError> {
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self.inner.state();
                if state.reserved == 0 {
                    return if state.uncertain { Err(SelfPasswordDrainError) } else { Ok(()) };
                }
            }
            changed.await;
        }
    }
}

/// Unacknowledged self-service work, never a rollback or physical-drain statement.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("self password command settlement is uncertain")]
pub struct SelfPasswordDrainError;

/// Immutable Database/session pairing. No arbitrary SQL executor or callback input.
#[derive(Debug, Clone)]
pub struct SelfPasswordChanges {
    coordinator: SelfCoordinator,
    profiles: ProfileStore,
    sessions: SessionStore,
    writer: WriterRegistration,
}
impl SelfPasswordChanges {
    /// Reserves before expensive validation/hash work; no SQL before start.
    pub fn reserve(&self) -> Result<SelfPasswordReservation, StorageError> {
        let mut state = self.coordinator.inner.state();
        if state.stopped {
            return Err(StorageError::AdmissionUnavailable { scope: "self password", reason: crate::AdmissionReason::Stopped });
        }
        if state.reserved >= super::CAPACITY {
            return Err(StorageError::AdmissionUnavailable { scope: "self password", reason: crate::AdmissionReason::Capacity });
        }
        state.reserved += 1;
        let command = Arc::new(SelfCommand { inner: self.coordinator.inner.clone(), phase: AtomicU8::new(RESERVED) });
        Ok(SelfPasswordReservation { changes: self.clone(), command })
    }

    /// Takes a bounded pair of opaque commitments belonging to THIS exact factory.
    /// Account comes from the first commitment. Invalid consumed inputs remain unknown.
    pub fn prepare_batch(
        &self,
        changes: Vec<PendingSelfPasswordChange>,
        action: SelfPasswordAction,
    ) -> Result<SelfPasswordBatch, StorageError> {
        if changes.is_empty() || changes.len() > super::CAPACITY {
            return Err(StorageError::Invariant("self password batch size"));
        }
        let first = &changes[0];
        let account = first.account;
        if changes.iter().any(|pending| {
            !Arc::ptr_eq(&pending.changes.coordinator.inner, &self.coordinator.inner)
                || !pending.changes.writer.same_domain(&self.writer)
                || !pending.changes.sessions.same_instance(&self.sessions)
                || pending.account != account
        }) {
            return Err(StorageError::Invariant("self password batch binding mismatch"));
        }
        {
            let _state = self.coordinator.inner.state();
            if changes.iter().any(|pending| pending.owner.0.phase.load(Ordering::Relaxed) != PENDING) {
                return Err(StorageError::OutcomeUnavailable { scope: "self password batch" });
            }
        }
        let runtime = first.runtime.clone();
        Ok(SelfPasswordBatch { changes, action, account, runtime })
    }
}

/// No command authority exists until the one synchronous start call accepts it.
#[derive(Debug)]
pub struct SelfPasswordReservation {
    changes: SelfPasswordChanges,
    command: Arc<SelfCommand>,
}
impl SelfPasswordReservation {
    pub fn start(self, account: AccountId, new_hash: String, expected_hash: String) -> Result<SelfPasswordResult, StorageError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| StorageError::AdmissionUnavailable {
            scope: "self password",
            reason: crate::AdmissionReason::RuntimeUnavailable,
        })?;
        let (send, receive) = oneshot::channel();
        let inner = self.command.inner.clone();
        let state = inner.state();
        if state.stopped {
            drop(state);
            return Err(StorageError::AdmissionUnavailable { scope: "self password", reason: crate::AdmissionReason::Stopped });
        }
        let Self { changes, command } = self;
        drop(state); // A closed runtime can synchronously destroy the captured owner.
        let captured_runtime = runtime.clone();
        drop(runtime.spawn(async move {
            let result = run_sql(changes, command, account, &new_hash, &expected_hash, captured_runtime).await;
            // A failed delivery OWNS its returned Pending; dropping it is unknown,
            // not successful SQL-stage completion or a clean command result.
            drop(send.send(result));
        }));
        Ok(SelfPasswordResult { receive })
    }
}

/// Receiver-only SQL-stage result. Dropping never aborts an admitted task.
#[derive(Debug)]
pub struct SelfPasswordResult {
    receive: oneshot::Receiver<Result<Option<PendingSelfPasswordChange>, StorageError>>,
}
impl SelfPasswordResult {
    pub async fn wait(self) -> Result<Option<PendingSelfPasswordChange>, StorageError> {
        self.receive.await.map_err(|_| StorageError::OutcomeUnavailable { scope: "self password SQL" })?
    }
}

/// A committed password with unfinished session work. Nonclone; Drop is uncertain.
/// It retains active authority enrollment while serial is released for other writers.
#[derive(Debug)]
pub struct PendingSelfPasswordChange {
    changes: SelfPasswordChanges,
    account: AccountId,
    mutation: Option<SuspendedMutation>,
    owner: SelfOwner,
    runtime: tokio::runtime::Handle,
}

/// Session effects only: no session creation, renewal or caller-supplied account.
#[derive(Debug, Clone, Copy)]
pub enum SelfPasswordAction {
    EndAll,
    EndOthers { current: [u8; 32] },
}
impl SelfPasswordAction {
    fn keep(self) -> Option<[u8; 32]> {
        match self {
            Self::EndAll => None,
            Self::EndOthers { current } => Some(current),
        }
    }
}

/// Validated commitments, already counted before terminal stop. Start adds no slot.
#[derive(Debug)]
pub struct SelfPasswordBatch {
    changes: Vec<PendingSelfPasswordChange>,
    action: SelfPasswordAction,
    account: AccountId,
    runtime: tokio::runtime::Handle,
}
impl SelfPasswordBatch {
    /// Dispatches through the runtime captured before SQL. No fresh runtime/admission.
    pub fn start(self) -> SelfPasswordBatchResult {
        let runtime = self.runtime.clone();
        let (send, receive) = oneshot::channel();
        drop(runtime.spawn(async move {
            let result = run_batch(self, |sessions, worker| async move { sessions.finish_self_password(worker).await }).await;
            let _ = send.send(result);
        }));
        SelfPasswordBatchResult { receive }
    }
}

/// Receiver-only final result, not a durable receipt or response-delivery statement.
#[derive(Debug)]
pub struct SelfPasswordBatchResult {
    receive: oneshot::Receiver<Result<usize, StorageError>>,
}
impl SelfPasswordBatchResult {
    pub async fn wait(self) -> Result<usize, StorageError> {
        self.receive.await.map_err(|_| StorageError::OutcomeUnavailable { scope: "self password batch" })?
    }
}

// All phase reads/writes are under Inner.state. Atomics permit this common-lock
// discipline without a second command mutex/lock order for a two-command batch.
const RESERVED: u8 = 0;
const ACQUIRED: u8 = 1;
const PENDING: u8 = 2;
const FINALIZING: u8 = 3;
const ACKNOWLEDGED: u8 = 4;
const UNCERTAIN: u8 = 5;

#[derive(Debug)]
struct SelfCommand {
    inner: Arc<Inner>,
    phase: AtomicU8,
}
impl SelfCommand {
    fn owner_dropped(&self) {
        let mut state = self.inner.state();
        if matches!(self.phase.load(Ordering::Relaxed), ACQUIRED | PENDING | FINALIZING) {
            self.phase.store(UNCERTAIN, Ordering::Relaxed);
            state.uncertain = true;
        }
        drop(state);
        self.inner.changed.notify_waiters();
    }
    fn acknowledged_no_write(&self, mutation: Mutation) {
        let _state = self.inner.state();
        mutation.complete();
        self.phase.store(ACKNOWLEDGED, Ordering::Relaxed);
    }
}
impl Drop for SelfCommand {
    fn drop(&mut self) {
        self.owner_dropped();
        self.inner.state().reserved -= 1;
        self.inner.changed.notify_waiters();
    }
}
#[derive(Debug)]
struct SelfOwner(Arc<SelfCommand>);
impl Drop for SelfOwner {
    fn drop(&mut self) {
        self.0.owner_dropped();
    }
}

async fn run_sql(
    changes: SelfPasswordChanges,
    command: Arc<SelfCommand>,
    account: AccountId,
    new_hash: &str,
    expected_hash: &str,
    runtime: tokio::runtime::Handle,
) -> Result<Option<PendingSelfPasswordChange>, StorageError> {
    let mut owner = Some(SelfOwner(command.clone()));
    let mutation = changes.writer.begin().await.map_err(|error| StorageError::AdmissionUnavailable {
        scope: "self password",
        reason: match error {
            BeginError::Capacity => crate::AdmissionReason::Capacity,
            BeginError::Stopped => crate::AdmissionReason::Stopped,
        },
    })?;
    {
        let _state = command.inner.state();
        command.phase.store(ACQUIRED, Ordering::Relaxed);
    }
    if !changes.profiles.replace_password_hash(account, new_hash, Some(expected_hash)).await? {
        command.acknowledged_no_write(mutation);
        return Ok(None);
    }
    let mutation = mutation.suspend();
    {
        let _state = command.inner.state();
        command.phase.store(PENDING, Ordering::Relaxed);
    }
    Ok(Some(PendingSelfPasswordChange { changes, account, mutation: Some(mutation), owner: owner.take().expect("SQL owner"), runtime }))
}

async fn run_batch<F, Fut>(mut batch: SelfPasswordBatch, finish: F) -> Result<usize, StorageError>
where
    F: FnOnce(SessionStore, SelfPasswordWorker) -> Fut,
    Fut: std::future::Future<Output = Result<usize, StorageError>>,
{
    // Immediate markers remain async-owned even while the actual worker retains
    // all commands: losing this task is unknown before the last Arc disappears.
    let _owners: Vec<_> = batch.changes.iter().map(|pending| SelfOwner(pending.owner.0.clone())).collect();
    let inner = batch.changes[0].owner.0.inner.clone();
    {
        let _state = inner.state();
        if batch.changes.iter().any(|pending| pending.owner.0.phase.load(Ordering::Relaxed) != PENDING) {
            return Err(StorageError::OutcomeUnavailable { scope: "self password batch" });
        }
        for pending in &batch.changes {
            pending.owner.0.phase.store(FINALIZING, Ordering::Relaxed);
        }
    }
    let mut leader = batch.changes.remove(0);
    let mutation = leader.mutation.take().expect("validated pending leader").resume().await;
    let sessions = leader.changes.sessions.clone();
    let worker = SelfPasswordWorker {
        account: batch.account,
        keep: batch.action.keep(),
        leader: Some(mutation),
        primary: leader,
        followers: batch.changes,
    };
    finish(sessions, worker).await
}

/// Private typed bridge; the actual KV closure owns every enrollment and slot.
#[derive(Debug)]
pub(crate) struct SelfPasswordWorker {
    pub(crate) account: AccountId,
    pub(crate) keep: Option<[u8; 32]>,
    leader: Option<Mutation>,
    primary: PendingSelfPasswordChange,
    followers: Vec<PendingSelfPasswordChange>,
}
impl SelfPasswordWorker {
    /// Called ONLY after the real synchronous batch result; no IO while locked.
    pub(crate) fn acknowledge(&mut self, success: bool) -> bool {
        let inner = self.primary.owner.0.inner.clone();
        let mut state = inner.state();
        let clean = success
            && std::iter::once(&self.primary)
                .chain(self.followers.iter())
                .all(|pending| pending.owner.0.phase.load(Ordering::Relaxed) == FINALIZING);
        if clean {
            for pending in &mut self.followers {
                pending.mutation.take().expect("validated follower").complete();
                pending.owner.0.phase.store(ACKNOWLEDGED, Ordering::Relaxed);
            }
            self.leader.take().expect("resumed leader").complete();
            self.primary.owner.0.phase.store(ACKNOWLEDGED, Ordering::Relaxed);
        } else {
            for pending in std::iter::once(&self.primary).chain(self.followers.iter()) {
                pending.owner.0.phase.store(UNCERTAIN, Ordering::Relaxed);
            }
            state.uncertain = true;
        }
        clean
    }
}

#[cfg(test)]
mod tests;
