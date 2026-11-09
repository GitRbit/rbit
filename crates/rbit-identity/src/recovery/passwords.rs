//! Replacement passwords from a reset link or an administrator.
//!
//! Each preparation reserves the caller's session-finalization owner before its first
//! await and reports a refusal only at the admission boundary, as before. Commits are
//! separate so callers run them inside their tracked finalization worker.

use std::{convert::Infallible, future::Future};

use zeroize::Zeroizing;

use super::{
    MAX_PASSWORD_LEN, RecoveryAccount, RecoveryCredentialRepository, RecoveryError, RecoveryRejection, RecoveryRepository, RecoveryResult,
    link_digest, links::reset_account, require_admin,
};
use crate::{PasswordLength, PasswordWork, TokenDigest, User, classify_password_length};
use rbit_contracts::{AccountId, Timestamp};

/// A validated, hashed reset awaiting its single-use redemption.
#[derive(Debug)]
pub struct PreparedReset<Ownership> {
    pub ownership: Ownership,
    pub account: RecoveryAccount,
    pub digest: TokenDigest,
    pub hash: String,
}

/// A validated, hashed administrator-set password awaiting its commit.
#[derive(Debug)]
pub struct PreparedAdminPassword<Ownership> {
    pub ownership: Ownership,
    pub account: RecoveryAccount,
    pub hash: String,
}

/// Redeems the link and replaces the password in one transaction. [`RecoveryError::NotFound`]
/// when a concurrent redemption, email change or suspension got there first.
pub async fn commit_reset<R: RecoveryRepository>(recovery: &R, digest: TokenDigest, hash: &str) -> RecoveryResult<(), R::Error> {
    recovery.reset_password(digest, hash, Timestamp::now()).await.map_err(RecoveryError::Store)?.ok_or(RecoveryError::NotFound)?;
    Ok(())
}

/// Stores an administrator-set password and invalidates outstanding reset links.
pub async fn commit_admin_password<C: RecoveryCredentialRepository>(
    credentials: &C,
    account: AccountId,
    hash: &str,
) -> RecoveryResult<(), C::Error> {
    credentials.set_password_hash(account, hash).await.map_err(RecoveryError::Store)
}

#[derive(Debug)]
pub struct RecoveryPasswords<'a, R, C> {
    recovery: &'a R,
    credentials: &'a C,
    passwords: &'a PasswordWork,
    min_password_length: usize,
}

impl<'a, R, C> RecoveryPasswords<'a, R, C>
where
    R: RecoveryRepository,
    C: RecoveryCredentialRepository<Error = R::Error>,
{
    pub fn new(recovery: &'a R, credentials: &'a C, passwords: &'a PasswordWork, min_password_length: usize) -> Self {
        Self { recovery, credentials, passwords, min_password_length }
    }

    /// Checks the link, then the sign-up password rules; an invalid replacement leaves the
    /// link usable. An administrator-provided initial password must actually change.
    pub fn prepare_reset<'s, O: 's, V: 's>(
        &'s self,
        token: &'s str,
        new_password: String,
        reserve: impl FnOnce() -> Result<O, V> + 's,
    ) -> impl Future<Output = RecoveryResult<PreparedReset<O>, R::Error, Infallible, V>> + 's {
        let mut new_password = Zeroizing::new(new_password);
        async move {
            let ownership = reserve();
            let account = reset_account::<R, Infallible, V>(self.recovery, token).await?;
            self.validate::<Infallible, V>(&new_password)?;
            let initial = self.credentials.password_change_required(account.id).await.map_err(RecoveryError::Store)?;
            let attempt = self.passwords.begin().map_err(RecoveryError::Admission)?;
            let ownership = ownership.map_err(RecoveryError::Reserve)?;
            let hash = if initial {
                let current =
                    self.credentials.password_hash(account.id).await.map_err(RecoveryError::Store)?.ok_or(RecoveryError::NotFound)?;
                attempt
                    .hash_if_different(std::mem::take(&mut *new_password), current)
                    .await
                    .map_err(RecoveryError::Password)?
                    .ok_or(RecoveryError::Rejected(RecoveryRejection::SameAsInitial))?
            } else {
                attempt.hash(std::mem::take(&mut *new_password)).await.map_err(RecoveryError::Password)?
            };
            let digest = link_digest(token).ok_or(RecoveryError::NotFound)?;
            Ok(PreparedReset { ownership, account, digest, hash })
        }
    }

    /// Sets another user's password; administrators only, never their own.
    pub fn prepare_admin_password<'s, O: 's, V: 's>(
        &'s self,
        actor: &'s User,
        target: AccountId,
        new_password: String,
        reserve: impl FnOnce() -> Result<O, V> + 's,
    ) -> impl Future<Output = RecoveryResult<PreparedAdminPassword<O>, R::Error, Infallible, V>> + 's {
        let mut new_password = Zeroizing::new(new_password);
        async move {
            require_admin::<R::Error, Infallible, V>(actor)?;
            let ownership = reserve();
            let account = self.recovery.find_account(target).await.map_err(RecoveryError::Store)?.ok_or(RecoveryError::NotFound)?;
            if account.id == actor.id() {
                return Err(RecoveryError::Rejected(RecoveryRejection::OwnPassword));
            }
            self.validate::<Infallible, V>(&new_password)?;
            let attempt = self.passwords.begin().map_err(RecoveryError::Admission)?;
            let ownership = ownership.map_err(RecoveryError::Reserve)?;
            let hash = attempt.hash(std::mem::take(&mut *new_password)).await.map_err(RecoveryError::Password)?;
            Ok(PreparedAdminPassword { ownership, account, hash })
        }
    }

    /// The sign-up password rules.
    fn validate<T, V>(&self, password: &str) -> RecoveryResult<(), R::Error, T, V> {
        let min = self.min_password_length;
        match classify_password_length(password, min, MAX_PASSWORD_LEN) {
            PasswordLength::TooShort => Err(RecoveryError::Rejected(RecoveryRejection::PasswordTooShort { min })),
            PasswordLength::TooLong => Err(RecoveryError::Rejected(RecoveryRejection::PasswordTooLong)),
            PasswordLength::Accepted => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rule_texts_match_sign_up() {
        assert_eq!(RecoveryRejection::PasswordTooShort { min: 10 }.to_string(), "password must be at least 10 characters");
        assert_eq!(RecoveryRejection::PasswordTooLong.to_string(), "password must be at most 256 characters");
        assert_eq!(classify_password_length("ééé", 3, MAX_PASSWORD_LEN), PasswordLength::Accepted, "length counts characters");
        assert_eq!(classify_password_length(&"x".repeat(MAX_PASSWORD_LEN + 1), 3, MAX_PASSWORD_LEN), PasswordLength::TooLong);
    }
}
