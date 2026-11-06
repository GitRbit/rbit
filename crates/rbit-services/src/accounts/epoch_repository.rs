//! Completed mutations are retained by the existing per-instance session owner.
use super::AccountService;
use crate::{ServiceError, credentials};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::{
    TokenDigest,
    credential::CredentialEpoch,
    sessions::{EpochSessionRepository, SessionRecord},
};
use rbit_storage::{EpochSessionStore, EpochSnapshot, StorageError};

pub(super) struct OwnedEpochSessions<'a> {
    owner: &'a AccountService,
    store: EpochSessionStore,
}
impl<'a> OwnedEpochSessions<'a> {
    pub fn new(owner: &'a AccountService) -> Self {
        Self { owner, store: owner.sessions.epoch_sidecar() }
    }
    fn run<T: Send + 'static, F, U>(&self, operation: U) -> impl Future<Output = Result<T, ServiceError>> + Send
    where
        F: Future<Output = Result<T, StorageError>> + Send + 'static,
        U: FnOnce(EpochSessionStore) -> F + Send + 'static,
    {
        // No owned task captures this adapter or its controller. The already-counted
        // permit adopts exactly one worker; dropping its caller cannot abort effects.
        let ownership = self.owner.reserve_password_change();
        let store = self.store.clone();
        async move {
            credentials::finish_value(ownership?, move |context| async move {
                let _ownership = context;
                operation(store).await.map_err(ServiceError::from)
            })
            .await
        }
    }
}
impl EpochSessionRepository for OwnedEpochSessions<'_> {
    type Error = ServiceError;
    type Snapshot = EpochSnapshot;
    fn put_bound(
        &self,
        digest: TokenDigest,
        record: SessionRecord,
        original: CredentialEpoch,
    ) -> impl Future<Output = Result<Option<EpochSnapshot>, Self::Error>> + Send {
        self.run(move |store| async move {
            let account = record.account_id;
            store.put_bound(digest, record, account, original).await
        })
    }
    fn get_snapshot(&self, digest: TokenDigest) -> impl Future<Output = Result<Option<EpochSnapshot>, Self::Error>> + Send {
        async move { self.store.get_snapshot(digest).await.map_err(ServiceError::from) }
    }
    fn renew(
        &self,
        expected: &EpochSnapshot,
        expires: Timestamp,
        seen: Timestamp,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        let expected = expected.clone();
        self.run(move |store| async move { store.renew_captured(expected, expires, seen).await })
    }
    fn delete(&self, expected: &EpochSnapshot) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        let expected = expected.clone();
        self.run(move |store| async move { store.delete_captured(expected).await })
    }
    fn list_bound(&self, account: AccountId) -> impl Future<Output = Result<Vec<EpochSnapshot>, Self::Error>> + Send {
        async move { self.store.list_bound(account).await.map_err(ServiceError::from) }
    }
    fn revoke_others(&self, current: &EpochSnapshot) -> impl Future<Output = Result<Option<usize>, Self::Error>> + Send {
        let current = current.clone();
        self.run(move |store| async move { store.revoke_others_captured(current).await })
    }
    fn purge_expired_bound(&self) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        self.run(move |store| async move { store.purge_expired_bound().await })
    }
}

#[cfg(test)]
mod tests;
