//! Login-only covering work for absent, malformed or unsupported stored credentials.

use argon2::{PasswordHash, PasswordVerifier, password_hash::Error};

use super::{DUMMY_HASH, PasswordError, hasher};

pub(crate) fn verify_login(password: &str, stored: Option<&str>) -> Result<bool, PasswordError> {
    if let Some(stored) = stored
        && let Some(accepted) = verify_stored(password, stored)?
    {
        return Ok(accepted);
    }
    // Even a hypothetical successful match of the dummy credential grants no identity.
    match hasher().verify_password(password.as_bytes(), DUMMY_HASH) {
        Ok(()) | Err(Error::PasswordInvalid) => Ok(false),
        Err(error) => Err(PasswordError::Hash(error)),
    }
}

fn verify_stored(password: &str, stored: &str) -> Result<Option<bool>, PasswordError> {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return Ok(None);
    };
    // PHC parsing permits optional salt/output. The generic verifier reports these
    // as PasswordInvalid without running Argon2; they still need covering work.
    if parsed.salt.is_none() || parsed.hash.is_none() {
        return Ok(None);
    }
    classify(hasher().verify_password(password.as_bytes(), &parsed))
}

/// None means no supported verification ran and dummy work is still required.
fn classify(result: Result<(), Error>) -> Result<Option<bool>, PasswordError> {
    match result {
        Ok(()) => Ok(Some(true)),
        Err(Error::PasswordInvalid) => Ok(Some(false)),
        Err(
            Error::Algorithm
            | Error::EncodingInvalid
            | Error::OutputSize
            | Error::ParamInvalid { .. }
            | Error::ParamsInvalid
            | Error::SaltInvalid
            | Error::Version,
        ) => Ok(None),
        Err(error) => Err(PasswordError::Hash(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_verification_results_never_request_a_second_dummy_hash() {
        assert_eq!(classify(Ok(())).unwrap(), Some(true));
        assert_eq!(classify(Err(Error::PasswordInvalid)).unwrap(), Some(false));
    }

    #[test]
    fn malformed_or_unsupported_credentials_request_covering_work() {
        for error in [
            Error::Algorithm,
            Error::EncodingInvalid,
            Error::OutputSize,
            Error::ParamInvalid { name: "m" },
            Error::ParamsInvalid,
            Error::SaltInvalid,
            Error::Version,
        ] {
            assert_eq!(classify(Err(error)).unwrap(), None);
        }
    }

    #[test]
    fn genuine_crypto_failures_cannot_become_bad_password_results() {
        for error in [Error::Crypto, Error::Internal, Error::OutOfMemory, Error::RngFailure] {
            assert!(matches!(classify(Err(error)), Err(PasswordError::Hash(returned)) if returned == error));
        }
    }

    #[test]
    fn incomplete_phc_fields_cannot_masquerade_as_an_expensive_password_mismatch() {
        for stored in ["", "malformed", "$argon2id", "$argon2id$v=19", "$argon2id$v=19$m=19456,t=2,p=1"] {
            assert_eq!(verify_stored("password", stored).unwrap(), None);
        }
    }
}
