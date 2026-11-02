//! Fixed owned Git edit flows for trusted compiled Rust callers only.
//!
//! This unused API has no SQL/Services/Fork adoption or execution authority. Callers
//! must reserve AND attach before their first effect. K is only an opaque keepalive,
//! not a callback, SQL settlement proof, sandbox or bound on arbitrary caller memory.
//! Unknown flows deliberately retain at most two custody cells per storage instance
//! until process exit. Independent storage constructions do not share this bound.
use crate::{
    GitError, RepositoryStorage,
    storage::{create_fixed, delete_fixed, head_ref, set_head_fixed},
};
use rbit_core::{AccountId, RepoId};
use state::{Custody, Owner, WorkerActivity, WorkerState};
use std::{fmt, path::PathBuf, sync::Arc};
use tokio::{runtime::Handle, time::Instant};

pub(crate) mod state;
#[cfg(test)]
mod tests;

/// Known pre-effect admission failure. No retry/no-effects claim for a whole caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GitAdmissionError {
    #[error("Git edit admission stopped")]
    Stopped,
    #[error("Git edit capacity exhausted")]
    Capacity,
    #[error("Git edit runtime unavailable")]
    RuntimeUnavailable,
    #[error("Git edit coordinator unavailable")]
    CoordinatorUnavailable,
}
/// Loss of the ACTUAL wrapper/worker/payload, never an HTTP final reply waiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerLoss {
    OwnerLost,
    PayloadLost,
    WorkerPanicked,
    CancelledOrRuntimeClosed,
    StateUnavailable,
}
/// Drain cannot certify clean completion; not a physical worker/SQL certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OwnedGitDrainError {
    #[error("Git edit admissions have not stopped")]
    NotStopped,
    #[error("Git edit drain deadline elapsed")]
    Deadline,
    #[error("Git edit outcome unconfirmed")]
    Unconfirmed,
}
/// No release, rebind or recovery interface. K may have been destroyed by panic/loss.
pub struct QuarantinedEdit {
    custody: Arc<Custody>,
}
impl fmt::Debug for QuarantinedEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuarantinedEdit").field("custody", &self.custody).finish_non_exhaustive()
    }
}
/// A fixed operation's result; no arbitrary worker payload or execution callback.
#[derive(Debug, PartialEq, Eq)]
pub enum GitEditOutput {
    Created(PathBuf),
    HeadUpdated,
    Deleted,
    Relocated(PathBuf),
}
/// Normal errors return the whole edit. Unknown loss never reconstructs missing K.
pub enum OwnedGitResult<K: Send + 'static> {
    Returned { edit: BoundRepositoryEdit<K>, operation: Result<GitEditOutput, GitError> },
    Unconfirmed { loss: WorkerLoss, retained: QuarantinedEdit },
}
impl<K: Send + 'static> fmt::Debug for OwnedGitResult<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Returned { edit, operation } => f.debug_struct("Returned").field("edit", edit).field("operation", operation).finish(),
            Self::Unconfirmed { loss, retained } => f.debug_struct("Unconfirmed").field("loss", loss).field("retained", retained).finish(),
        }
    }
}
/// Returns original K on known pre-effect attach refusal; Debug never formats K.
pub struct EditAttachError<K: Send + 'static> {
    pub reason: GitAdmissionError,
    pub context: K,
}
impl<K: Send + 'static> fmt::Debug for EditAttachError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditAttachError").field("reason", &self.reason).finish_non_exhaustive()
    }
}
/// A bounded credit before RepoId/SQL exists. Unattached Drop releases known no-work.
pub struct GitFlowReservation {
    storage: Option<RepositoryStorage>,
    custody: Option<Arc<Custody>>,
}
impl fmt::Debug for GitFlowReservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitFlowReservation").field("custody", &self.custody).finish_non_exhaustive()
    }
}
impl RepositoryStorage {
    /// Reserve before first SQL/backend effect. Capturing Handle does not keep a runtime alive.
    pub fn reserve_edit(&self) -> Result<GitFlowReservation, GitAdmissionError> {
        let runtime = Handle::try_current().map_err(|_| GitAdmissionError::RuntimeUnavailable)?;
        let custody = self.owned_edits.reserve(runtime)?;
        Ok(GitFlowReservation { storage: Some(self.clone()), custody: Some(custody) })
    }
    /// Irreversibly refuses new reserve AND attach; already attached flows can continue.
    pub fn prepare_owned_shutdown(&self) {
        self.owned_edits.stop();
    }
    /// Call after `prepare_owned_shutdown`. Unknown returns error, never clears custody.
    /// Reserved/forgotten credits remain counted; deadline is not an abort/release.
    pub async fn drain_owned(&self, deadline: Instant) -> Result<(), OwnedGitDrainError> {
        loop {
            let changed = self.owned_edits.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if Instant::now() >= deadline {
                return Err(OwnedGitDrainError::Deadline);
            }
            let (stopped, count, unknown) = self.owned_edits.snapshot();
            if !stopped {
                return Err(OwnedGitDrainError::NotStopped);
            }
            if unknown {
                return Err(OwnedGitDrainError::Unconfirmed);
            }
            if count == 0 {
                return if Instant::now() >= deadline { Err(OwnedGitDrainError::Deadline) } else { Ok(()) };
            }
            tokio::time::timeout_at(deadline, changed).await.map_err(|_| OwnedGitDrainError::Deadline)?;
        }
    }
}
impl GitFlowReservation {
    /// This atomic start check MUST precede the caller's first SQL or backend effect.
    /// Returns K on refusal; successful attachment remains eligible after shutdown.
    pub fn attach<K: Send + 'static>(mut self, context: K) -> Result<RepositoryEdit<K>, EditAttachError<K>> {
        // Reserve recoverable payload storage before admission locks or caller effects.
        let context = Box::new(context);
        let custody = self.custody.as_ref().expect("reservation custody");
        if let Err(reason) = custody.registry.attach(custody) {
            return Err(EditAttachError { reason, context: *context });
        }
        Ok(RepositoryEdit {
            core: Core {
                context: Some(context),
                custody: self.custody.take().expect("reservation custody"),
                storage: self.storage.take().expect("reservation storage"),
            },
        })
    }
}
impl Drop for GitFlowReservation {
    fn drop(&mut self) {
        if let Some(custody) = self.custody.take() {
            custody.release(true);
        }
    }
}
struct Core<K: Send + 'static> {
    context: Option<Box<K>>,
    custody: Arc<Custody>,
    storage: RepositoryStorage,
}
impl<K: Send + 'static> Core<K> {
    fn quarantine(mut self) -> QuarantinedEdit {
        self.custody.unknown(WorkerLoss::OwnerLost);
        if let Some(context) = self.context.take() {
            self.custody.retain(context);
        }
        QuarantinedEdit { custody: self.custody.clone() }
    }
    fn finish(mut self) -> Result<K, QuarantinedEdit> {
        if !self.custody.release(false) {
            return Err(self.quarantine());
        }
        Ok(*self.context.take().expect("owned context"))
    }
}
impl<K: Send + 'static> Drop for Core<K> {
    fn drop(&mut self) {
        if self.context.is_some() {
            self.custody.unknown(WorkerLoss::PayloadLost);
            // K may be lost here (including worker unwind). Custody still owns the
            // lease/credit. Do not invoke arbitrary K Drop under the state mutex.
            drop(self.context.take());
        }
    }
}
/// An admitted nonclone flow before create assigns `RepoId`. No serial/SQL authority.
pub struct RepositoryEdit<K: Send + 'static> {
    core: Core<K>,
}
impl<K: Send + 'static> fmt::Debug for RepositoryEdit<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.core.custody.fmt(f)
    }
}
impl<K: Send + 'static> RepositoryEdit<K> {
    pub fn context_mut(&mut self) -> &mut K {
        self.core.context.as_deref_mut().expect("owned context")
    }
    /// Bind once without another capacity/runtime/stop check, including after SQL INSERT.
    pub fn bind(self, repo: RepoId) -> BoundRepositoryEdit<K> {
        BoundRepositoryEdit { core: self.core, repo }
    }
    pub fn quarantine(self) -> QuarantinedEdit {
        self.core.quarantine()
    }
    /// Trusted builtin assertion: SQL is already KNOWN settled and no required
    /// backend/repair is pending. This unbound type has performed no Git phase.
    /// Returns the SAME K with its active SQL enrollment for the later Storage
    /// terminal; not a no-effects, SQL settlement or whole-flow certificate.
    /// Already attached flows remain eligible after stop; unknown still quarantines.
    pub fn finish_without_backend(self) -> Result<K, QuarantinedEdit> {
        self.core.finish()
    }
    /// Trusted assertion that no SQL/backend effects have begun. No generic SQL proof.
    pub fn cancel_before_effects(self) -> Result<K, QuarantinedEdit> {
        self.core.finish()
    }
}
/// A single fixed repository, retaining edit custody across SQL/backend phases.
pub struct BoundRepositoryEdit<K: Send + 'static> {
    core: Core<K>,
    repo: RepoId,
}
impl<K: Send + 'static> fmt::Debug for BoundRepositoryEdit<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundRepositoryEdit").field("repo", &self.repo).field("custody", &self.core.custody).finish_non_exhaustive()
    }
}
impl<K: Send + 'static> BoundRepositoryEdit<K> {
    pub fn context_mut(&mut self) -> &mut K {
        self.core.context.as_deref_mut().expect("owned context")
    }
    pub async fn create(self, owner: AccountId, default_branch: String) -> OwnedGitResult<K> {
        run(self, Fixed::Create { owner, default_branch }).await
    }
    pub async fn set_head(self, owner: AccountId, branch: String) -> OwnedGitResult<K> {
        run(self, Fixed::Head { owner, branch }).await
    }
    pub async fn delete(self, owner: AccountId) -> OwnedGitResult<K> {
        run(self, Fixed::Delete { owner }).await
    }
    pub async fn relocate(self, from: AccountId, to: AccountId) -> OwnedGitResult<K> {
        run(self, Fixed::Relocate { from, to }).await
    }
    pub fn quarantine(self) -> QuarantinedEdit {
        self.core.quarantine()
    }
    /// Trusted builtin caller asserts SQL is ALREADY known settled, no unknown tail,
    /// and required repair/tails are complete. This releases Git custody and returns K;
    /// it does NOT acknowledge Storage. K must retain active enrollment until its later
    /// private Storage acknowledgement; owner loss in that gap must remain unknown.
    pub fn finish(self) -> Result<K, QuarantinedEdit> {
        self.core.finish()
    }
}

enum Fixed {
    Create { owner: AccountId, default_branch: String },
    Head { owner: AccountId, branch: String },
    Delete { owner: AccountId },
    Relocate { from: AccountId, to: AccountId },
}
impl Fixed {
    fn execute(self, storage: &RepositoryStorage, repo: RepoId) -> Result<GitEditOutput, GitError> {
        match self {
            Self::Create { owner, default_branch } => {
                let head = head_ref(&default_branch)?;
                let path = storage.path_for(owner, repo);
                if path.try_exists()? {
                    return Err(GitError::AlreadyExists);
                }
                create_fixed(&path, &head)?;
                Ok(GitEditOutput::Created(path))
            }
            Self::Head { owner, branch } => {
                let head = head_ref(&branch)?;
                let path = storage.path_for(owner, repo);
                if !path.join("HEAD").try_exists()? {
                    return Err(GitError::NotFound("repository"));
                }
                set_head_fixed(&path, &head)?;
                Ok(GitEditOutput::HeadUpdated)
            }
            Self::Delete { owner } => {
                delete_fixed(&storage.path_for(owner, repo))?;
                Ok(GitEditOutput::Deleted)
            }
            Self::Relocate { from, to } => {
                let source = storage.path_for(from, repo);
                let target = storage.path_for(to, repo);
                if source != target {
                    crate::relocate::relocate_fixed(&source, &target)?;
                }
                Ok(GitEditOutput::Relocated(target))
            }
        }
    }
}
async fn run<K: Send + 'static>(edit: BoundRepositoryEdit<K>, operation: Fixed) -> OwnedGitResult<K> {
    let BoundRepositoryEdit { core, repo } = edit;
    let custody = core.custody.clone();
    let mut owner = Owner::new(custody.clone());
    if let Some(loss) = custody.loss() {
        owner.acknowledge();
        return OwnedGitResult::Unconfirmed { loss, retained: core.quarantine() };
    }
    if !custody.has_lease() {
        let lease = core.storage.exclusive(repo).await;
        custody.install_lease(lease);
    }
    #[cfg(test)]
    let gate = core.storage.worker_gates.take();
    custody.worker(WorkerState::Scheduled);
    // No state lock is held: a closed runtime can synchronously drop captured K.
    let worker = custody.runtime.spawn_blocking(move || {
        let _activity = WorkerActivity::start(core.custody.clone());
        #[cfg(test)]
        if let Some(gate) = gate {
            gate.enter();
        }
        let result = operation.execute(&core.storage, repo);
        (core, result)
    });
    let result = worker.await;
    owner.acknowledge();
    match result {
        Ok((core, operation)) => {
            if let Some(loss) = custody.loss() {
                drop(core); // A late success never clears owner/payload uncertainty.
                OwnedGitResult::Unconfirmed { loss, retained: QuarantinedEdit { custody } }
            } else {
                OwnedGitResult::Returned { edit: BoundRepositoryEdit { core, repo }, operation }
            }
        }
        Err(error) => {
            let loss = if error.is_panic() { WorkerLoss::WorkerPanicked } else { WorkerLoss::CancelledOrRuntimeClosed };
            custody.unknown(loss);
            custody.worker(WorkerState::Finished);
            OwnedGitResult::Unconfirmed { loss, retained: QuarantinedEdit { custody } }
        }
    }
}
