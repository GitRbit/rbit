//! Authenticator port: the existing sealed-secret statements and their guarded writes.

use std::future::Future;

use rbit_core::AccountId;
use rbit_identity_models::two_factor::{RecoveryDigest, TwoFactorRecord, TwoFactorRepository};

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
