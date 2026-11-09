//! Static persistence port. Implementations own single-use atomicity: consuming a link
//! and redeeming a reset are each one guarded write, so concurrent requests cannot both win.

use std::{collections::HashSet, future::Future};

use super::{EmailTokenRecord, NewEmailToken, RecoveryAccount, TokenPurpose};
use crate::{Email, TokenDigest};
use rbit_contracts::{AccountId, Timestamp};

pub trait RecoveryRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Stores a token (and may sweep expired ones).
    fn insert(&self, token: &NewEmailToken) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// The token, if it would be honoured at `now`. Does not use it up.
    fn find_valid(
        &self,
        digest: TokenDigest,
        purpose: TokenPurpose,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<EmailTokenRecord>, Self::Error>> + Send;
    /// Marks the token used and returns it, if it was honoured at `now`; at most one of
    /// two concurrent calls gets `Some`.
    fn consume(
        &self,
        digest: TokenDigest,
        purpose: TokenPurpose,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<EmailTokenRecord>, Self::Error>> + Send;
    /// Uses up every outstanding token of `purpose` for the account.
    fn revoke_all(&self, account: AccountId, purpose: TokenPurpose) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    /// The user with this sign-in address (case-insensitive), if any.
    fn find_account_by_email(&self, email: &Email) -> impl Future<Output = Result<Option<RecoveryAccount>, Self::Error>> + Send;
    fn find_account(&self, account: AccountId) -> impl Future<Output = Result<Option<RecoveryAccount>, Self::Error>> + Send;
    /// Records verification, provided the account's address is still `email`.
    fn mark_verified(&self, account: AccountId, email: &Email, at: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn verified_among(&self, accounts: &[AccountId]) -> impl Future<Output = Result<HashSet<AccountId>, Self::Error>> + Send;
    /// Redeems a reset link, replaces the password, verifies the address and revokes the
    /// other reset links in one transaction; `None` if the link is no longer honoured.
    fn reset_password(
        &self,
        digest: TokenDigest,
        hash: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<EmailTokenRecord>, Self::Error>> + Send;
}

/// Password state that a reset or an administrator's password change reads and replaces.
pub trait RecoveryCredentialRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    /// Whether the account must replace an administrator-provided initial password.
    fn password_change_required(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// The stored password hash, or `None` if the user does not exist.
    fn password_hash(&self, account: AccountId) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    /// Stores a new hash and revokes outstanding reset links atomically.
    fn set_password_hash(&self, account: AccountId, hash: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
