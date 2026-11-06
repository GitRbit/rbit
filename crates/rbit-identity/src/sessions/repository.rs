//! Backend guarantees are explicit; returning a snapshot never authorizes renewal.

use super::{SessionRecord, SessionUser, StoredSession};
use crate::TokenDigest;
use rbit_contracts::{AccountId, Timestamp};
use std::future::Future;

pub trait SessionRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    fn put(&self, digest: TokenDigest, record: SessionRecord) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Reads only live sessions; cleanup must recheck expiry in its write transaction.
    fn get_live(&self, digest: TokenDigest) -> impl Future<Output = Result<Option<SessionRecord>, Self::Error>> + Send;
    /// Atomically rechecks live existence, never upserts and only moves timestamps forward.
    fn renew_live(
        &self,
        digest: TokenDigest,
        expires: Timestamp,
        seen: Timestamp,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn delete(&self, digest: TokenDigest) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Live sessions only, in descending last-seen order; account ownership is mandatory.
    fn list_for_account(&self, account: AccountId) -> impl Future<Output = Result<Vec<StoredSession>, Self::Error>> + Send;
    /// Both session and account-index rows change in the same transaction.
    fn delete_for_account(&self, account: AccountId, keep: Option<TokenDigest>) -> impl Future<Output = Result<usize, Self::Error>> + Send;
}

pub trait SessionUserRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    /// One lookup must enforce active-user visibility and supply both request flags.
    fn find_session_user(
        &self,
        account: AccountId,
        notifications: bool,
    ) -> impl Future<Output = Result<Option<SessionUser>, Self::Error>> + Send;
}
