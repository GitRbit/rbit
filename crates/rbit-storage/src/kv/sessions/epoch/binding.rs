//! Fixed-width account/epoch/incarnation binding; never a serialized auth receipt.
use crate::StorageError;
use rbit_core::AccountId;
use rbit_identity_models::credential::CredentialEpoch;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Binding {
    pub account: AccountId,
    pub epoch: CredentialEpoch,
    incarnation: [u8; 16],
}
impl Binding {
    pub(super) fn with_random(
        account: AccountId,
        epoch: CredentialEpoch,
        random: impl FnOnce(&mut [u8]) -> Result<(), ()>,
    ) -> Result<Self, StorageError> {
        let mut incarnation = [0; 16];
        random(&mut incarnation).map_err(|()| corrupt())?;
        Ok(Self { account, epoch, incarnation })
    }
    pub(super) fn promoted(self, epoch: CredentialEpoch) -> Self {
        Self { epoch, ..self }
    }
    pub(super) fn same_incarnation(self, other: Self) -> bool {
        self.account == other.account && self.incarnation == other.incarnation
    }
    pub fn encode(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&self.account.0.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.epoch.get().to_be_bytes());
        bytes[16..].copy_from_slice(&self.incarnation);
        bytes
    }
    pub fn decode(bytes: &[u8; 32], account: AccountId) -> Result<Self, StorageError> {
        let stored_account = AccountId(i64::from_be_bytes(bytes[..8].try_into().map_err(|_| corrupt())?));
        let epoch = i64::from_be_bytes(bytes[8..16].try_into().map_err(|_| corrupt())?);
        let epoch = CredentialEpoch::new(epoch).ok_or_else(corrupt)?;
        if stored_account != account {
            return Err(corrupt());
        }
        Ok(Self { account, epoch, incarnation: bytes[16..].try_into().map_err(|_| corrupt())? })
    }
}
pub(super) fn corrupt() -> StorageError {
    StorageError::Corrupt("invalid session epoch binding".into())
}
