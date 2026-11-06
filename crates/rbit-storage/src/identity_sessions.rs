//! Session ports retain the existing backend transaction and projection implementations.

use crate::{AccountStore, SessionStore, StorageError};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::{
    TokenDigest,
    sessions::{SessionRecord, SessionRepository, SessionUser, SessionUserRepository, StoredSession},
};
use std::future::Future;

impl SessionRepository for SessionStore {
    type Error = StorageError;
    fn put(&self, digest: TokenDigest, record: SessionRecord) -> impl Future<Output = Result<(), Self::Error>> + Send {
        SessionStore::put(self, digest, record)
    }
    fn get_live(&self, digest: TokenDigest) -> impl Future<Output = Result<Option<SessionRecord>, Self::Error>> + Send {
        SessionStore::get(self, digest)
    }
    fn renew_live(
        &self,
        digest: TokenDigest,
        expires: Timestamp,
        seen: Timestamp,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        SessionStore::renew(self, digest, expires, seen)
    }
    fn delete(&self, digest: TokenDigest) -> impl Future<Output = Result<(), Self::Error>> + Send {
        SessionStore::delete(self, digest)
    }
    fn list_for_account(&self, account: AccountId) -> impl Future<Output = Result<Vec<StoredSession>, Self::Error>> + Send {
        SessionStore::list_for_account(self, account)
    }
    fn delete_for_account(&self, account: AccountId, keep: Option<TokenDigest>) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        SessionStore::delete_for_account(self, account, keep)
    }
}

impl SessionUserRepository for AccountStore {
    type Error = StorageError;
    fn find_session_user(
        &self,
        account: AccountId,
        notifications: bool,
    ) -> impl Future<Output = Result<Option<SessionUser>, Self::Error>> + Send {
        AccountStore::find_session_user(self, account, notifications)
    }
}
