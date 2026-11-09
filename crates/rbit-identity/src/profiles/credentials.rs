//! Credential changes verify the current password, then commit only while the verified
//! hash is still current. Throttle checks keep their original order and charging.

use std::future::Future;

use zeroize::Zeroizing;

use super::{
    AccountRenameRepository, CredentialThrottle, MAX_PASSWORD_LEN, ProfileApplication, ProfileError, ProfileRejection, ProfileRepository,
    ProfileResult,
};
use crate::{AccountName, Email, PasswordError, PasswordLength, User, classify_password_length};
use rbit_contracts::AccountId;

/// A verified password change awaiting its commit, holding the caller's reservation.
#[derive(Debug)]
pub struct PreparedPasswordChange<Ownership> {
    pub ownership: Ownership,
    /// The new password's hash.
    pub hash: String,
    /// The hash that was verified; the commit succeeds only while it is current.
    pub verified: Zeroizing<String>,
}

/// Commits a prepared change. [`ProfileError::InvalidCredentials`] when a concurrent reset
/// or change replaced the verified hash first.
pub async fn commit_password_change<P: ProfileRepository>(
    profiles: &P,
    account: AccountId,
    hash: &str,
    verified: &str,
) -> ProfileResult<(), P::Error, std::convert::Infallible> {
    if !profiles.set_password_hash_if_current(account, hash, verified).await.map_err(ProfileError::Store)? {
        return Err(ProfileError::InvalidCredentials);
    }
    Ok(())
}

impl<P, A, T> ProfileApplication<'_, P, A, T>
where
    P: ProfileRepository,
    A: AccountRenameRepository<Error = P::Error>,
    T: CredentialThrottle,
{
    /// Renames the user's own account without changing its identity or credentials.
    /// The rename throttle is checked before the name is parsed or the password verified.
    pub fn change_username<'s>(
        &'s self,
        user: &'s User,
        current_password: String,
        new_name: &'s str,
    ) -> impl Future<Output = ProfileResult<AccountName, P::Error, T::Error>> + 's {
        let mut current_password = Zeroizing::new(current_password);
        async move {
            self.throttle.check_rename(user.id()).map_err(ProfileError::Throttle)?;
            let name: AccountName = new_name.trim().parse().map_err(ProfileError::Name)?;
            let verified = self.verify_password(user, std::mem::take(&mut *current_password)).await?;
            if !self.accounts.rename_if_password(user.id(), &name, &verified).await.map_err(ProfileError::Store)? {
                return Err(ProfileError::InvalidCredentials);
            }
            Ok(name)
        }
    }

    /// Changes the sign-in email after re-checking the current password.
    pub fn change_email<'s>(
        &'s self,
        user: &'s User,
        current_password: String,
        new_email: &'s str,
    ) -> impl Future<Output = ProfileResult<Email, P::Error, T::Error>> + 's {
        let mut current_password = Zeroizing::new(current_password);
        async move {
            let email: Email = new_email.trim().parse().map_err(ProfileError::Name)?;
            let verified = self.verify_password(user, std::mem::take(&mut *current_password)).await?;
            if !self.profiles.set_email_if_password(user.id(), &email, &verified).await.map_err(ProfileError::Store)? {
                return Err(ProfileError::InvalidCredentials);
            }
            Ok(email)
        }
    }

    /// Validates the new password, verifies the current one and hashes the replacement.
    /// `reserve` runs before the first await; its refusal is reported only after the
    /// current password was loaded and work admitted, preserving error precedence.
    pub fn prepare_password_change<'s, O: 's, R: 's>(
        &'s self,
        user: &'s User,
        current_password: String,
        new_password: String,
        reserve: impl FnOnce() -> Result<O, R> + 's,
    ) -> impl Future<Output = ProfileResult<PreparedPasswordChange<O>, P::Error, T::Error, R>> + 's {
        let mut current_password = Zeroizing::new(current_password);
        let mut new_password = Zeroizing::new(new_password);
        async move {
            self.validate_new_password::<R>(&new_password)?;
            if current_password.as_str() == new_password.as_str() {
                return Err(ProfileError::Rejected(ProfileRejection::SamePassword));
            }
            let ownership = reserve();
            let verified = self.checked_password_hash::<R>(user, &current_password).await?;
            let attempt = self.passwords.begin().map_err(ProfileError::Admission)?;
            let ownership = ownership.map_err(ProfileError::Reserve)?;
            let result = attempt
                .verify_and_hash(std::mem::take(&mut *current_password), verified.to_string(), std::mem::take(&mut *new_password))
                .await;
            if matches!(&result, Err(PasswordError::Hash(_))) {
                self.finish_password_check::<R>(user.id(), true)?;
            }
            let hash = result.map_err(ProfileError::Password)?;
            self.finish_password_check::<R>(user.id(), hash.is_some())?;
            let hash = hash.ok_or(ProfileError::InvalidCredentials)?;
            Ok(PreparedPasswordChange { ownership, hash, verified })
        }
    }

    fn validate_new_password<R>(&self, password: &str) -> ProfileResult<(), P::Error, T::Error, R> {
        let min = self.min_password_length;
        match classify_password_length(password, min, MAX_PASSWORD_LEN) {
            PasswordLength::TooShort => Err(ProfileError::Rejected(ProfileRejection::PasswordTooShort { min })),
            PasswordLength::TooLong => Err(ProfileError::Rejected(ProfileRejection::PasswordTooLong)),
            PasswordLength::Accepted => Ok(()),
        }
    }

    async fn checked_password_hash<R>(&self, user: &User, password: &str) -> ProfileResult<Zeroizing<String>, P::Error, T::Error, R> {
        self.throttle.check_password(user.id()).map_err(ProfileError::Throttle)?;
        if password.len() > MAX_PASSWORD_LEN * 4 {
            self.finish_password_check::<R>(user.id(), false)?;
        }
        self.profiles.password_hash(user.id()).await.map_err(ProfileError::Store)?.map(Zeroizing::new).ok_or(ProfileError::NotFound)
    }

    fn finish_password_check<R>(&self, account: AccountId, valid: bool) -> ProfileResult<(), P::Error, T::Error, R> {
        self.throttle.record_password(account, valid);
        if valid { Ok(()) } else { Err(ProfileError::InvalidCredentials) }
    }

    fn verify_password<'s>(
        &'s self,
        user: &'s User,
        password: String,
    ) -> impl Future<Output = ProfileResult<Zeroizing<String>, P::Error, T::Error>> + 's {
        let mut password = Zeroizing::new(password);
        async move {
            let hash = self.checked_password_hash::<std::convert::Infallible>(user, &password).await?;
            let attempt = self.passwords.begin().map_err(ProfileError::Admission)?;
            let valid = attempt.verify(std::mem::take(&mut *password), hash.to_string()).await.map_err(ProfileError::Password)?;
            self.finish_password_check::<std::convert::Infallible>(user.id(), valid)?;
            Ok(hash)
        }
    }
}
