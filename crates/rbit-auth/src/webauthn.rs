//! Passkey ceremony compatibility path; option building, response verification and their
//! tests are identity-owned (`rbit_identity::passkeys::webauthn`).

pub use rbit_identity::passkeys::webauthn::{
    CEREMONY_TIMEOUT_MS, Challenge, MAX_CREDENTIAL_ID_LEN, NewCredential, RelyingParty, WebauthnError,
};

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_paths_are_the_identity_types() {
        use std::any::TypeId;
        assert_eq!(TypeId::of::<super::RelyingParty>(), TypeId::of::<rbit_identity::passkeys::webauthn::RelyingParty>());
        assert_eq!(TypeId::of::<crate::Challenge>(), TypeId::of::<rbit_identity::passkeys::webauthn::Challenge>());
    }
}
