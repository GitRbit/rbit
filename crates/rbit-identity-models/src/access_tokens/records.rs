//! Stable token metadata. The secret itself is shown once at creation and never stored.

use crate::TokenDigest;
use rbit_contracts::{AccountId, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessTokenId(pub i64);

/// Token metadata, generic over the authorization scope it carries.
#[derive(Debug, Clone)]
pub struct AccessTokenRecord<Scope> {
    pub id: AccessTokenId,
    pub account_id: AccountId,
    pub name: String,
    /// The last characters of the secret, so users can tell tokens apart.
    pub hint: String,
    pub scope: Scope,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
    pub expires_at: Option<Timestamp>,
}

impl<Scope> AccessTokenRecord<Scope> {
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|at| at <= Timestamp::now())
    }
}

/// Everything needed to store a token. The secret itself never reaches this layer.
#[derive(Debug)]
pub struct NewAccessTokenRecord<Scope> {
    pub account_id: AccountId,
    pub name: String,
    pub digest: TokenDigest,
    pub hint: String,
    pub scope: Scope,
    pub expires_at: Option<Timestamp>,
}
