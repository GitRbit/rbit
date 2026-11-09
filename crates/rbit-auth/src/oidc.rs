//! External sign-in compatibility path; flow sealing, PKCE, ID-token validation and their
//! tests are identity-owned (`rbit_identity::oidc`).

pub use rbit_identity::oidc::{
    CLOCK_LEEWAY_SECONDS, Flow, FlowKey, IdToken, IdTokenCheck, JwkSet, OidcError, Pkce, token_key_id, verify_id_token,
};

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_paths_are_the_identity_types() {
        use std::any::TypeId;
        assert_eq!(TypeId::of::<super::FlowKey>(), TypeId::of::<rbit_identity::oidc::FlowKey>());
        assert_eq!(TypeId::of::<crate::JwkSet>(), TypeId::of::<rbit_identity::oidc::JwkSet>());
    }
}
