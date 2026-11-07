//! One operation task retains the actual stores through cancellation of its waiter.
use super::{
    model::{Checkpoint, IdentityUpgradeError as Error},
    owner::StoppedIdentityUpgrade,
};
use futures_util::FutureExt;
use sqlx::Connection;
use std::{panic::AssertUnwindSafe, sync::Arc};
pub(super) enum Operation {
    Install,
    Prepare,
    Accept,
}
pub(super) struct Outcome {
    pub owner: StoppedIdentityUpgrade,
    pub checkpoint: Option<Checkpoint>,
}
/// Ordinary operation failures retain their owner for explicit retry. Abnormal task
/// loss retains the KV guard but cannot reconstruct a lost SQLite connection.
pub struct IdentityUpgradeFailure {
    owner: Option<Box<StoppedIdentityUpgrade>>,
    _guard: Arc<redb::Database>,
    error: Error,
}
impl IdentityUpgradeFailure {
    pub fn error(&self) -> &Error {
        &self.error
    }
    pub fn into_owner(self) -> Option<StoppedIdentityUpgrade> {
        self.owner.map(|owner| *owner)
    }
}
impl std::fmt::Debug for IdentityUpgradeFailure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("IdentityUpgradeFailure").finish_non_exhaustive()
    }
}
impl std::fmt::Display for IdentityUpgradeFailure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, out)
    }
}
impl std::error::Error for IdentityUpgradeFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl StoppedIdentityUpgrade {
    pub(super) async fn execute_owned(self, operation: Operation) -> Result<Outcome, IdentityUpgradeFailure> {
        let guard = Arc::clone(&self.db);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return Err(IdentityUpgradeFailure { owner: Some(Box::new(self)), _guard: guard, error: Error::NoRuntime });
        };
        let task = runtime.spawn(async move {
            let mut owner = self;
            let result = AssertUnwindSafe(async {
                match operation {
                    Operation::Install => owner.install_inner().await.map(|()| None),
                    Operation::Prepare => owner.prepare_inner().await.map(Some),
                    Operation::Accept => owner.validate_prepared().await.map(Some),
                }
            })
            .catch_unwind()
            .await
            .unwrap_or(Err(Error::Abnormal));
            // SQLite uses a worker thread. This ordered barrier follows any queued rollback
            // before returning an owner/result; it is not a claim of thread termination.
            let barrier = owner.connection.ping().await;
            match (result, barrier) {
                (Ok(checkpoint), Ok(())) => Ok(Outcome { owner, checkpoint }),
                (Err(error), _) => Err((Box::new(owner), error)),
                (Ok(_), Err(error)) => Err((Box::new(owner), Error::from(error))),
            }
        });
        match task.await {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err((owner, error))) => Err(IdentityUpgradeFailure { owner: Some(owner), _guard: guard, error }),
            Err(error) => Err(IdentityUpgradeFailure { owner: None, _guard: guard, error: Error::from(error) }),
        }
    }
}
