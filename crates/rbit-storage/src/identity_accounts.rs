//! Account persistence adapter delegates transaction and race authority to existing SQL.

use std::future::Future;

use rbit_identity_models::{
    User,
    accounts::{AccountRepository, LoginRecord, NewUser},
};

use crate::{AccountStore, StorageError};

impl AccountRepository for AccountStore {
    type Error = StorageError;

    fn count_users(&self) -> impl Future<Output = Result<i64, Self::Error>> + Send {
        AccountStore::count_users(self)
    }

    fn find_login(&self, login: &str) -> impl Future<Output = Result<Option<LoginRecord>, Self::Error>> + Send {
        AccountStore::find_login(self, login)
    }

    fn create_user(&self, new: NewUser) -> impl Future<Output = Result<User, Self::Error>> + Send {
        AccountStore::create_user(self, new)
    }

    fn create_user_with_initial_password(&self, new: NewUser) -> impl Future<Output = Result<User, Self::Error>> + Send {
        AccountStore::create_user_with_initial_password(self, new)
    }

    fn register_user(&self, new: NewUser, registration_open: bool) -> impl Future<Output = Result<User, Self::Error>> + Send {
        AccountStore::register_user(self, new, registration_open)
    }
}

impl rbit_identity_models::credential::CredentialLoginRepository for AccountStore {
    type Error = StorageError;
    fn find_login_snapshot(
        &self,
        login: &str,
    ) -> impl Future<Output = Result<Option<rbit_identity_models::credential::CredentialLoginSnapshot>, Self::Error>> + Send {
        AccountStore::find_login_snapshot(self, login)
    }
}
