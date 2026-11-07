//! Trusted provider-account persistence facts; these are not authentication receipts.
use super::{CreatedAccountSnapshot, NewUser};
use crate::{
    User,
    credential::{CredentialEpoch, CredentialFence},
};
use rbit_contracts::AccountId;
use std::future::Future;

/// The concrete adapter captures the already-verified provider identity. Creation,
/// identity linking and original epoch must commit in one authoritative transaction.
pub trait ProviderCreationRepository: Sync {
    type Error;
    fn create_original(
        &self,
        new: NewUser,
        registration_open: bool,
    ) -> impl Future<Output = Result<CreatedAccountSnapshot, Self::Error>> + Send;
}

/// Informational user and fixed fence from the FIRST acceptance statement. The user
/// is not final issuance authority; the fenced issuer reloads current flags once.
#[derive(Debug)]
pub struct ProviderAccountSnapshot {
    pub user: User,
    pub fence: CredentialFence,
}

/// The adapter retains its existing identity DTO while capturing the account fence
/// in the SAME first linked-account lookup, without reading password material.
#[derive(Debug)]
pub struct LinkedProviderSnapshot<I> {
    pub identity: I,
    pub account: ProviderAccountSnapshot,
}

/// Trusted composition port. Only a verified remote callback may start acceptance.
/// A port implementation is not an adversarial cryptographic trust boundary.
pub trait ProviderLoginRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    type Identity: Send;
    fn find_linked_snapshot(
        &self,
        provider: &str,
        subject: &str,
    ) -> impl Future<Output = Result<Option<LinkedProviderSnapshot<Self::Identity>>, Self::Error>> + Send;
    fn find_verified_email_snapshot(
        &self,
        email: &str,
    ) -> impl Future<Output = Result<Option<ProviderAccountSnapshot>, Self::Error>> + Send;
    /// Existing final account lookup, constrained by the captured original epoch,
    /// current pending intent and active account flags. Never restamp a current user.
    fn find_active_at_epoch(
        &self,
        account: AccountId,
        original: CredentialEpoch,
    ) -> impl Future<Output = Result<Option<User>, Self::Error>> + Send;
}
