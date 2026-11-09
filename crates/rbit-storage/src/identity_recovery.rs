//! Recovery ports retain the existing single-statement consume and reset transactions.

use std::{collections::HashSet, future::Future};

use rbit_core::{AccountId, Email, Timestamp};
use rbit_identity_models::{
    TokenDigest,
    recovery::{EmailTokenRecord, NewEmailToken, RecoveryAccount, RecoveryCredentialRepository, RecoveryRepository, TokenPurpose},
};

use crate::{
    AccountStore, StorageError,
    sqlite::{profiles::ProfileStore, recovery::RecoveryStore},
};

impl RecoveryRepository for RecoveryStore {
    type Error = StorageError;

    fn insert(&self, token: &NewEmailToken) -> impl Future<Output = Result<(), Self::Error>> + Send {
        RecoveryStore::insert(self, token)
    }
    async fn find_valid(
        &self,
        digest: TokenDigest,
        purpose: TokenPurpose,
        now: Timestamp,
    ) -> Result<Option<EmailTokenRecord>, Self::Error> {
        RecoveryStore::find_valid(self, &digest, purpose, now).await
    }
    async fn consume(&self, digest: TokenDigest, purpose: TokenPurpose, now: Timestamp) -> Result<Option<EmailTokenRecord>, Self::Error> {
        RecoveryStore::consume(self, &digest, purpose, now).await
    }
    fn revoke_all(&self, account: AccountId, purpose: TokenPurpose) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        RecoveryStore::revoke_all(self, account, purpose)
    }
    fn find_account_by_email(&self, email: &Email) -> impl Future<Output = Result<Option<RecoveryAccount>, Self::Error>> + Send {
        RecoveryStore::find_account_by_email(self, email)
    }
    fn find_account(&self, account: AccountId) -> impl Future<Output = Result<Option<RecoveryAccount>, Self::Error>> + Send {
        RecoveryStore::find_account(self, account)
    }
    fn mark_verified(&self, account: AccountId, email: &Email, at: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        RecoveryStore::mark_verified(self, account, email, at)
    }
    fn verified_among(&self, accounts: &[AccountId]) -> impl Future<Output = Result<HashSet<AccountId>, Self::Error>> + Send {
        RecoveryStore::verified_among(self, accounts)
    }
    async fn reset_password(&self, digest: TokenDigest, hash: &str, now: Timestamp) -> Result<Option<EmailTokenRecord>, Self::Error> {
        RecoveryStore::reset_password(self, &digest, hash, now).await
    }
}

/// The account and login stores that hold password state, as one recovery port.
#[derive(Debug, Clone)]
pub struct RecoveryCredentials {
    pub accounts: AccountStore,
    pub profiles: ProfileStore,
}

impl RecoveryCredentialRepository for RecoveryCredentials {
    type Error = StorageError;
    fn password_change_required(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.accounts.password_change_required(account)
    }
    fn password_hash(&self, account: AccountId) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send {
        self.profiles.password_hash(account)
    }
    fn set_password_hash(&self, account: AccountId, hash: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.profiles.set_password_hash(account, hash)
    }
}
