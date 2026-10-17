//! Complete credential changes and session revocation even when the request disconnects.

use std::future::Future;

use crate::ServiceError;

/// Once a credential update starts, its security side effects belong to the server.
/// Dropping the awaiting request detaches the task instead of cancelling revocation
/// between the database commit and the session-store update.
pub(crate) async fn finish_update<F>(update: F) -> Result<(), ServiceError>
where
    F: Future<Output = Result<(), ServiceError>> + Send + 'static,
{
    tokio::spawn(async move {
        let result = update.await;
        if let Err(ServiceError::Internal(error)) = &result {
            tracing::error!(%error, "credential update could not complete");
        }
        result
    })
    .await
    .map_err(ServiceError::internal)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn request_cancellation_cannot_interrupt_session_revocation() {
        let (committed, commit) = tokio::sync::oneshot::channel();
        let (continue_update, resume) = tokio::sync::oneshot::channel();
        let (revoked, revocation) = tokio::sync::oneshot::channel();
        let request = tokio::spawn(finish_update(async move {
            committed.send(()).unwrap();
            resume.await.unwrap();
            revoked.send(()).unwrap();
            Ok(())
        }));
        commit.await.unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        continue_update.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), revocation).await.unwrap().unwrap();
    }
}
