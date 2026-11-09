//! Passkeys and security keys as a second factor: `WebAuthn` ceremonies, registration
//! policy and one-use assertions.
//!
//! [`webauthn`] builds browser options and verifies responses for a [`RelyingParty`] bound
//! to the site's public URL (origin checks included). [`PasskeyApplication`] decides
//! registration (name, limit, size, single-use challenge, verification, duplicates, first
//! factor recovery codes) and checks an assertion against the stored key, advancing its
//! counter through [`PasskeyClaims`] so a response works once. Callers keep audit, logs,
//! the pending sign-in and password-plus-proof checks.

mod challenges;
mod rejection;
pub mod webauthn;

pub use challenges::{Ceremony, ChallengeBook};
pub use rbit_identity_models::passkeys::{NewPasskey, PasskeyAdded, PasskeyClaims, PasskeyRecord, PasskeyRepository};
pub use rejection::PasskeyRejection;

use crate::User;
use crate::two_factor::{formatted, new_recovery_codes, recovery_digests};
use webauthn::{Challenge, RelyingParty, WebauthnError};

/// Passkeys per account.
pub const MAX_PASSKEYS: usize = 20;
/// Longest passkey name, in characters.
pub const MAX_PASSKEY_NAME: usize = 64;
/// Size limit of a browser's `WebAuthn` answer (they are a few hundred bytes to a few KiB).
pub const MAX_RESPONSE_LEN: usize = 16 * 1024;

/// The result of registering a passkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasskeyRegistered {
    /// Added next to the account's other factors.
    Added,
    /// The account's first factor: these fresh recovery codes must be shown once.
    FirstFactor(Vec<String>),
}

#[derive(Debug)]
pub enum PasskeyError<Store, Claim = std::convert::Infallible> {
    Store(Store),
    Claim(Claim),
    /// The rejection's text is user-facing.
    Rejected(PasskeyRejection),
    /// A registration response failed verification; the reason is for logs only and the
    /// user sees [`PasskeyRejection::NotVerified`].
    RegistrationRefused(WebauthnError),
    /// An assertion failed verification; the reason is for logs only.
    AssertionRefused(WebauthnError),
    /// Unknown passkey, oversized or already used response, or a missing challenge.
    InvalidCredentials,
}

pub type PasskeyResult<T, Store, Claim = std::convert::Infallible> = Result<T, PasskeyError<Store, Claim>>;

/// A trimmed passkey name, `Passkey` when left empty.
pub fn passkey_name(name: &str) -> Result<String, PasskeyRejection> {
    let name = name.trim();
    if name.chars().count() > MAX_PASSKEY_NAME {
        return Err(PasskeyRejection::NameTooLong);
    }
    if name.chars().any(char::is_control) {
        return Err(PasskeyRejection::NameControlCharacters);
    }
    Ok(if name.is_empty() { "Passkey".to_owned() } else { name.to_owned() })
}

#[derive(Debug)]
pub struct PasskeyApplication<'a, S> {
    store: &'a S,
    challenges: &'a ChallengeBook,
    relying_party: Option<RelyingParty>,
}

impl<'a, S: PasskeyRepository> PasskeyApplication<'a, S> {
    /// `relying_party` is `None` when the site has no usable public URL; passkeys are bound
    /// to it, so they only work when the site is opened at that address.
    pub fn new(store: &'a S, challenges: &'a ChallengeBook, relying_party: Option<RelyingParty>) -> Self {
        Self { store, challenges, relying_party }
    }

    fn relying_party<C>(&self) -> PasskeyResult<&RelyingParty, S::Error, C> {
        self.relying_party.as_ref().ok_or(PasskeyError::Rejected(PasskeyRejection::NoPublicUrl))
    }

    /// Options for `navigator.credentials.get` limited to `passkeys`, or `None` if empty.
    pub fn request_options(&self, passkeys: &[PasskeyRecord], challenge: &Challenge) -> PasskeyResult<Option<String>, S::Error> {
        if passkeys.is_empty() {
            return Ok(None);
        }
        let ids: Vec<Vec<u8>> = passkeys.iter().map(|passkey| passkey.credential_id.clone()).collect();
        Ok(Some(self.relying_party()?.request_options(challenge, &ids)))
    }

    /// Options for `navigator.credentials.create` to register a new passkey.
    pub async fn creation_options(&self, user: &User) -> PasskeyResult<String, S::Error> {
        let rp = self.relying_party()?;
        let existing = self.store.passkeys(user.id()).await.map_err(PasskeyError::Store)?;
        let existing: Vec<Vec<u8>> = existing.into_iter().map(|passkey| passkey.credential_id).collect();
        let challenge = self.challenges.issue(user.id(), Ceremony::Register);
        Ok(rp.creation_options(&challenge, &user.id().0.to_be_bytes(), user.name().as_str(), &existing))
    }

    /// Options to confirm a sensitive change with a passkey, or `None` if the account has none.
    pub async fn confirmation_options(&self, user: &User) -> PasskeyResult<Option<String>, S::Error> {
        let passkeys = self.store.passkeys(user.id()).await.map_err(PasskeyError::Store)?;
        if passkeys.is_empty() {
            return Ok(None);
        }
        let challenge = self.challenges.issue(user.id(), Ceremony::Confirm);
        self.request_options(&passkeys, &challenge)
    }

    /// Registers a passkey from the browser's answer to [`Self::creation_options`]. Returns
    /// the stored name (for the audit) and whether it was the account's first factor.
    pub async fn register(&self, user: &User, name: &str, response: &str) -> PasskeyResult<(String, PasskeyRegistered), S::Error> {
        let name = passkey_name(name).map_err(PasskeyError::Rejected)?;
        let challenge = self
            .challenges
            .take(user.id(), Ceremony::Register, response)
            .ok_or(PasskeyError::Rejected(PasskeyRejection::RequestExpired))?;
        if self.store.passkeys(user.id()).await.map_err(PasskeyError::Store)?.len() >= MAX_PASSKEYS {
            return Err(PasskeyError::Rejected(PasskeyRejection::TooMany));
        }
        if response.len() > MAX_RESPONSE_LEN {
            return Err(PasskeyError::Rejected(PasskeyRejection::ResponseTooLarge));
        }
        let credential = self.relying_party()?.verify_registration(&challenge, response).map_err(PasskeyError::RegistrationRefused)?;
        let codes = new_recovery_codes();
        let passkey = NewPasskey {
            credential_id: &credential.id,
            public_key: &credential.public_key,
            algorithm: credential.algorithm,
            sign_count: credential.sign_count,
            transports: &credential.transports,
            name: &name,
        };
        let registered =
            match self.store.add_passkey(user.id(), &passkey, &recovery_digests(user, &codes)).await.map_err(PasskeyError::Store)? {
                PasskeyAdded::Duplicate => return Err(PasskeyError::Rejected(PasskeyRejection::AlreadyRegistered)),
                PasskeyAdded::Added => PasskeyRegistered::Added,
                PasskeyAdded::FirstFactor => PasskeyRegistered::FirstFactor(formatted(&codes)),
            };
        Ok((name, registered))
    }

    /// Checks a passkey's answer to `challenge` for `user` and advances its counter once.
    pub async fn verify_assertion<C: PasskeyClaims>(
        &self,
        user: &User,
        challenge: &Challenge,
        response: &str,
        claims: &C,
    ) -> PasskeyResult<(), S::Error, C::Error> {
        if response.len() > MAX_RESPONSE_LEN {
            return Err(PasskeyError::InvalidCredentials);
        }
        let rp = self.relying_party()?;
        let credential_id = RelyingParty::assertion_credential_id(response).map_err(PasskeyError::AssertionRefused)?;
        let found = self.store.find_passkey(user.id(), &credential_id).await.map_err(PasskeyError::Store)?;
        let Some(passkey) = found else {
            return Err(PasskeyError::InvalidCredentials);
        };
        let count =
            rp.verify_assertion(challenge, response, &passkey.public_key, passkey.sign_count).map_err(PasskeyError::AssertionRefused)?;
        if claims.use_passkey(user.id(), passkey.id, passkey.sign_count, count).await.map_err(PasskeyError::Claim)? {
            Ok(())
        } else {
            Err(PasskeyError::InvalidCredentials)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passkey_names_are_trimmed_and_bounded() {
        assert_eq!(passkey_name("  YubiKey 5 ").unwrap(), "YubiKey 5");
        assert_eq!(passkey_name("").unwrap(), "Passkey");
        assert!(passkey_name(&"x".repeat(MAX_PASSKEY_NAME + 1)).is_err());
        assert!(passkey_name("a\u{0}b").is_err());
    }

    #[test]
    fn rejection_texts_match_the_legacy_messages() {
        assert_eq!(PasskeyRejection::NameTooLong.to_string(), "the name can be at most 64 characters");
        assert_eq!(PasskeyRejection::TooMany.to_string(), "you can register at most 20 passkeys");
        assert_eq!(PasskeyRejection::NoPublicUrl.to_string(), "passkeys need `server.public_url` to be set to this site's address");
    }
}
