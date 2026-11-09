//! Static persistence ports; implementations own atomicity and ownership filtering.

use std::future::Future;

use super::{AccessTokenId, AccessTokenRecord, NewAccessTokenRecord};
use crate::{TokenDigest, User};
use rbit_contracts::{AccountId, Timestamp};

/// A token resolved from its digest, together with its owner.
pub type ResolvedAccessToken<Scope> = (AccessTokenRecord<Scope>, User);

pub trait AccessTokenRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    type Scope: Copy + Send + Sync;

    /// Inserts only while the owner holds fewer than `limit` tokens, atomically with the
    /// insertion, so concurrent requests cannot exceed it. `None` when the limit is reached.
    fn create_with_limit(
        &self,
        new: NewAccessTokenRecord<Self::Scope>,
        limit: i64,
    ) -> impl Future<Output = Result<Option<AccessTokenRecord<Self::Scope>>, Self::Error>> + Send;
    /// The account's tokens, newest first.
    fn list_for_account(&self, account: AccountId)
    -> impl Future<Output = Result<Vec<AccessTokenRecord<Self::Scope>>, Self::Error>> + Send;
    /// Deletes the token only if `account` owns it; returns whether one was removed.
    fn delete(&self, account: AccountId, id: AccessTokenId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Resolves a digest to the token and its owner. Expiry is the caller's concern.
    fn find_by_digest(
        &self,
        digest: TokenDigest,
    ) -> impl Future<Output = Result<Option<ResolvedAccessToken<Self::Scope>>, Self::Error>> + Send;
    /// Records a use at `now`; implementations may coarsen the resolution.
    fn touch(&self, id: AccessTokenId, now: Timestamp) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Account facts that can refuse an otherwise valid token.
pub trait AccessTokenOwnerRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    fn password_change_required(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn is_suspended(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
