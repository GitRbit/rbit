//! Personal access tokens: long-lived credentials for Git over HTTPS and the REST API.

/// Token identity and lifecycle records are identity-owned; this path stays compatible.
pub use rbit_identity_models::access_tokens::AccessTokenId;

/// What a token may do; an authorization decision owned by `rbit-access`.
pub use rbit_access::TokenScope;

/// Token metadata. The secret itself is shown once at creation and never stored.
pub type AccessToken = rbit_identity_models::access_tokens::AccessTokenRecord<TokenScope>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_token_paths_are_the_identity_records() {
        use rbit_identity_models::access_tokens::{AccessTokenId as Id, AccessTokenRecord};
        use std::any::TypeId;
        assert_eq!(TypeId::of::<AccessTokenId>(), TypeId::of::<Id>());
        assert_eq!(TypeId::of::<AccessToken>(), TypeId::of::<AccessTokenRecord<TokenScope>>());
        assert_eq!(TypeId::of::<TokenScope>(), TypeId::of::<rbit_access::TokenScope>());
    }
}
