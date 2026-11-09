//! Resolving a presented Git/API credential to its owner and scope.

use super::{
    AccessTokenApplication, AccessTokenError, AccessTokenId, AccessTokenOwnerRepository, AccessTokenRepository, AccessTokenResult,
};
use crate::{AccessTokenSecret, User};
use rbit_contracts::Timestamp;

/// The outcome of presenting a secret. Only `Accepted` authenticates.
#[derive(Debug)]
pub enum TokenAuthentication<Scope, Error> {
    /// The token is live and its owner may use it. `bookkeeping` is the result of
    /// recording last use: a failure there must not turn a valid credential away.
    Accepted { user: User, scope: Scope, token: AccessTokenId, bookkeeping: Result<(), Error> },
    /// The owner is suspended; distinguished only so the caller can log it.
    SuspendedOwner { user: User },
    /// Unknown, malformed or expired, or the owner must first change their password.
    Refused,
}

impl<T: AccessTokenRepository, A: AccessTokenOwnerRepository> AccessTokenApplication<'_, T, A> {
    /// Checks run in a fixed order: format, digest lookup, expiry, required password
    /// change, suspension. Last use is recorded only for an accepted token.
    pub async fn authenticate(&self, secret: &str) -> AccessTokenResult<TokenAuthentication<T::Scope, T::Error>, T::Error, A::Error> {
        let Some(secret) = AccessTokenSecret::parse(secret) else {
            return Ok(TokenAuthentication::Refused);
        };
        let Some((token, user)) = self.tokens.find_by_digest(secret.digest()).await.map_err(AccessTokenError::Token)? else {
            return Ok(TokenAuthentication::Refused);
        };
        if token.is_expired() {
            return Ok(TokenAuthentication::Refused);
        }
        if self.accounts.password_change_required(user.id()).await.map_err(AccessTokenError::Account)? {
            return Ok(TokenAuthentication::Refused);
        }
        if self.accounts.is_suspended(user.id()).await.map_err(AccessTokenError::Account)? {
            return Ok(TokenAuthentication::SuspendedOwner { user });
        }
        let bookkeeping = self.tokens.touch(token.id, Timestamp::now()).await;
        Ok(TokenAuthentication::Accepted { user, scope: token.scope, token: token.id, bookkeeping })
    }
}
