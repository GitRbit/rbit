//! Session token compatibility path; all constructors and wire behavior are identity-owned.

pub use rbit_identity::SessionToken;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_parse_round_trip() {
        let token = SessionToken::generate();
        let parsed = SessionToken::parse(&token.encode()).expect("round trip");
        assert_eq!(parsed.digest(), token.digest());
    }

    #[test]
    fn legacy_paths_are_the_same_identity_token_and_digest_types() {
        use std::any::TypeId;
        assert_eq!(TypeId::of::<SessionToken>(), TypeId::of::<rbit_identity::SessionToken>());
        assert_eq!(TypeId::of::<crate::SessionToken>(), TypeId::of::<rbit_identity::SessionToken>());
        assert_eq!(TypeId::of::<crate::TokenDigest>(), TypeId::of::<rbit_identity::TokenDigest>());
        assert_eq!(TypeId::of::<crate::TokenDigest>(), TypeId::of::<[u8; 32]>());
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(SessionToken::parse("short").is_none());
        assert!(SessionToken::parse("").is_none());
    }
}
