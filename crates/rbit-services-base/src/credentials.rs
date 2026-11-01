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

/// Owns only forced/admin replacement through its audit/event tail. Other credential
/// flows still use `finish_update` and require separate lifecycle coordination.
#[derive(Debug, Clone, Default)]
pub(crate) struct AdminPasswordOwner {
    inner: std::sync::Arc<AdminInner>,
}
#[derive(Debug, Default)]
struct AdminInner {
    admission: std::sync::Mutex<AdminAdmission>,
    tasks: tokio_util::task::TaskTracker,
}
#[derive(Debug, Default)]
struct AdminAdmission {
    active: usize,
    closed: bool,
    uncertain: bool,
}
impl AdminInner {
    fn admission(&self) -> std::sync::MutexGuard<'_, AdminAdmission> {
        self.admission.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.closed = true;
            state.uncertain = true;
            state
        })
    }
}
impl AdminPasswordOwner {
    pub(crate) fn prepare_shutdown(&self) {
        let mut state = self.inner.admission();
        state.closed = true;
        self.inner.tasks.close();
    }
    pub(crate) async fn shutdown(&self) -> Result<(), ServiceError> {
        self.prepare_shutdown();
        self.inner.tasks.wait().await;
        if self.inner.admission().uncertain {
            return Err(ServiceError::Conflict("administrator password task settlement uncertain"));
        }
        Ok(())
    }
    pub(crate) async fn run<F, Fut>(
        &self,
        changes: &rbit_storage::sqlite::credential_changes::ForcedPasswordChanges,
        operation: F,
    ) -> Result<(), ServiceError>
    where
        F: FnOnce(rbit_storage::sqlite::credential_changes::ForcedPasswordReservation) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ServiceError>> + Send + 'static,
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        let (reservation, mut owned) = {
            let mut state = self.inner.admission();
            if state.closed {
                return Err(ServiceError::Conflict("administrator password commands stopped"));
            }
            if state.active >= 2 {
                return Err(ServiceError::Conflict("administrator password command capacity"));
            }
            let reservation = changes.reserve()?;
            let token = self.inner.tasks.token();
            state.active += 1;
            (reservation, AdminTask { inner: self.inner.clone(), _token: token, acknowledged: false })
        };
        // The registered slot/token precedes terminal close; never hold its
        // mutex across spawn, which can synchronously drop a rejected future.
        drop(tokio::spawn(async move {
            let result = operation(reservation).await;
            // A returned Err is a known task outcome, not lost settlement.
            // Storage independently retains uncertainty after possible writes.
            owned.acknowledged = true;
            if let Err(error) = &result {
                tracing::error!(%error, "administrator password command returned failure");
            }
            drop(owned);
            let _ = send.send(result);
        }));
        receive.await.map_err(ServiceError::internal)?
    }
}
#[derive(Debug)]
struct AdminTask {
    inner: std::sync::Arc<AdminInner>,
    _token: tokio_util::task::task_tracker::TaskTrackerToken,
    acknowledged: bool,
}
impl Drop for AdminTask {
    fn drop(&mut self) {
        let mut state = self.inner.admission();
        if !self.acknowledged {
            state.uncertain = true;
        }
        state.active -= 1;
    }
}

#[cfg(test)]
mod admin_tests;

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
