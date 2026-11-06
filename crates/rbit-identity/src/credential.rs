//! Password hashing with Argon2id.
//!
//! Hashing is deliberately expensive (~19 MiB, 2 passes), so both operations run on
//! Tokio's blocking pool to keep request handling latency unaffected.

use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("password hashing failed: {0}")]
    Hash(argon2::password_hash::Error),
    #[error("password hashing task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// A well-formed Argon2id hash of a random password; see [`Passwords::verify_dummy`].
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Zf3xIZKjFtw3Ua2Hn1z0vNfS0d1oR0Q2O9iZb6v3m1o";

/// Stateless facade over the configured Argon2id instance.
#[derive(Debug, Clone, Copy, Default)]
pub struct Passwords;

impl Passwords {
    /// Argon2id v1.3 with the OWASP-recommended defaults (m = 19 MiB, t = 2, p = 1).
    fn argon2() -> Argon2<'static> {
        Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
    }

    /// Returns a PHC string (`$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`) with a fresh random salt.
    pub async fn hash(password: String) -> Result<String, PasswordError> {
        tokio::task::spawn_blocking(move || {
            Self::argon2().hash_password(password.as_bytes()).map(|hash| hash.to_string()).map_err(PasswordError::Hash)
        })
        .await?
    }

    /// Constant-time verification. Malformed stored hashes simply fail to verify.
    pub async fn verify(password: String, phc_hash: String) -> Result<bool, PasswordError> {
        Ok(tokio::task::spawn_blocking(move || Self::argon2().verify_password(password.as_bytes(), phc_hash.as_str()).is_ok()).await?)
    }

    /// A valid hash of a random password, verified against when a login name does not exist
    /// so that response timing does not reveal which usernames are registered.
    pub async fn verify_dummy(password: String) {
        let _ = Self::verify(password, DUMMY_HASH.to_owned()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_hash_is_well_formed() {
        assert!(argon2::PasswordHash::new(DUMMY_HASH).is_ok());
    }

    #[tokio::test]
    async fn hash_then_verify() {
        let hash = Passwords::hash("correct horse battery staple".into()).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(Passwords::verify("correct horse battery staple".into(), hash.clone()).await.unwrap());
        assert!(!Passwords::verify("wrong".into(), hash).await.unwrap());
    }
}
