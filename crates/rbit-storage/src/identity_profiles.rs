//! Profile ports retain the existing SQL statements and their compare-and-set guards.

use std::future::Future;

use rbit_core::{AccountId, AccountName, Email};
use rbit_identity_models::profiles::{AccountProfile, AccountRenameRepository, ProfileRepository};

use crate::{AccountStore, StorageError, sqlite::profiles::ProfileStore};

impl ProfileRepository for ProfileStore {
    type Error = StorageError;

    fn find_profile(&self, account: AccountId) -> impl Future<Output = Result<Option<AccountProfile>, Self::Error>> + Send {
        ProfileStore::find(self, account)
    }
    fn update_profile(&self, account: AccountId, profile: &AccountProfile) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ProfileStore::update(self, account, profile)
    }
    fn password_hash(&self, account: AccountId) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send {
        ProfileStore::password_hash(self, account)
    }
    fn set_email_if_password(
        &self,
        account: AccountId,
        email: &Email,
        expected: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        ProfileStore::set_email_if_password(self, account, email, expected)
    }
    fn set_password_hash_if_current(
        &self,
        account: AccountId,
        hash: &str,
        expected: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        ProfileStore::set_password_hash_if_current(self, account, hash, expected)
    }
}

impl AccountRenameRepository for AccountStore {
    type Error = StorageError;
    fn rename_if_password(
        &self,
        account: AccountId,
        name: &AccountName,
        expected: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        AccountStore::rename_if_password(self, account, name, expected)
    }
}
