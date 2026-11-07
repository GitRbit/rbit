//! Same-transaction account creation facts, not a wire credential or auth receipt.
use super::{AccountRepository, NewUser};
use crate::{User, credential::CredentialEpoch};
use std::future::Future;

#[derive(Debug)]
pub struct CreatedAccountSnapshot {
    pub user: User,
    pub epoch: CredentialEpoch,
}

/// Creation epoch must come from the SAME authoritative insertion transaction.
/// Never read the current epoch later to rehabilitate a concurrently reset account.
pub trait AccountCreationRepository: AccountRepository {
    fn create_user_original(&self, new: NewUser) -> impl Future<Output = Result<CreatedAccountSnapshot, Self::Error>> + Send;
    fn create_user_with_initial_password_original(
        &self,
        new: NewUser,
    ) -> impl Future<Output = Result<CreatedAccountSnapshot, Self::Error>> + Send;
    fn register_user_original(
        &self,
        new: NewUser,
        registration_open: bool,
    ) -> impl Future<Output = Result<CreatedAccountSnapshot, Self::Error>> + Send;
}
