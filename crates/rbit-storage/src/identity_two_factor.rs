//! Authenticator and passkey ports: the existing sealed-secret and passkey statements and
//! their guarded writes.

use std::future::Future;

use rbit_core::{AccountId, User};
use rbit_identity_models::{
    credential::CredentialEpoch,
    passkeys::{NewPasskey, PasskeyAdded, PasskeyRecord, PasskeyRepository},
    two_factor::{RecoveryDigest, SecondStepAccounts, TwoFactorRecord, TwoFactorRepository},
};

use crate::sqlite::two_factor::TwoFactorStore;
use crate::{AccountStore, StorageError};

impl TwoFactorRepository for TwoFactorStore {
    type Error = StorageError;

    fn find(&self, account: AccountId) -> impl Future<Output = Result<Option<TwoFactorRecord>, Self::Error>> + Send {
        TwoFactorStore::find(self, account)
    }
    fn begin_enrolment(&self, account: AccountId, secret: &[u8]) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        TwoFactorStore::begin_enrolment(self, account, secret)
    }
    fn cancel_enrolment(&self, account: AccountId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        TwoFactorStore::cancel_enrolment(self, account)
    }
    fn confirm(
        &self,
        account: AccountId,
        secret: &[u8],
        step: u64,
        recovery: &[RecoveryDigest],
    ) -> impl Future<Output = Result<Option<bool>, Self::Error>> + Send {
        TwoFactorStore::confirm(self, account, secret, step, recovery)
    }
    fn replace_recovery_codes(
        &self,
        account: AccountId,
        digests: &[RecoveryDigest],
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        TwoFactorStore::replace_recovery_codes(self, account, digests)
    }
    fn disable(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        TwoFactorStore::disable(self, account)
    }
}

impl PasskeyRepository for TwoFactorStore {
    type Error = StorageError;

    fn passkeys(&self, account: AccountId) -> impl Future<Output = Result<Vec<PasskeyRecord>, Self::Error>> + Send {
        TwoFactorStore::passkeys(self, account)
    }
    fn find_passkey(
        &self,
        account: AccountId,
        credential_id: &[u8],
    ) -> impl Future<Output = Result<Option<PasskeyRecord>, Self::Error>> + Send {
        TwoFactorStore::find_passkey(self, account, credential_id)
    }
    fn add_passkey(
        &self,
        account: AccountId,
        passkey: &NewPasskey<'_>,
        recovery: &[RecoveryDigest],
    ) -> impl Future<Output = Result<PasskeyAdded, Self::Error>> + Send {
        TwoFactorStore::add_passkey(self, account, passkey, recovery)
    }
    fn remove_passkey(&self, account: AccountId, id: i64) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        TwoFactorStore::remove_passkey(self, account, id)
    }
}

impl SecondStepAccounts for AccountStore {
    type Error = StorageError;

    fn find_active_user(&self, account: AccountId) -> impl Future<Output = Result<Option<User>, Self::Error>> + Send {
        AccountStore::find_active_user(self, account)
    }
    async fn find_user_at_epoch(&self, account: AccountId, original: CredentialEpoch) -> Result<Option<User>, Self::Error> {
        Ok(AccountStore::find_session_user_at_epoch(self, account, false, original).await?.map(|row| row.user))
    }
    async fn password_hash(&self, account: AccountId) -> Result<Option<String>, Self::Error> {
        Ok(AccountStore::find_login_by_id(self, account).await?.map(|record| record.password_hash))
    }
}
