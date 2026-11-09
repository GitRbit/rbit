//! Personal access token compatibility path; the format and its constructors are
//! identity-owned (`rbit_identity::token::AccessTokenSecret`).

pub use rbit_identity::token::AccessTokenSecret;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_path_is_the_identity_secret_type() {
        use std::any::TypeId;
        assert_eq!(TypeId::of::<AccessTokenSecret>(), TypeId::of::<rbit_identity::token::AccessTokenSecret>());
        assert_eq!(TypeId::of::<crate::AccessTokenSecret>(), TypeId::of::<rbit_identity::AccessTokenSecret>());
    }

    #[test]
    fn round_trips_with_prefix() {
        let secret = AccessTokenSecret::generate();
        let encoded = secret.encode();
        assert!(encoded.starts_with("rbit_"));
        assert_eq!(encoded.len(), 5 + 43);
        assert_eq!(AccessTokenSecret::parse(&encoded).unwrap().digest(), secret.digest());
        assert!(encoded.ends_with(&secret.hint()));
    }

    #[test]
    fn legacy_prefix_parses_to_the_same_secret() {
        let secret = AccessTokenSecret::generate();
        let legacy = secret.encode().replacen("rbit_", "rit_", 1);
        assert!(AccessTokenSecret::looks_like_token(&legacy));
        let parsed = AccessTokenSecret::parse(&legacy).unwrap();
        assert_eq!(parsed.digest(), secret.digest());
        assert_eq!(parsed.hint(), secret.hint());
        assert!(legacy.ends_with(&parsed.hint()));
    }

    #[test]
    fn rejects_passwords_and_session_tokens() {
        assert!(AccessTokenSecret::parse("hunter2").is_none());
        assert!(!AccessTokenSecret::looks_like_token("hunter2"));
        assert!(AccessTokenSecret::parse(&crate::SessionToken::generate().encode()).is_none());
        assert!(AccessTokenSecret::parse("rbit_tooshort").is_none());
        assert!(AccessTokenSecret::parse("rit_tooshort").is_none());
        // One prefix only: `rbit_rit_…` is not a token.
        let doubled = format!("rbit_rit_{}", &AccessTokenSecret::generate().encode()[5..]);
        assert!(AccessTokenSecret::parse(&doubled).is_none());
    }
}
