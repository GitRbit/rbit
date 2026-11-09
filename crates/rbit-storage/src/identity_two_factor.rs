//! Authenticator and passkey ports: the existing sealed-secret and passkey statements and
//! their guarded writes.

use std::future::Future;

use rbit_core::AccountId;
use rbit_identity_models::{
    passkeys::{NewPasskey, PasskeyAdded, PasskeyRecord, PasskeyRepository},
    two_factor::{RecoveryDigest, TwoFactorRecord, TwoFactorRepository},
};

use crate::StorageError;
use crate::sqlite::two_factor::TwoFactorStore;

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
}
