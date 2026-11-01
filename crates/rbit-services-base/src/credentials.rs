//! Complete credential changes and session revocation even when the request disconnects.

use std::future::Future;

use crate::ServiceError;

/// Owns forced/admin and email-reset replacements through their audit/event tails.
/// Self-service uses its independent bounded INITIAL/FINAL flow owner.
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
            return Err(ServiceError::OutcomeUnavailable { scope: "password change task" });
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
                return Err(ServiceError::AdmissionUnavailable {
                    scope: "password change",
                    reason: rbit_storage::AdmissionReason::Stopped,
                });
            }
            if state.active >= 2 {
                return Err(ServiceError::AdmissionUnavailable {
                    scope: "password change",
                    reason: rbit_storage::AdmissionReason::Capacity,
                });
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
                tracing::error!(%error, "password change command returned failure");
            }
            drop(owned);
            let _ = send.send(result);
        }));
        receive.await.map_err(|_| ServiceError::OutcomeUnavailable { scope: "password change task" })?
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

pub(crate) mod self_service;

#[cfg(test)]
mod tests {
    use super::self_service::SelfPasswordOwner;
    use rbit_core::Timestamp;
    use rbit_storage::{Database, NewUser, SessionRecord, sqlite::credential_changes::SelfPasswordAction};
    use std::time::Duration;
    use tokio::{sync::oneshot, time::timeout};

    #[tokio::test]
    async fn request_cancellation_cannot_interrupt_session_revocation() {
        const WAIT: Duration = Duration::from_secs(2);
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("sql.db")).await.unwrap();
        let kv = database.open_kv(&directory.path().join("kv.redb")).unwrap();
        let user = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "old".into(),
                is_admin: true,
            })
            .await
            .unwrap();
        kv.sessions()
            .put([1; 32], SessionRecord::new(user.id(), Timestamp(Timestamp::now().seconds() + 3600), None, "test"))
            .await
            .unwrap();
        let changes = database.self_password_changes(&kv).unwrap();
        let owned_changes = changes.clone();
        let owner = SelfPasswordOwner::default();
        let coordinator = owner.clone();
        let (committed, commit) = oneshot::channel();
        let (continue_update, resume) = oneshot::channel();
        let (revoked, revocation) = oneshot::channel();
        let request = tokio::spawn(async move {
            coordinator
                .run(&changes, move |reservation, _lease| async move {
                    let pending = reservation.start(user.id(), "new".into(), "old".into())?.wait().await?.unwrap();
                    committed.send(()).unwrap(); // The original regression now observes real SQL COMMIT.
                    timeout(WAIT, resume).await.map_err(crate::ServiceError::internal)?.map_err(crate::ServiceError::internal)?;
                    owned_changes.prepare_batch(vec![pending], SelfPasswordAction::EndAll)?.start().wait().await?;
                    revoked.send(()).unwrap();
                    Ok(())
                })
                .await
        });
        timeout(WAIT, commit).await.unwrap().unwrap();
        request.abort();
        assert!(timeout(WAIT, request).await.unwrap().unwrap_err().is_cancelled());
        continue_update.send(()).unwrap();
        timeout(WAIT, revocation).await.unwrap().unwrap();
        timeout(WAIT, owner.shutdown()).await.unwrap().unwrap();
        timeout(WAIT, database.drain_self_password_changes()).await.unwrap().unwrap();
        assert!(kv.sessions().get([1; 32]).await.unwrap().is_none());
    }
}
