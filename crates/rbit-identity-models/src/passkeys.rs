//! Registered passkey records and static persistence ports.
//!
//! Advancing a passkey's signature counter is a compare-and-set in the implementation, so
//! one assertion can never be honoured twice.

use std::future::Future;

use crate::credential::CredentialProofWrite;
use crate::two_factor::RecoveryDigest;
use rbit_contracts::{AccountId, Timestamp};

/// A registered passkey or security key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasskeyRecord {
    pub id: i64,
    pub credential_id: Vec<u8>,
    /// The COSE key as registered.
    pub public_key: Vec<u8>,
    pub algorithm: i64,
    pub sign_count: u32,
    pub transports: Vec<String>,
    pub name: String,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

/// A passkey to register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewPasskey<'a> {
    pub credential_id: &'a [u8],
    pub public_key: &'a [u8],
    pub algorithm: i64,
    pub sign_count: u32,
    pub transports: &'a [String],
    pub name: &'a str,
}

/// What adding a passkey did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasskeyAdded {
    /// Added to an account that already had two-factor authentication.
    Added,
    /// Added as the account's first factor: the recovery codes given were stored.
    FirstFactor,
    /// This credential is already registered (to this or another account).
    Duplicate,
}

pub trait PasskeyRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn passkeys(&self, account: AccountId) -> impl Future<Output = Result<Vec<PasskeyRecord>, Self::Error>> + Send;
    fn find_passkey(
        &self,
        account: AccountId,
        credential_id: &[u8],
    ) -> impl Future<Output = Result<Option<PasskeyRecord>, Self::Error>> + Send;
    /// Adds a passkey; when it is the account's first factor, `recovery` becomes its codes.
    fn add_passkey(
        &self,
        account: AccountId,
        passkey: &NewPasskey<'_>,
        recovery: &[RecoveryDigest],
    ) -> impl Future<Output = Result<PasskeyAdded, Self::Error>> + Send;
    /// Removes passkey `id` of the account, and the recovery codes too if no factor
    /// remains. `true` if it existed.
    fn remove_passkey(&self, account: AccountId, id: i64) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// One-use assertion claims; implementations decide the write mode.
pub trait PasskeyClaims: Sync {
    type Error: Send;
    /// Moves passkey `id`'s counter from `seen` (the value the signature was checked
    /// against) to `next`; `Rejected` if another sign-in got there first.
    fn use_passkey(
        &self,
        account: AccountId,
        id: i64,
        seen: u32,
        next: u32,
    ) -> impl Future<Output = Result<CredentialProofWrite, Self::Error>> + Send;
}
