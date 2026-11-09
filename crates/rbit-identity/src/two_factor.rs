//! Authenticator apps (TOTP) and recovery codes: enrolment, one-use code proofs and code
//! issuance.
//!
//! [`totp`] holds the RFC 6238 algorithm and the recovery-code format. [`TwoFactorApplication`]
//! decides enrolment (resume a pending secret, confirm with a matching code, issue recovery
//! codes only for the first factor) and checks a typed code: a recovery code is spent, an
//! authenticator code's time step is claimed so it works once. Callers keep throttling,
//! audit, passkeys and the pending sign-in.

mod rejection;
pub mod totp;

pub use rbit_identity_models::two_factor::{RecoveryDigest, SecondFactorClaims, TwoFactorRecord, TwoFactorRepository};
pub use rejection::TwoFactorRejection;

use crate::User;
use rbit_contracts::Timestamp;
use totp::{RecoveryCode, TotpSecret};

/// Recovery codes issued per enrolment (and per regeneration).
pub const RECOVERY_CODE_COUNT: usize = 10;

/// A pending enrolment: what the user puts into their authenticator app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrolment {
    /// Base32, for manual entry.
    pub secret: String,
    /// `otpauth://` URI, for the QR code.
    pub provisioning_uri: String,
}

/// Which kind of typed code was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeFactor {
    Authenticator,
    RecoveryCode,
}

#[derive(Debug)]
pub enum TwoFactorError<Store, Claim = std::convert::Infallible> {
    Store(Store),
    Claim(Claim),
    /// The rejection's text is user-facing.
    Rejected(TwoFactorRejection),
    /// A wrong, reused or malformed code.
    InvalidCredentials,
}

pub type TwoFactorResult<T, Store, Claim = std::convert::Infallible> = Result<T, TwoFactorError<Store, Claim>>;

/// Fresh recovery codes.
pub fn new_recovery_codes() -> Vec<RecoveryCode> {
    (0..RECOVERY_CODE_COUNT).map(|_| RecoveryCode::generate()).collect()
}

/// The stored digests of `codes`, bound to their owner.
pub fn recovery_digests(user: &User, codes: &[RecoveryCode]) -> Vec<RecoveryDigest> {
    codes.iter().map(|code| code.digest(user.id().0)).collect()
}

/// The codes as shown to the user, once.
pub fn formatted(codes: &[RecoveryCode]) -> Vec<String> {
    codes.iter().map(RecoveryCode::formatted).collect()
}

#[derive(Debug)]
pub struct TwoFactorApplication<'a, S> {
    store: &'a S,
}

impl<'a, S: TwoFactorRepository> TwoFactorApplication<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    /// Starts, or resumes so a reloaded page keeps an already scanned secret, an
    /// enrolment. `issuer` names the site in the authenticator app.
    pub async fn begin_enrolment(&self, user: &User, issuer: &str) -> TwoFactorResult<Enrolment, S::Error> {
        let already = || TwoFactorError::Rejected(TwoFactorRejection::AlreadySetUp);
        let secret = match self.store.find(user.id()).await.map_err(TwoFactorError::Store)? {
            Some(record) if record.is_enabled() => return Err(already()),
            Some(record) => TotpSecret::from_bytes(&record.secret),
            None => None,
        };
        let secret = if let Some(secret) = secret {
            secret
        } else {
            let secret = TotpSecret::generate();
            if !self.store.begin_enrolment(user.id(), secret.as_bytes()).await.map_err(TwoFactorError::Store)? {
                return Err(already());
            }
            secret
        };
        Ok(Enrolment { secret: secret.to_base32(), provisioning_uri: secret.provisioning_uri(issuer, user.name().as_str()) })
    }

    /// Discards a pending (unconfirmed) enrolment.
    pub async fn cancel_enrolment(&self, user: &User) -> TwoFactorResult<(), S::Error> {
        self.store.cancel_enrolment(user.id()).await.map_err(TwoFactorError::Store)
    }

    /// Enables a pending enrolment if `code` matches; returns new recovery codes if this
    /// is the account's first factor.
    pub async fn confirm(&self, user: &User, code: &str) -> TwoFactorResult<Option<Vec<RecoveryCode>>, S::Error> {
        let pending = self.store.find(user.id()).await.map_err(TwoFactorError::Store)?.filter(|record| !record.is_enabled());
        let Some(record) = pending else {
            return Err(TwoFactorError::Rejected(TwoFactorRejection::NoSetup));
        };
        let secret = TotpSecret::from_bytes(&record.secret).ok_or(TwoFactorError::Rejected(TwoFactorRejection::RestartSetup))?;
        let step = secret.verify(code, Timestamp::now().seconds()).ok_or(TwoFactorError::InvalidCredentials)?;
        let codes = new_recovery_codes();
        let confirmed =
            self.store.confirm(user.id(), &record.secret, step, &recovery_digests(user, &codes)).await.map_err(TwoFactorError::Store)?;
        let Some(first_factor) = confirmed else {
            return Err(TwoFactorError::Rejected(TwoFactorRejection::SetupChanged));
        };
        Ok(first_factor.then_some(codes))
    }

    /// Checks a typed code for an enrolled account: an unused recovery code (spent) or a
    /// 6-digit authenticator code whose time step has not been accepted before.
    pub async fn verify_code<C: SecondFactorClaims>(
        &self,
        user: &User,
        code: &str,
        claims: &C,
    ) -> TwoFactorResult<CodeFactor, S::Error, C::Error> {
        if let Some(recovery) = RecoveryCode::parse(code) {
            let spent = claims.spend_recovery_code(user.id(), &recovery.digest(user.id().0)).await.map_err(TwoFactorError::Claim)?;
            return if spent { Ok(CodeFactor::RecoveryCode) } else { Err(TwoFactorError::InvalidCredentials) };
        }
        let enabled = self.store.find(user.id()).await.map_err(TwoFactorError::Store)?.filter(TwoFactorRecord::is_enabled);
        let Some(record) = enabled else {
            return Err(TwoFactorError::InvalidCredentials);
        };
        let secret = TotpSecret::from_bytes(&record.secret).ok_or(TwoFactorError::InvalidCredentials)?;
        match secret.verify(code, Timestamp::now().seconds()) {
            Some(step) if claims.claim_step(user.id(), &record.secret, step).await.map_err(TwoFactorError::Claim)? => {
                Ok(CodeFactor::Authenticator)
            }
            _ => Err(TwoFactorError::InvalidCredentials),
        }
    }

    /// Replaces the recovery codes, invalidating the old ones. Callers check the password
    /// and a proof first.
    pub async fn regenerate_recovery_codes(&self, user: &User) -> TwoFactorResult<Vec<RecoveryCode>, S::Error> {
        let codes = new_recovery_codes();
        if !self.store.replace_recovery_codes(user.id(), &recovery_digests(user, &codes)).await.map_err(TwoFactorError::Store)? {
            return Err(TwoFactorError::Rejected(TwoFactorRejection::NotEnabled));
        }
        Ok(codes)
    }
}
