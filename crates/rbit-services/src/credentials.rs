//! A previously admitted lease adopts the existing single detached finalization task.

use crate::ServiceError;
use rbit_identity::sessions::effects::{SessionMutationContext, SessionMutationPermit};
use std::future::Future;

pub(crate) async fn finish_update<F, U>(ownership: SessionMutationPermit, update: U) -> Result<(), ServiceError>
where
    F: Future<Output = Result<(), ServiceError>> + Send + 'static,
    U: FnOnce(SessionMutationContext) -> F + Send + 'static,
{
    finish_value(ownership, update).await
}

/// Return a completed store outcome through the same single tracked worker.
pub(crate) async fn finish_value<T: Send + 'static, F, U>(ownership: SessionMutationPermit, update: U) -> Result<T, ServiceError>
where
    F: Future<Output = Result<T, ServiceError>> + Send + 'static,
    U: FnOnce(SessionMutationContext) -> F + Send + 'static,
{
    let work = rbit_storage::capture_work();
    ownership
        .submit_value(move |context| {
            work.scope(async move {
                let result = update(context).await;
                if let Err(ServiceError::Internal(error)) = &result {
                    tracing::error!(%error, "credential update could not complete");
                }
                result
            })
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
    #[tokio::test]
    async fn completed_values_preserve_real_sql_work_through_owned_handoff() {
        let directory = tempfile::tempdir().unwrap();
        let database = rbit_storage::Database::open(&directory.path().join("work.db")).await.unwrap();
        let accounts = database.accounts();
        let scope = SessionEffectScope::new();
        let ownership = scope.reserve().unwrap();
        let (result, work) =
            rbit_storage::count_work(finish_value(
                ownership,
                move |_| async move { accounts.count_users().await.map_err(ServiceError::from) },
            ))
            .await;
        assert_eq!(result.unwrap(), 0);
        assert_eq!(work, rbit_storage::Work { acquisitions: 1, statements: 1, cache_writes: 0 });
        assert!(scope.shutdown().await.drained());
        database.close().await;
    }
}
