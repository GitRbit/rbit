//! Session-token compatibility independent of password algorithms and admission.

use rbit_identity::{SessionToken, TokenDigest, token::codec};
use std::fmt::Write as _;

#[test]
fn fixed_session_token_encoding_digest_and_debug_remain_exact() {
    let encoded = "A".repeat(43);
    let token = SessionToken::parse(&encoded).unwrap();
    assert_eq!(token.encode(), encoded);
    assert_eq!(token.clone().digest(), token.digest());
    let digest: TokenDigest = token.digest();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        write!(hex, "{byte:02x}").unwrap();
    }
    assert_eq!(hex, "66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925");
    assert_eq!(codec::decode(&encoded), Some([0; 32]));
    assert_eq!(codec::encode(&[0; 32]), encoded);
    assert_eq!(format!("{token:?}"), "SessionToken(<redacted>)");
}

#[test]
fn malformed_padding_lengths_and_prefixes_are_not_cookie_tokens() {
    for value in [
        String::new(),
        "short".into(),
        "A".repeat(42),
        "A".repeat(44),
        format!("{}=", "A".repeat(43)),
        format!("rbit_{}", "A".repeat(43)),
        format!("{}B", "A".repeat(42)),
    ] {
        assert!(SessionToken::parse(&value).is_none(), "{value}");
    }
    let token = SessionToken::generate();
    assert_eq!(token.encode().len(), 43);
    assert_eq!(SessionToken::parse(&token.encode()).unwrap().digest(), token.digest());
}
