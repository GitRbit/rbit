//! Access token ports retain the existing SQL statements and their atomicity.

use std::future::Future;

use rbit_core::{AccessToken, AccessTokenId, AccountId, Timestamp, TokenScope, User};
use rbit_identity_models::{
    TokenDigest,
    access_tokens::{AccessTokenOwnerRepository, AccessTokenRepository},
};

use crate::{AccessTokenStore, AccountStore, NewAccessToken, StorageError};

impl AccessTokenRepository for AccessTokenStore {
    type Error = StorageError;
    type Scope = TokenScope;

    fn create_with_limit(&self, new: NewAccessToken, limit: i64) -> impl Future<Output = Result<Option<AccessToken>, Self::Error>> + Send {
        AccessTokenStore::create_with_limit(self, new, limit)
    }
    fn list_for_account(&self, account: AccountId) -> impl Future<Output = Result<Vec<AccessToken>, Self::Error>> + Send {
        AccessTokenStore::list_for_account(self, account)
    }
    fn delete(&self, account: AccountId, id: AccessTokenId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        AccessTokenStore::delete(self, account, id)
    }
    async fn find_by_digest(&self, digest: TokenDigest) -> Result<Option<(AccessToken, User)>, Self::Error> {
        AccessTokenStore::find_by_digest(self, &digest).await
    }
    fn touch(&self, id: AccessTokenId, now: Timestamp) -> impl Future<Output = Result<(), Self::Error>> + Send {
        AccessTokenStore::touch(self, id, now)
    }
}

impl AccessTokenOwnerRepository for AccountStore {
    type Error = StorageError;
    fn password_change_required(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        AccountStore::password_change_required(self, account)
    }
    fn is_suspended(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        AccountStore::is_suspended(self, account)
    }
}
