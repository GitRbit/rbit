//! Pure captured-binding contracts for the inactive epoch-aware session owner.
use super::{SessionRecord, SessionUser, StoredSession};
use crate::{TokenDigest, credential::CredentialEpoch};
use rbit_contracts::{AccountId, Timestamp};
use std::future::Future;

/// A backend-captured value, not a session-issuance receipt. Implementations must
/// preserve its exact binding/incarnation across all compare-and-set mutations.
pub trait EpochSessionSnapshot: Clone + Send + Sync {
    fn digest(&self) -> TokenDigest;
    fn record(&self) -> &SessionRecord;
    fn epoch(&self) -> CredentialEpoch;
    fn is_bound(&self) -> bool;
    fn into_stored(self) -> StoredSession;
}

pub trait EpochSessionRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    type Snapshot: EpochSessionSnapshot;
    /// Insert-only record/index/epoch commit; return its captured binding, not a
    /// second observation that could accidentally adopt a replacement.
    fn put_bound(
        &self,
        digest: TokenDigest,
        record: SessionRecord,
        original: CredentialEpoch,
    ) -> impl Future<Output = Result<Option<Self::Snapshot>, Self::Error>> + Send;
    /// One read snapshot, including expired records for captured logout/cleanup.
    fn get_snapshot(&self, digest: TokenDigest) -> impl Future<Output = Result<Option<Self::Snapshot>, Self::Error>> + Send;
    fn renew(
        &self,
        expected: &Self::Snapshot,
        expires: Timestamp,
        seen: Timestamp,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn delete(&self, expected: &Self::Snapshot) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Cookie-secret logout matches the same incarnation across a monotonic epoch
    /// promotion. It must still refuse a replacement incarnation/account.
    fn logout(&self, expected: &Self::Snapshot) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Live, bound records for this account; never manufacture missing bindings.
    fn list_bound(&self, account: AccountId) -> impl Future<Output = Result<Vec<Self::Snapshot>, Self::Error>> + Send;
    /// Match a still-live current incarnation and delete its other same-epoch
    /// sessions atomically. None means the current authority no longer matches.
    fn revoke_others(&self, current: &Self::Snapshot) -> impl Future<Output = Result<Option<usize>, Self::Error>> + Send;
    /// Bounded snapshot batches; recheck expiry and exact binding under each write.
    fn purge_expired_bound(&self) -> impl Future<Output = Result<usize, Self::Error>> + Send;
}

pub trait EpochSessionUserRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    /// One statement supplies current active flags, notifications and locale while
    /// checking this ORIGINAL epoch and absent current-target pending intent.
    fn find_session_user_at_epoch(
        &self,
        account: AccountId,
        notifications: bool,
        original: CredentialEpoch,
    ) -> impl Future<Output = Result<Option<SessionUser>, Self::Error>> + Send;
}
