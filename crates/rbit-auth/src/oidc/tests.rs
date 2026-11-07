//! Existing OIDC flow and signature/claim acceptance fixtures.

use p256::ecdsa::{SigningKey, signature::Signer as _};
use serde_json::json;

use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;
use sha2::Sha256;

const ISSUER: &str = "https://sso.example.com";
const CLIENT: &str = "rbit";
const NOW: i64 = 1_800_000_000;

/// A 2048-bit RSA key (test only), from its primes.
const RSA_P: &str = "d8a261abe2b17c665e43ac30f9c507b91ab449b0b0286551a6730309921404cd55469a6219adcc1d6c009e80274231d3c866ba282e6c771e8740275bee7d579f3873a64aeff03dc0050ee7b3f92e0ae99324f2aa2bfed739482df1eca6bede7590616163bcafe1ad32a1fc649d1cf85c1a95410fa9e0900c14df4fa00eb0d1b7";
const RSA_Q: &str = "d19702292ef00f068f3763582b71731ce7a41b55655f8d62221838d9db2da4124539adf68427e82cb8cb1674feea0921fc0f775a182edfc81b36b5befffb628b83509b06162f2fc14c1dfec7448facfb8a838cf08545888438dbe3bac274933c1eae09204cea6321a4d118ea3b7a4d33fde8f0b432d581a3d51bd89caa811d05";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn rsa_key() -> rsa::RsaPrivateKey {
    let p = rsa::BoxedUint::from_be_slice_vartime(&hex(RSA_P));
    let q = rsa::BoxedUint::from_be_slice_vartime(&hex(RSA_Q));
    rsa::RsaPrivateKey::from_p_q(p, q, rsa::BoxedUint::from(65_537_u32)).unwrap()
}

fn ec_key() -> SigningKey {
    SigningKey::from_slice(&[0x17; 32]).unwrap()
}

fn rsa_jwk(kid: &str) -> Value {
    use rsa::traits::PublicKeyParts as _;
    let key = rsa_key().to_public_key();
    json!({ "kty": "RSA", "kid": kid, "use": "sig", "alg": "RS256",
            "n": URL_SAFE_NO_PAD.encode(key.n().to_be_bytes()), "e": URL_SAFE_NO_PAD.encode(key.e().to_be_bytes()) })
}

fn ec_jwk(kid: &str) -> Value {
    let point = ec_key().verifying_key().to_sec1_point(false);
    let bytes = point.as_bytes();
    json!({ "kty": "EC", "crv": "P-256", "kid": kid, "x": URL_SAFE_NO_PAD.encode(&bytes[1..33]), "y": URL_SAFE_NO_PAD.encode(&bytes[33..65]) })
}

fn keys(list: &[Value]) -> JwkSet {
    JwkSet::parse(json!({ "keys": list }).to_string().as_bytes()).unwrap()
}

fn claims() -> Value {
    json!({ "iss": ISSUER, "aud": CLIENT, "sub": "user-1", "exp": NOW + 300, "iat": NOW, "nonce": "n0nce",
            "email": "alice@example.com", "email_verified": true, "preferred_username": "alice", "amr": ["pwd", "mfa"] })
}

fn sign(header: &Value, claims: &Value) -> String {
    let signed = format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
    let signature = match header["alg"].as_str() {
        Some("RS256") => {
            use rsa::signature::SignatureEncoding as _;
            rsa::pkcs1v15::SigningKey::<Sha256>::new(rsa_key()).sign(signed.as_bytes()).to_vec()
        }
        Some("ES256") => {
            let signature: p256::ecdsa::Signature = ec_key().sign(signed.as_bytes());
            signature.to_bytes().to_vec()
        }
        _ => b"forged".to_vec(),
    };
    format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature))
}

fn check() -> IdTokenCheck<'static> {
    IdTokenCheck { issuer: ISSUER, client_id: CLIENT, nonce: "n0nce", now: NOW }
}

#[test]
fn flows_seal_and_open() {
    let key = FlowKey::new(&[7; 32]).unwrap();
    assert!(FlowKey::new(&[7; 16]).is_none(), "short keys are refused");
    let flow = Flow::new("corp", "/settings", Some(4), NOW);
    assert_eq!(flow.state.len(), 43);
    let sealed = key.seal(&flow);
    assert!(sealed.starts_with("v1."));
    assert_eq!(key.open(&sealed, NOW + 10, 600), Some(flow.clone()));
    assert!(flow.state_matches(&flow.state.clone()));
    assert!(!flow.state_matches("other"));

    assert_eq!(key.open(&sealed, NOW + 601, 600), None, "expired");
    assert_eq!(key.open(&sealed, NOW - 120, 600), None, "from the future");
    assert_eq!(FlowKey::new(&[8; 32]).unwrap().open(&sealed, NOW, 600), None, "another key");
    let (head, mac) = sealed.rsplit_once('.').unwrap();
    let mut tampered = Flow { link_account: Some(1), ..flow.clone() };
    tampered.issued_at = NOW;
    let forged = format!("v1.{}.{mac}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(&tampered).unwrap()));
    assert_eq!(key.open(&forged, NOW, 600), None, "a changed payload needs a new MAC");
    assert_eq!(key.open(&format!("{head}.AAAA"), NOW, 600), None);
    assert_eq!(key.open("garbage", NOW, 600), None);

    // Derived secrets are stable per flow and differ between flows.
    let other = Flow::new("corp", "/", None, NOW);
    assert_eq!(key.pkce(&flow), key.pkce(&flow));
    assert_ne!(key.pkce(&flow), key.pkce(&other));
    assert_eq!(key.pkce(&flow).verifier().len(), 43);
    assert_ne!(key.nonce(&flow), key.nonce(&other));
    assert_ne!(key.nonce(&flow), key.pkce(&flow).verifier(), "labels separate the purposes");
}

#[test]
fn link_tokens_are_bound_to_a_session() {
    let key = FlowKey::new(&[7; 32]).unwrap();
    let token = key.link_token(&[1; 32]);
    assert!(key.check_link_token(&[1; 32], &token));
    assert!(!key.check_link_token(&[2; 32], &token));
    assert!(!key.check_link_token(&[1; 32], "x"));
}

#[test]
fn accepts_rs256_and_es256_tokens() {
    let set = keys(&[rsa_jwk("r1"), ec_jwk("e1")]);
    assert_eq!(set.len(), 2);
    let token = sign(&json!({ "alg": "RS256", "kid": "r1" }), &claims());
    assert_eq!(token_key_id(&token).unwrap().as_deref(), Some("r1"));
    let verified = verify_id_token(&token, &set, &check()).unwrap();
    assert_eq!(verified.subject, "user-1");
    assert_eq!(verified.email.as_deref(), Some("alice@example.com"));
    assert!(verified.email_verified);
    assert_eq!(verified.preferred_username.as_deref(), Some("alice"));
    assert_eq!(verified.amr, ["pwd", "mfa"]);

    // Without a `kid`, every key of the right type is tried.
    let token = sign(&json!({ "alg": "ES256" }), &claims());
    assert_eq!(verify_id_token(&token, &set, &check()).unwrap().subject, "user-1");
}

#[test]
fn refuses_bad_signatures_and_algorithms() {
    let set = keys(&[rsa_jwk("r1"), ec_jwk("e1")]);
    let token = sign(&json!({ "alg": "RS256", "kid": "r1" }), &claims());
    let (head, signature) = token.rsplit_once('.').unwrap();
    let (header, _) = head.split_once('.').unwrap();
    let mut changed = claims();
    changed["sub"] = json!("admin");
    let forged = format!("{header}.{}.{signature}", URL_SAFE_NO_PAD.encode(changed.to_string()));
    assert_eq!(verify_id_token(&forged, &set, &check()), Err(OidcError::BadSignature));

    let ec_signed_as_rsa = sign(&json!({ "alg": "ES256", "kid": "r1" }), &claims());
    assert_eq!(verify_id_token(&ec_signed_as_rsa, &set, &check()), Err(OidcError::UnknownKey), "a kid of the wrong type");
    for alg in ["none", "HS256", "RS512", "PS256"] {
        let token = sign(&json!({ "alg": alg, "kid": "r1" }), &claims());
        assert_eq!(verify_id_token(&token, &set, &check()), Err(OidcError::UnsupportedAlgorithm(alg.to_owned())));
    }
    let unsigned = format!("{}.{}.", URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#), URL_SAFE_NO_PAD.encode(claims().to_string()));
    assert!(verify_id_token(&unsigned, &set, &check()).is_err());

    let rotated = sign(&json!({ "alg": "RS256", "kid": "r2" }), &claims());
    assert_eq!(verify_id_token(&rotated, &set, &check()), Err(OidcError::UnknownKey), "an unknown kid asks for a refetch");
    assert!(matches!(verify_id_token("a.b", &set, &check()), Err(OidcError::Malformed(_))));
}

#[test]
fn checks_every_claim() {
    let set = keys(&[ec_jwk("e1")]);
    let verify = |changes: Value| {
        let mut body = claims();
        for (name, value) in changes.as_object().unwrap() {
            if value.is_null() {
                body.as_object_mut().unwrap().remove(name);
            } else {
                body[name] = value.clone();
            }
        }
        verify_id_token(&sign(&json!({ "alg": "ES256", "kid": "e1" }), &body), &set, &check())
    };
    assert!(verify(json!({})).is_ok());
    assert_eq!(verify(json!({ "iss": "https://evil.example" })), Err(OidcError::Claim("iss")));
    assert_eq!(verify(json!({ "iss": "https://sso.example.com/" })), Err(OidcError::Claim("iss")), "compared exactly");
    assert_eq!(verify(json!({ "aud": "someone-else" })), Err(OidcError::Claim("aud")));
    assert!(verify(json!({ "aud": ["rbit"] })).is_ok());
    assert_eq!(verify(json!({ "aud": ["rbit", "other"] })), Err(OidcError::Claim("azp")), "several audiences need azp");
    assert!(verify(json!({ "aud": ["rbit", "other"], "azp": "rbit" })).is_ok());
    assert_eq!(verify(json!({ "azp": "other" })), Err(OidcError::Claim("azp")));
    assert_eq!(verify(json!({ "exp": NOW - CLOCK_LEEWAY_SECONDS })), Err(OidcError::Claim("exp")), "expired");
    assert!(verify(json!({ "exp": NOW - 10 })).is_ok(), "within the leeway");
    assert_eq!(verify(json!({ "exp": null })), Err(OidcError::Claim("exp")));
    assert_eq!(verify(json!({ "iat": NOW + 600 })), Err(OidcError::Claim("iat")));
    assert_eq!(verify(json!({ "nbf": NOW + 600 })), Err(OidcError::Claim("nbf")));
    assert_eq!(verify(json!({ "nonce": "replayed" })), Err(OidcError::Claim("nonce")));
    assert_eq!(verify(json!({ "nonce": null })), Err(OidcError::Claim("nonce")));
    assert_eq!(verify(json!({ "sub": "" })), Err(OidcError::Claim("sub")));
    assert!(!verify(json!({ "email_verified": false })).unwrap().email_verified);
    assert!(verify(json!({ "email_verified": "true" })).unwrap().email_verified);
    assert!(!verify(json!({ "email_verified": null })).unwrap().email_verified, "absent means unverified");
}

#[test]
fn key_sets_skip_unusable_keys() {
    let set = keys(&[
        json!({ "kty": "oct", "k": "c2VjcmV0" }),
        json!({ "kty": "EC", "crv": "P-384", "x": "AA", "y": "AA" }),
        {
            let mut key = rsa_jwk("enc");
            key["use"] = json!("enc");
            key
        },
        ec_jwk("e1"),
    ]);
    assert_eq!(set.len(), 1);
    assert!(JwkSet::parse(br#"{"keys": [{"kty": "oct", "k": "c2VjcmV0"}]}"#).is_err());
    assert!(JwkSet::parse(b"not json").is_err());
    // A key pinned to another algorithm is not used for this one.
    let mut pinned = ec_jwk("e1");
    pinned["alg"] = json!("ES384");
    let token = sign(&json!({ "alg": "ES256", "kid": "e1" }), &claims());
    assert_eq!(verify_id_token(&token, &keys(&[pinned]), &check()), Err(OidcError::UnknownKey));
}

#[test]
fn sealed_flow_debug_hides_state_navigation_and_link_identity_without_changing_wire_fields() {
    let key = FlowKey::new(&[7; 32]).unwrap();
    let flow = Flow::new("PRIVATE_PROVIDER_SENTINEL", "/PRIVATE_NEXT_SENTINEL", Some(847_261), NOW);
    let sealed = key.seal(&flow);
    let opened = key.open(&sealed, NOW, 600).unwrap();
    assert_eq!(opened, flow);
    assert_eq!(serde_json::to_value(&opened).unwrap()["s"], flow.state);
    for debug in [format!("{opened:?}"), format!("{opened:#?}")] {
        for private in [flow.state.as_str(), "PRIVATE_PROVIDER_SENTINEL", "PRIVATE_NEXT_SENTINEL", "847261"] {
            assert!(!debug.contains(private), "flow Debug must not disclose correlation or identity data");
        }
    }
}

#[test]
fn verified_claims_and_check_debug_hide_real_signed_fixture_identity_and_nonce() {
    let set = keys(&[ec_jwk("e1")]);
    let check = IdTokenCheck { nonce: "PRIVATE_NONCE_SENTINEL", ..check() };
    let mut body = claims();
    for (field, value) in [
        ("sub", "PRIVATE_SUBJECT_SENTINEL"),
        ("email", "private@example.test"),
        ("name", "PRIVATE_NAME_SENTINEL"),
        ("preferred_username", "PRIVATE_USERNAME_SENTINEL"),
        ("nonce", check.nonce),
    ] {
        body[field] = json!(value);
    }
    body["amr"] = json!(["PRIVATE_METHOD_SENTINEL"]);
    let token = sign(&json!({ "alg": "ES256", "kid": "e1" }), &body);
    let verified = verify_id_token(&token, &set, &check).unwrap();
    assert_eq!(verified.subject, "PRIVATE_SUBJECT_SENTINEL");
    assert_eq!(verified.email.as_deref(), Some("private@example.test"));
    assert_eq!(verified.name.as_deref(), Some("PRIVATE_NAME_SENTINEL"));
    assert_eq!(verified.preferred_username.as_deref(), Some("PRIVATE_USERNAME_SENTINEL"));
    assert_eq!(verified.amr, ["PRIVATE_METHOD_SENTINEL"]);
    for debug in [format!("{check:?}"), format!("{check:#?}"), format!("{verified:?}"), format!("{verified:#?}")] {
        for private in [
            check.nonce,
            "PRIVATE_SUBJECT_SENTINEL",
            "private@example.test",
            "PRIVATE_NAME_SENTINEL",
            "PRIVATE_USERNAME_SENTINEL",
            "PRIVATE_METHOD_SENTINEL",
        ] {
            assert!(!debug.contains(private), "token Debug must not disclose nonce or verified identity claims");
        }
    }
}
