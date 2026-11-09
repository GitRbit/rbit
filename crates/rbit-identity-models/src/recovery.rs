//! One-use email link records (password reset, address verification) and their ports.
//!
//! A link carries 256 random bits; only the SHA-256 digest is stored, bound to a purpose,
//! an expiry and the address it was sent to. A link is honoured only while unused,
//! unexpired and still addressed to the account's current email.

mod repository;

pub use repository::{RecoveryCredentialRepository, RecoveryRepository};

use crate::{AccountName, Email, TokenDigest};
use rbit_contracts::{AccountId, Timestamp};

/// What a token lets its holder do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPurpose {
    /// Choose a new password without knowing the old one.
    Reset,
    /// Prove control of the address the token was sent to.
    Verify,
}

impl TokenPurpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reset => "reset",
            Self::Verify => "verify",
        }
    }
}

/// A token to store. `digest` is the SHA-256 of the secret that went into the link.
#[derive(Debug, Clone)]
pub struct NewEmailToken {
    pub digest: TokenDigest,
    pub account_id: AccountId,
    pub purpose: TokenPurpose,
    /// The address the link is sent to.
    pub email: Email,
    pub expires_at: Timestamp,
}

/// A token that is currently honoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailTokenRecord {
    pub account_id: AccountId,
    pub email: Email,
    pub expires_at: Timestamp,
}

/// The parts of a user that recovery flows need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryAccount {
    pub id: AccountId,
    pub name: AccountName,
    pub email: Email,
    pub suspended: bool,
    pub email_verified_at: Option<Timestamp>,
}
