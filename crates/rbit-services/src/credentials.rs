//! A previously admitted lease adopts the existing single detached finalization task.

use crate::ServiceError;
use rbit_identity::sessions::effects::{SessionMutationContext, SessionMutationPermit};
use std::future::Future;

pub(crate) async fn finish_update<F, U>(ownership: SessionMutationPermit, update: U) -> Result<(), ServiceError>
where
    F: Future<Output = Result<(), ServiceError>> + Send + 'static,
    U: FnOnce(SessionMutationContext) -> F + Send + 'static,
{
    ownership
        .submit(move |context| async move {
            let result = update(context).await;
            if let Err(ServiceError::Internal(error)) = &result {
                tracing::error!(%error, "credential update could not complete");
            }
            result
        })
        .map_err(ServiceError::internal)?
        .await
        .map_err(ServiceError::internal)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_identity::sessions::effects::SessionEffectScope;

    #[tokio::test]
    async fn cancellation_after_commit_cannot_interrupt_revocation_or_escape_shutdown_drain() {
        let scope = SessionEffectScope::new();
        let (committed, commit) = tokio::sync::oneshot::channel();
        let (continue_update, resume) = tokio::sync::oneshot::channel();
        let (revoked, revocation) = tokio::sync::oneshot::channel();
        let ownership = scope.reserve().unwrap();
        let request = tokio::spawn(finish_update(ownership, move |_context| async move {
            committed.send(()).unwrap();
            resume.await.unwrap();
            revoked.send(()).unwrap();
            Ok(())
        }));
        commit.await.unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        scope.close();
        assert!(!scope.report().drained());
        continue_update.send(()).unwrap();
        let report = scope.shutdown().await;
        assert!(report.drained());
        assert_eq!(report.abnormal, 0);
        revocation.await.unwrap();
    }
}
