//! Sensitive factor changes, each confirmed with the password and a proof first.

use std::future::Future;

use super::step::SecondFactorStep;
use super::{SecondFactorProof, SecondFactorThrottle, SecondStepAccounts, SecondStepError, SecondStepResult};
use crate::User;
use crate::passkeys::{PasskeyClaims, PasskeyRepository};
use crate::two_factor::{SecondFactorClaims, TwoFactorRepository};

type ClaimError<C> = <C as SecondFactorClaims>::Error;

impl<S, A, T> SecondFactorStep<'_, S, A, T>
where
    S: TwoFactorRepository + PasskeyRepository<Error = <S as TwoFactorRepository>::Error>,
    A: SecondStepAccounts<Error = <S as TwoFactorRepository>::Error>,
    T: SecondFactorThrottle,
{
    /// Removes passkey `id` after the confirmation and returns its name for the audit.
    /// An unknown passkey is [`SecondStepError::NotFound`] before anything is checked,
    /// including before the proof is converted.
    pub fn remove_passkey<'s, 'p: 's, C>(
        &'s self,
        user: &'s User,
        id: i64,
        password: String,
        proof: impl Into<SecondFactorProof<'p>> + 's,
        claims: &'s C,
    ) -> impl Future<Output = SecondStepResult<String, <S as TwoFactorRepository>::Error, T::Error, ClaimError<C>>> + 's
    where
        C: SecondFactorClaims + PasskeyClaims<Error = ClaimError<C>>,
    {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let passkeys = PasskeyRepository::passkeys(self.store, user.id()).await.map_err(SecondStepError::Store)?;
            let Some(passkey) = passkeys.into_iter().find(|passkey| passkey.id == id) else {
                return Err(SecondStepError::NotFound);
            };
            self.confirm(user, std::mem::take(&mut *password), proof.into(), claims).await?;
            PasskeyRepository::remove_passkey(self.store, user.id(), id).await.map_err(SecondStepError::Store)?;
            Ok(passkey.name)
        }
    }

    /// Removes the authenticator app after the confirmation. Two-factor authentication
    /// stays on while the account has passkeys.
    pub fn disable_authenticator<'s, C>(
        &'s self,
        user: &'s User,
        password: String,
        proof: SecondFactorProof<'s>,
        claims: &'s C,
    ) -> impl Future<Output = SecondStepResult<(), <S as TwoFactorRepository>::Error, T::Error, ClaimError<C>>> + 's
    where
        C: SecondFactorClaims + PasskeyClaims<Error = ClaimError<C>>,
    {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            self.confirm(user, std::mem::take(&mut *password), proof, claims).await?;
            TwoFactorRepository::disable(self.store, user.id()).await.map_err(SecondStepError::Store)?;
            Ok(())
        }
    }
}
