//! Response-deferred password work: separate bounded INITIAL and FINAL owners.
//!
//! No ledger is captured by a deferred finalizer. Both phases retain one counted
//! flow lease; an acknowledged phase cannot clear another lost phase's uncertainty.
use std::{
    future::Future,
    sync::{Arc, Mutex, MutexGuard},
};

use rbit_core::AccountId;
use rbit_storage::sqlite::credential_changes::{
    PendingSelfPasswordChange, SelfPasswordAction, SelfPasswordChanges, SelfPasswordReservation,
};
use tokio::{runtime::Handle, sync::oneshot};
use tokio_util::task::{TaskTracker, task_tracker::TaskTrackerToken};

use crate::{ServiceError, accounts::SessionEnforcer};

#[derive(Debug, Default)]
struct Admission {
    active: usize,
    closed: bool,
    uncertain: bool,
}
#[derive(Debug, Default)]
struct Inner {
    admission: Mutex<Admission>,
    tasks: TaskTracker,
}
impl Inner {
    fn admission(&self) -> MutexGuard<'_, Admission> {
        self.admission.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.closed = true;
            state.uncertain = true;
            state
        })
    }
}
/// Two flow/audit-tail slots per `AccountService` construction, shared by its clones.
/// Independent service constructions keep separate owners and response ledgers.
/// Storage's separate two-command limit is shared by Database clones.
/// Slots cover only admitted changes, never ordinary pre-body response guards.
#[derive(Debug, Clone, Default)]
pub(crate) struct SelfPasswordOwner {
    inner: Arc<Inner>,
}
impl SelfPasswordOwner {
    pub(crate) fn prepare_shutdown(&self) {
        let mut state = self.inner.admission();
        state.closed = true;
        self.inner.tasks.close();
    }
    pub(crate) async fn shutdown(&self) -> Result<(), ServiceError> {
        self.prepare_shutdown();
        self.inner.tasks.wait().await;
        if self.inner.admission().uncertain {
            return Err(ServiceError::OutcomeUnavailable { scope: "self password task" });
        }
        Ok(())
    }
    pub(crate) async fn run<F, Fut>(&self, changes: &SelfPasswordChanges, operation: F) -> Result<(), ServiceError>
    where
        F: FnOnce(SelfPasswordReservation, Arc<FlowLease>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ServiceError>> + Send + 'static,
    {
        let runtime = Handle::try_current().map_err(|_| ServiceError::AdmissionUnavailable {
            scope: "self password task",
            reason: rbit_storage::AdmissionReason::RuntimeUnavailable,
        })?;
        let (reservation, lease) = {
            let mut state = self.inner.admission();
            if state.closed {
                return Err(ServiceError::AdmissionUnavailable { scope: "self password", reason: rbit_storage::AdmissionReason::Stopped });
            }
            if state.active >= 2 {
                return Err(ServiceError::AdmissionUnavailable { scope: "self password", reason: rbit_storage::AdmissionReason::Capacity });
            }
            let reservation = changes.reserve()?;
            let token = self.inner.tasks.token();
            state.active += 1;
            (reservation, Arc::new(FlowLease { inner: self.inner.clone(), _token: token, runtime: runtime.clone() }))
        };
        let mut initial = PhaseOwner::new(lease.clone());
        let (send, receive) = oneshot::channel();
        // No admission lock across spawn: closed runtimes synchronously drop tasks.
        drop(runtime.spawn(async move {
            let result = operation(reservation, lease).await;
            initial.acknowledged = true; // Normal Err is known task outcome; Storage owns write uncertainty.
            if let Err(ServiceError::Internal(error)) = &result {
                tracing::error!(%error, "credential update could not complete");
            }
            drop(initial);
            let _ = send.send(result);
        }));
        receive.await.map_err(|_| ServiceError::OutcomeUnavailable { scope: "self password task" })?
    }
}

/// One shared flow slot/tracker token, retained across initial and final overlap.
/// It owns only narrow resources and never AccountService/ProfileService/ledger.
#[derive(Debug)]
pub(crate) struct FlowLease {
    inner: Arc<Inner>,
    _token: TaskTrackerToken,
    runtime: Handle,
}
impl Drop for FlowLease {
    fn drop(&mut self) {
        self.inner.admission().active -= 1;
    }
}
#[derive(Debug)]
struct PhaseOwner {
    lease: Arc<FlowLease>,
    acknowledged: bool,
}
impl PhaseOwner {
    fn new(lease: Arc<FlowLease>) -> Self {
        Self { lease, acknowledged: false }
    }
}
impl Drop for PhaseOwner {
    fn drop(&mut self) {
        if !self.acknowledged {
            self.lease.inner.admission().uncertain = true;
        }
    }
}

/// The same flow slot/runtime/INITIAL owner was preadmitted before SQL; this FINAL
/// marker is created synchronously after handoff, before ledger insertion, with no
/// await gap. No edge back to the account ledger is permitted here.
#[derive(Debug)]
pub(crate) struct DeferredCommit {
    pub(crate) change: PendingSelfPasswordChange,
    finalizer: Finalizer,
}
#[derive(Debug)]
struct Finalizer {
    owner: PhaseOwner,
    changes: SelfPasswordChanges,
    enforcer: SessionEnforcer,
    user: AccountId,
}
impl DeferredCommit {
    pub(crate) fn new(
        change: PendingSelfPasswordChange,
        lease: Arc<FlowLease>,
        changes: SelfPasswordChanges,
        enforcer: SessionEnforcer,
        user: AccountId,
    ) -> Self {
        Self { change, finalizer: Finalizer { owner: PhaseOwner::new(lease), changes, enforcer, user } }
    }
}
#[derive(Debug)]
pub(crate) struct Decision {
    commits: Vec<DeferredCommit>,
    action: SelfPasswordAction,
}

/// Only a receiver. Dispatch occurred synchronously, so an unpolled/dropped wait
/// cannot cancel the actual continuation or create another task/admission slot.
#[derive(Debug, Default)]
pub(crate) struct Completion {
    receive: Option<oneshot::Receiver<Result<(), ServiceError>>>,
}
impl Completion {
    pub(crate) async fn wait(self) -> Result<(), ServiceError> {
        match self.receive {
            Some(receive) => receive.await.map_err(|_| ServiceError::OutcomeUnavailable { scope: "self password task" })?,
            None => Ok(()),
        }
    }
}
impl Decision {
    // Only the ledger builds decisions. Its vectors are either one committed item
    // or an atomic union bounded by the two Storage/self flow slots, never empty.
    pub(crate) fn new(commits: Vec<DeferredCommit>, action: SelfPasswordAction) -> Self {
        assert!((1..=2).contains(&commits.len()), "typed password decision capacity");
        Self { commits, action }
    }
    pub(crate) fn dispatch(self) -> Completion {
        self.dispatch_using(|enforcer, user, removed| async move { enforcer.record_ended_sessions(user, removed).await })
    }
    fn dispatch_using<F, Fut>(self, record: F) -> Completion
    where
        F: FnOnce(SessionEnforcer, AccountId, usize) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let runtime = self.commits[0].finalizer.owner.lease.runtime.clone();
        let user = self.commits[0].finalizer.user;
        let action = self.action;
        let (send, receive) = oneshot::channel();
        // State/response attribution was frozen before this point. All capacity
        // and phase owners already exist, including on a no-current-runtime Drop.
        drop(runtime.spawn(async move {
            let result = self.run_with(record).await;
            if let Err(error) = &result {
                match action {
                    SelfPasswordAction::EndAll => tracing::error!(%error, user = user.0, "failed to end sessions after a password change"),
                    SelfPasswordAction::EndOthers { .. } => {
                        tracing::error!(%error, user = user.0, "failed to end other sessions after a password change");
                    }
                }
            }
            let _ = send.send(result);
        }));
        Completion { receive: Some(receive) }
    }
    // Private monomorphized post-KV audit seam; production above always supplies
    // the real fixed recorder. No callback enters any public service interface.
    async fn run_with<F, Fut>(self, record: F) -> Result<(), ServiceError>
    where
        F: FnOnce(SessionEnforcer, AccountId, usize) -> Fut,
        Fut: Future<Output = ()>,
    {
        let mut changes = Vec::with_capacity(self.commits.len());
        let mut finalizers = Vec::with_capacity(self.commits.len());
        for commit in self.commits {
            changes.push(commit.change);
            finalizers.push(commit.finalizer);
        }
        let result: Result<(), ServiceError> = async {
            let first = &finalizers[0];
            let removed = first.changes.prepare_batch(changes, self.action)?.start().wait().await?;
            record(first.enforcer.clone(), first.user, removed).await;
            Ok(())
        }
        .await;
        // Every normal final Result is known service-phase completion. Storage
        // independently refuses a clean command/domain certificate after write loss.
        for finalizer in &mut finalizers {
            finalizer.owner.acknowledged = true;
        }
        result
    }
    #[cfg(test)]
    pub(crate) fn dispatch_test<F, Fut>(self, record: F) -> Completion
    where
        F: FnOnce(SessionEnforcer, AccountId, usize) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.dispatch_using(record)
    }
}
