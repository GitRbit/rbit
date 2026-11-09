//! Password hashing with Argon2id.
//!
//! Hashing is deliberately expensive (~19 MiB, 2 passes). These synchronous primitives
//! run only inside workers admitted by `PasswordWork`; the unadmitted async facade that
//! scheduled them directly on Tokio's blocking pool is retired.

#[cfg(feature = "password-work")]
use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};

#[cfg(feature = "password-work")]
mod login;
#[cfg(feature = "password-work")]
pub(crate) use login::verify_login;

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("password hashing failed: {0}")]
    Hash(argon2::password_hash::Error),
    #[error("password hashing task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// A well-formed Argon2id hash of a random password, verified against when a login name
/// does not exist so that response timing does not reveal which usernames are registered.
#[cfg(feature = "password-work")]
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Zf3xIZKjFtw3Ua2Hn1z0vNfS0d1oR0Q2O9iZb6v3m1o";

/// Argon2id v1.3 with the OWASP-recommended defaults (m = 19 MiB, t = 2, p = 1).
#[cfg(feature = "password-work")]
fn hasher() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
}

/// Returns a PHC string (`$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`) with a fresh random salt.
#[cfg(feature = "password-work")]
pub(crate) fn hash(password: &str) -> Result<String, PasswordError> {
    hasher().hash_password(password.as_bytes()).map(|hash| hash.to_string()).map_err(PasswordError::Hash)
}

/// Constant-time verification. Malformed stored hashes simply fail to verify.
#[cfg(feature = "password-work")]
pub(crate) fn verify(password: &str, phc_hash: &str) -> bool {
    hasher().verify_password(password.as_bytes(), phc_hash).is_ok()
}

#[cfg(feature = "password-work")]
pub(crate) fn verify_dummy(password: &str) {
    let _ = verify(password, DUMMY_HASH);
}

#[cfg(all(test, feature = "password-work"))]
mod tests {
    use super::*;

    #[test]
    fn dummy_hash_is_well_formed_and_uses_the_configured_parameters() {
        assert!(argon2::PasswordHash::new(DUMMY_HASH).is_ok());
        let params = hasher().params().clone();
        let expected = format!("$argon2id$v=19$m={},t={},p={}$", params.m_cost(), params.t_cost(), params.p_cost());
        assert!(DUMMY_HASH.starts_with(&expected));
    }

    #[test]
    fn hash_then_verify() {
        let hash = hash("correct horse battery staple").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify("correct horse battery staple", &hash));
        assert!(!verify("wrong", &hash));
        assert!(!verify("anything", "malformed"));
    }
}
