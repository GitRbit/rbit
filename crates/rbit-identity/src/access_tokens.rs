//! Personal access token application: issuing, listing, revoking and resolving secrets.
//!
//! Policy lives here, independent of SQL, HTTP and audit: name and expiry validation, the
//! per-account limit, secret generation (only the digest and a short hint are stored),
//! owner-scoped revocation, and the checks that refuse a presented secret. Callers keep
//! audit, tracing and error mapping. Scopes pass through uninterpreted.

mod authentication;
mod rejection;

pub use authentication::TokenAuthentication;
pub use rbit_identity_models::access_tokens::{
    AccessTokenId, AccessTokenOwnerRepository, AccessTokenRecord, AccessTokenRepository, NewAccessTokenRecord, ResolvedAccessToken,
};
pub use rejection::AccessTokenRejection;

use crate::{AccessTokenSecret, User};
use rbit_contracts::Timestamp;

/// Longest accepted token name, in characters, after trimming.
pub const MAX_NAME_LEN: usize = 64;
/// Tokens one account may hold at once.
pub const MAX_TOKENS_PER_ACCOUNT: i64 = 50;
/// Allowed values for `expires_in_days` (`None` = never). The UI renders these as choices.
pub const EXPIRY_CHOICES: &[Option<u32>] = &[Some(7), Some(30), Some(90), Some(365), None];
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

#[derive(Debug)]
pub enum AccessTokenError<Token, Account> {
    Token(Token),
    Account(Account),
    /// The request was refused; the rejection's text is user-facing.
    Rejected(AccessTokenRejection),
    /// The token does not exist or belongs to another account.
    NotFound,
}

pub type AccessTokenResult<T, Token, Account> = Result<T, AccessTokenError<Token, Account>>;

#[derive(Debug)]
pub struct AccessTokenApplication<'a, T, A> {
    tokens: &'a T,
    accounts: &'a A,
}

impl<'a, T: AccessTokenRepository, A: AccessTokenOwnerRepository> AccessTokenApplication<'a, T, A> {
    pub fn new(tokens: &'a T, accounts: &'a A) -> Self {
        Self { tokens, accounts }
    }

    /// Returns the stored metadata and the plaintext secret (shown to the user exactly once).
    pub async fn create(
        &self,
        owner: &User,
        name: &str,
        scope: T::Scope,
        expires_in_days: Option<u32>,
    ) -> AccessTokenResult<(AccessTokenRecord<T::Scope>, String), T::Error, A::Error> {
        let name = name.trim();
        let name_len = name.chars().count();
        if name_len == 0 {
            return Err(AccessTokenError::Rejected(AccessTokenRejection::EmptyName));
        }
        if name_len > MAX_NAME_LEN {
            return Err(AccessTokenError::Rejected(AccessTokenRejection::NameTooLong));
        }
        if !EXPIRY_CHOICES.contains(&expires_in_days) {
            return Err(AccessTokenError::Rejected(AccessTokenRejection::UnofferedExpiry));
        }
        let secret = AccessTokenSecret::generate();
        let expires_at = expires_in_days.map(|days| Timestamp(Timestamp::now().seconds() + i64::from(days) * SECONDS_PER_DAY));
        let new = NewAccessTokenRecord {
            account_id: owner.id(),
            name: name.to_owned(),
            digest: secret.digest(),
            hint: secret.hint(),
            scope,
            expires_at,
        };
        let token = self
            .tokens
            .create_with_limit(new, MAX_TOKENS_PER_ACCOUNT)
            .await
            .map_err(AccessTokenError::Token)?
            .ok_or(AccessTokenError::Rejected(AccessTokenRejection::LimitReached))?;
        Ok((token, secret.encode()))
    }

    /// The owner's tokens, newest first.
    pub async fn list(&self, owner: &User) -> AccessTokenResult<Vec<AccessTokenRecord<T::Scope>>, T::Error, A::Error> {
        self.tokens.list_for_account(owner.id()).await.map_err(AccessTokenError::Token)
    }

    /// Deletes one of the owner's tokens and returns its name, when it was still listed,
    /// for the caller's audit entry. [`AccessTokenError::NotFound`] if it is not theirs.
    pub async fn revoke(&self, owner: &User, id: AccessTokenId) -> AccessTokenResult<Option<String>, T::Error, A::Error> {
        let listed = self.tokens.list_for_account(owner.id()).await.map_err(AccessTokenError::Token)?;
        let name = listed.into_iter().find(|token| token.id == id).map(|token| token.name);
        if !self.tokens.delete(owner.id(), id).await.map_err(AccessTokenError::Token)? {
            return Err(AccessTokenError::NotFound);
        }
        Ok(name)
    }
}
