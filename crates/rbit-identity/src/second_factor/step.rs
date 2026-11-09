//! Finishing a pending sign-in, and confirming sensitive changes, with one proof.

use std::future::Future;

use zeroize::Zeroizing;

use super::{
    CompletedSecondFactor, CompletedSignIn, PendingSignIns, SecondFactor, SecondFactorProof, SecondFactorThrottle, SecondStepAccounts,
    SecondStepError, SecondStepResult,
};
use crate::accounts::OriginalProofReceipt;
use crate::passkeys::{
    Ceremony, ChallengeBook, PasskeyApplication, PasskeyClaims, PasskeyError, PasskeyRejection, PasskeyRepository, webauthn,
};
use crate::two_factor::{CodeFactor, SecondFactorClaims, TwoFactorApplication, TwoFactorError, TwoFactorRepository};
use crate::{PasswordWork, SessionToken, User};
use webauthn::{Challenge, RelyingParty, WebauthnError};

/// Why a proof was not accepted, before the step decides what that means.
enum Refusal<S, C> {
    Store(S),
    Claim(C),
    Rejected(PasskeyRejection),
    Wrong(Option<WebauthnError>),
    Invalidated,
}

type ClaimError<C> = <C as SecondFactorClaims>::Error;

#[derive(Debug)]
pub struct SecondFactorStep<'a, S, A, T> {
    pending: &'a PendingSignIns,
    pub(super) store: &'a S,
    challenges: &'a ChallengeBook,
    relying_party: Option<RelyingParty>,
    accounts: &'a A,
    throttle: &'a T,
    passwords: &'a PasswordWork,
}

impl<'a, S, A, T> SecondFactorStep<'a, S, A, T>
where
    S: TwoFactorRepository + PasskeyRepository<Error = <S as TwoFactorRepository>::Error>,
    A: SecondStepAccounts<Error = <S as TwoFactorRepository>::Error>,
    T: SecondFactorThrottle,
{
    pub fn new(
        pending: &'a PendingSignIns,
        store: &'a S,
        challenges: &'a ChallengeBook,
        relying_party: Option<RelyingParty>,
        accounts: &'a A,
        throttle: &'a T,
        passwords: &'a PasswordWork,
    ) -> Self {
        Self { pending, store, challenges, relying_party, accounts, throttle, passwords }
    }

    /// Checks a second factor for an enrolled account: a 6-digit code (each time step
    /// works once), an unused recovery code (spent) or a passkey's answer to `challenge`.
    async fn verify<C>(
        &self,
        user: &User,
        proof: SecondFactorProof<'_>,
        challenge: Option<Challenge>,
        claims: &C,
    ) -> Result<SecondFactor, Refusal<<S as TwoFactorRepository>::Error, ClaimError<C>>>
    where
        C: SecondFactorClaims + PasskeyClaims<Error = ClaimError<C>>,
    {
        match proof {
            SecondFactorProof::Code(code) => match TwoFactorApplication::new(self.store).verify_code(user, code, claims).await {
                Ok(CodeFactor::Authenticator) => Ok(SecondFactor::Authenticator),
                Ok(CodeFactor::RecoveryCode) => Ok(SecondFactor::RecoveryCode),
                Err(TwoFactorError::Store(error)) => Err(Refusal::Store(error)),
                Err(TwoFactorError::Claim(error)) => Err(Refusal::Claim(error)),
                Err(TwoFactorError::Invalidated) => Err(Refusal::Invalidated),
                Err(TwoFactorError::InvalidCredentials | TwoFactorError::Rejected(_)) => Err(Refusal::Wrong(None)),
            },
            SecondFactorProof::Passkey(response) => {
                let challenge = challenge.ok_or(Refusal::Wrong(None))?;
                let passkeys = PasskeyApplication::new(self.store, self.challenges, self.relying_party.clone());
                match passkeys.verify_assertion(user, &challenge, response, claims).await {
                    Ok(()) => Ok(SecondFactor::Passkey),
                    Err(PasskeyError::Store(error)) => Err(Refusal::Store(error)),
                    Err(PasskeyError::Claim(error)) => Err(Refusal::Claim(error)),
                    Err(PasskeyError::Rejected(rejection)) => Err(Refusal::Rejected(rejection)),
                    Err(PasskeyError::AssertionRefused(error)) => Err(Refusal::Wrong(Some(error))),
                    Err(PasskeyError::Invalidated) => Err(Refusal::Invalidated),
                    Err(PasskeyError::InvalidCredentials | PasskeyError::RegistrationRefused(_)) => Err(Refusal::Wrong(None)),
                }
            }
        }
    }

    /// Finishes a pending sign-in with one proof. `original_only` selects the receipted
    /// (inactive) or the legacy entry kind; neither completes the other. `claims_for`
    /// supplies the claim writes for the entry's mode. `after_lookup` runs right after the
    /// authoritative account lookup (production passes a ready future; tests park there).
    pub async fn complete<C>(
        &self,
        token: &SessionToken,
        proof: SecondFactorProof<'_>,
        original_only: bool,
        claims_for: impl FnOnce(Option<&OriginalProofReceipt>) -> C,
        after_lookup: impl Future<Output = ()> + Send,
    ) -> SecondStepResult<CompletedSecondFactor, <S as TwoFactorRepository>::Error, T::Error, ClaimError<C>>
    where
        C: SecondFactorClaims + PasskeyClaims<Error = ClaimError<C>>,
    {
        let (account, original) = self.pending.proof(token).ok_or(SecondStepError::NotFound)?;
        if original.is_some() != original_only {
            return Err(SecondStepError::NotFound);
        }
        self.throttle.check(account).map_err(SecondStepError::Throttle)?;
        let user = match &original {
            Some(receipt) => self.accounts.find_user_at_epoch(account, receipt.original_epoch()).await,
            None => self.accounts.find_active_user(account).await,
        };
        let Some(user) = user.map_err(SecondStepError::Store)? else {
            self.pending.end(token);
            return Err(SecondStepError::NotFound);
        };
        after_lookup.await;
        let challenge = matches!(proof, SecondFactorProof::Passkey(_)).then(|| self.pending.take_challenge(token)).flatten();
        let claims = claims_for(original.as_ref());
        let verified = match &original {
            Some(receipt) if receipt.account() != user.id() => Err(Refusal::Invalidated),
            _ => self.verify(&user, proof, challenge, &claims).await,
        };
        match verified {
            Ok(factor) => {
                let claimed = match &original {
                    Some(receipt) => self.pending.finish_original(token, receipt),
                    None => self.pending.finish(token),
                };
                let method = claimed.ok_or(SecondStepError::NotFound)?;
                let sign_in = match original {
                    Some(receipt) => CompletedSignIn::Original(receipt.bind_user(user).map_err(|_| SecondStepError::NotFound)?),
                    None => CompletedSignIn::Legacy(user),
                };
                self.throttle.reset(account);
                Ok(CompletedSecondFactor { sign_in, factor, method })
            }
            Err(Refusal::Wrong(refused)) => {
                self.throttle.record_failure(account);
                self.pending.failed(token);
                Err(SecondStepError::WrongFactor { user: Box::new(user), refused })
            }
            Err(Refusal::Invalidated) => {
                if original.is_some() {
                    self.pending.end(token);
                }
                Err(SecondStepError::NotFound)
            }
            Err(Refusal::Store(error)) => Err(SecondStepError::Store(error)),
            Err(Refusal::Claim(error)) => Err(SecondStepError::Claim(error)),
            Err(Refusal::Rejected(rejection)) => Err(SecondStepError::Rejected(rejection)),
        }
    }

    /// Re-checks the password and a proof before a sensitive change. Admission precedes
    /// proof consumption, and a passkey challenge is taken before anything is checked so
    /// a failed attempt can't be retried with it.
    pub fn confirm<'s, C>(
        &'s self,
        user: &'s User,
        password: String,
        proof: SecondFactorProof<'s>,
        claims: &'s C,
    ) -> impl Future<Output = SecondStepResult<(), <S as TwoFactorRepository>::Error, T::Error, ClaimError<C>>> + 's
    where
        C: SecondFactorClaims + PasskeyClaims<Error = ClaimError<C>>,
    {
        let mut password = Zeroizing::new(password);
        async move {
            self.throttle.check(user.id()).map_err(SecondStepError::Throttle)?;
            let attempt = self.passwords.begin().map_err(SecondStepError::Admission)?;
            let challenge = match proof {
                SecondFactorProof::Passkey(response) => self.challenges.take(user.id(), Ceremony::Confirm, response),
                SecondFactorProof::Code(_) => None,
            };
            let hash = self.accounts.password_hash(user.id()).await.map_err(SecondStepError::Store)?;
            let password_ok = match hash {
                Some(hash) => attempt.verify(std::mem::take(&mut *password), hash).await.map_err(SecondStepError::Password)?,
                None => false,
            };
            let verified =
                if password_ok { self.verify(user, proof, challenge, claims).await.map(drop) } else { Err(Refusal::Wrong(None)) };
            match verified {
                Ok(()) => {
                    self.throttle.reset(user.id());
                    Ok(())
                }
                Err(Refusal::Wrong(refused)) => {
                    self.throttle.record_failure(user.id());
                    Err(SecondStepError::WrongFactor { user: Box::new(user.clone()), refused })
                }
                Err(Refusal::Invalidated) => Err(SecondStepError::NotFound),
                Err(Refusal::Store(error)) => Err(SecondStepError::Store(error)),
                Err(Refusal::Claim(error)) => Err(SecondStepError::Claim(error)),
                Err(Refusal::Rejected(rejection)) => Err(SecondStepError::Rejected(rejection)),
            }
        }
    }
}
