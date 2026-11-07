//! Static epoch-session ports use the actual redb CAS and one-statement SQL projection.
use crate::{AccountStore, EpochSnapshot, StorageError, kv::sessions::epoch::EpochSessionStore};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::{
    TokenDigest,
    credential::CredentialEpoch,
    sessions::{EpochSessionRepository, EpochSessionUserRepository, SessionRecord, SessionUser},
};

impl EpochSessionRepository for EpochSessionStore {
    type Error = StorageError;
    type Snapshot = EpochSnapshot;
    fn put_bound(
        &self,
        digest: TokenDigest,
        record: SessionRecord,
        original: CredentialEpoch,
    ) -> impl Future<Output = Result<Option<EpochSnapshot>, Self::Error>> + Send {
        let account = record.account_id;
        EpochSessionStore::put_bound(self, digest, record, account, original)
    }
    fn get_snapshot(&self, digest: TokenDigest) -> impl Future<Output = Result<Option<EpochSnapshot>, Self::Error>> + Send {
        EpochSessionStore::get_snapshot(self, digest)
    }
    fn renew(
        &self,
        expected: &EpochSnapshot,
        expires: Timestamp,
        seen: Timestamp,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        EpochSessionStore::renew(self, expected, expires, seen)
    }
    fn delete(&self, expected: &EpochSnapshot) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        EpochSessionStore::delete(self, expected)
    }
    fn logout(&self, expected: &EpochSnapshot) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        EpochSessionStore::logout(self, expected)
    }
    fn list_bound(&self, account: AccountId) -> impl Future<Output = Result<Vec<EpochSnapshot>, Self::Error>> + Send {
        EpochSessionStore::list_bound(self, account)
    }
    fn revoke_others(&self, current: &EpochSnapshot) -> impl Future<Output = Result<Option<usize>, Self::Error>> + Send {
        EpochSessionStore::revoke_others(self, current)
    }
    fn purge_expired_bound(&self) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        EpochSessionStore::purge_expired_bound(self)
    }
}
impl EpochSessionUserRepository for AccountStore {
    type Error = StorageError;
    fn find_session_user_at_epoch(
        &self,
        account: AccountId,
        notifications: bool,
        original: CredentialEpoch,
    ) -> impl Future<Output = Result<Option<SessionUser>, Self::Error>> + Send {
        AccountStore::find_session_user_at_epoch(self, account, notifications, original)
    }
}
