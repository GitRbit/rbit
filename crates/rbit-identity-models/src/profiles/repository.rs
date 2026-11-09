//! Compare-and-set ports: each credential change commits only while the password hash
//! the caller verified is still current, so a concurrent reset or change wins.

use std::future::Future;

use super::AccountProfile;
use crate::{AccountName, Email};
use rbit_contracts::AccountId;

pub trait ProfileRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// The account's profile, or `None` if the account does not exist.
    fn find_profile(&self, account: AccountId) -> impl Future<Output = Result<Option<AccountProfile>, Self::Error>> + Send;
    /// Replaces every profile field atomically.
    fn update_profile(&self, account: AccountId, profile: &AccountProfile) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// The stored password hash, or `None` if the user does not exist.
    fn password_hash(&self, account: AccountId) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    /// Changes the email only while `expected` is the current hash; a different address
    /// starts unverified and outstanding email links are retired in the same write.
    fn set_email_if_password(
        &self,
        account: AccountId,
        email: &Email,
        expected: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Replaces the hash and revokes reset links only while `expected` is current.
    fn set_password_hash_if_current(
        &self,
        account: AccountId,
        hash: &str,
        expected: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

pub trait AccountRenameRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    /// Renames a non-suspended user only while `expected` is the current hash.
    fn rename_if_password(
        &self,
        account: AccountId,
        name: &AccountName,
        expected: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
