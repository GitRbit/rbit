//! Existing software-authenticator registration, assertion and refusal fixtures.

use p256::ecdsa::{SigningKey, signature::Signer as _};

use super::{cbor::tests::encode, *};

const ORIGIN: &str = "https://git.example.com";

fn rp() -> RelyingParty {
    RelyingParty::from_public_url(ORIGIN, "rbit").unwrap()
}

/// A software authenticator holding one P-256 key.
struct Authenticator {
    key: SigningKey,
    id: Vec<u8>,
    counter: u32,
}

impl Authenticator {
    fn new() -> Self {
        let key = SigningKey::from_slice(&[0x42; 32]).unwrap();
        Self { key, id: vec![9; 16], counter: 0 }
    }

    fn cose_key(&self) -> Vec<u8> {
        let point = self.key.verifying_key().to_sec1_point(false);
        let bytes = point.as_bytes();
        encode(&Value::Map(vec![
            (Value::Integer(1), Value::Integer(2)),
            (Value::Integer(3), Value::Integer(-7)),
            (Value::Integer(-1), Value::Integer(1)),
            (Value::Integer(-2), Value::Bytes(bytes[1..33].to_vec())),
            (Value::Integer(-3), Value::Bytes(bytes[33..65].to_vec())),
        ]))
    }

    fn auth_data(&self, rp_id: &str, flags: u8, attested: bool) -> Vec<u8> {
        let mut data = Sha256::digest(rp_id.as_bytes()).to_vec();
        data.push(flags | if attested { ATTESTED_CREDENTIAL_DATA } else { 0 });
        data.extend(self.counter.to_be_bytes());
        if attested {
            data.extend([0; 16]);
            data.extend(u16::try_from(self.id.len()).unwrap().to_be_bytes());
            data.extend(&self.id);
            data.extend(self.cose_key());
        }
        data
    }

    fn client_data(kind: &str, challenge: &Challenge, origin: &str) -> Vec<u8> {
        json!({ "type": kind, "challenge": challenge.encode(), "origin": origin, "crossOrigin": false }).to_string().into_bytes()
    }

    fn register(&self, challenge: &Challenge) -> String {
        let auth_data = self.auth_data("git.example.com", USER_PRESENT, true);
        let attestation = encode(&Value::Map(vec![
            (Value::Text("fmt".to_owned()), Value::Text("none".to_owned())),
            (Value::Text("attStmt".to_owned()), Value::Map(vec![])),
            (Value::Text("authData".to_owned()), Value::Bytes(auth_data)),
        ]));
        let id = URL_SAFE_NO_PAD.encode(&self.id);
        json!({
            "id": id, "rawId": id, "type": "public-key",
            "response": {
                "clientDataJSON": URL_SAFE_NO_PAD.encode(Self::client_data("webauthn.create", challenge, ORIGIN)),
                "attestationObject": URL_SAFE_NO_PAD.encode(attestation),
                "transports": ["usb", "nfc"],
            }
        })
        .to_string()
    }

    fn sign_in(&mut self, challenge: &Challenge, rp_id: &str, origin: &str, flags: u8) -> String {
        self.counter += 1;
        let auth_data = self.auth_data(rp_id, flags, false);
        let client_data = Self::client_data("webauthn.get", challenge, origin);
        let mut signed = auth_data.clone();
        signed.extend(Sha256::digest(&client_data));
        let signature: p256::ecdsa::Signature = self.key.sign(&signed);
        let id = URL_SAFE_NO_PAD.encode(&self.id);
        json!({
            "id": id, "rawId": id, "type": "public-key",
            "response": {
                "clientDataJSON": URL_SAFE_NO_PAD.encode(client_data),
                "authenticatorData": URL_SAFE_NO_PAD.encode(auth_data),
                "signature": URL_SAFE_NO_PAD.encode(signature.to_der().as_bytes()),
            }
        })
        .to_string()
    }
}

#[test]
fn derives_rp_id_and_origin_from_the_public_url() {
    let rp = RelyingParty::from_public_url("https://Git.Example.com:443/sub/", "rbit").unwrap();
    assert_eq!((rp.id(), rp.origin()), ("git.example.com", "https://git.example.com"));
    let rp = RelyingParty::from_public_url("http://localhost:3000", "rbit").unwrap();
    assert_eq!((rp.id(), rp.origin()), ("localhost", "http://localhost:3000"));
    assert!(RelyingParty::from_public_url("not a url", "rbit").is_none());
}

#[test]
fn options_carry_the_challenge_and_credentials() {
    let challenge = Challenge::generate();
    let create: serde_json::Value = serde_json::from_str(&rp().creation_options(&challenge, &[1, 2], "alice", &[vec![7, 7]])).unwrap();
    assert_eq!(create["rp"]["id"], "git.example.com");
    assert_eq!(create["challenge"], challenge.encode());
    assert_eq!(create["user"]["id"], "AQI");
    assert_eq!(create["excludeCredentials"][0]["id"], "Bwc");
    let get: serde_json::Value = serde_json::from_str(&rp().request_options(&challenge, &[vec![7, 7]])).unwrap();
    assert_eq!(get["allowCredentials"][0]["id"], "Bwc");
}

#[test]
fn registers_and_signs_in() {
    let mut authenticator = Authenticator::new();
    let challenge = Challenge::generate();
    let credential = rp().verify_registration(&challenge, &authenticator.register(&challenge)).unwrap();
    assert_eq!(credential.id, authenticator.id);
    assert_eq!(credential.algorithm, ES256);
    assert_eq!(credential.transports, ["usb", "nfc"]);

    let challenge = Challenge::generate();
    let response = authenticator.sign_in(&challenge, "git.example.com", ORIGIN, USER_PRESENT);
    assert_eq!(RelyingParty::assertion_credential_id(&response).unwrap(), authenticator.id);
    assert_eq!(Challenge::answered_by(&response), Ok(challenge.clone()));
    let count = rp().verify_assertion(&challenge, &response, &credential.public_key, credential.sign_count).unwrap();
    assert_eq!(count, 1);

    // Replaying the same response: the counter no longer moves forward.
    assert_eq!(
        rp().verify_assertion(&challenge, &response, &credential.public_key, count),
        Err(WebauthnError("signature counter went backwards"))
    );
}

#[test]
fn refuses_registration_for_another_challenge() {
    let authenticator = Authenticator::new();
    let response = authenticator.register(&Challenge::generate());
    assert_eq!(rp().verify_registration(&Challenge::generate(), &response), Err(WebauthnError("wrong challenge")));
}

#[test]
fn refuses_assertions_that_are_not_ours() {
    let mut authenticator = Authenticator::new();
    let challenge = Challenge::generate();
    let key = rp().verify_registration(&challenge, &authenticator.register(&challenge)).unwrap().public_key;
    let challenge = Challenge::generate();
    let check = |response: String| rp().verify_assertion(&challenge, &response, &key, 0);

    assert_eq!(
        check(authenticator.sign_in(&challenge, "git.example.com", "https://evil.example", USER_PRESENT)),
        Err(WebauthnError("wrong origin"))
    );
    assert_eq!(check(authenticator.sign_in(&challenge, "evil.example", ORIGIN, USER_PRESENT)), Err(WebauthnError("wrong RP ID")));
    assert_eq!(check(authenticator.sign_in(&challenge, "git.example.com", ORIGIN, 0)), Err(WebauthnError("user not present")));
    assert_eq!(
        check(authenticator.sign_in(&Challenge::generate(), "git.example.com", ORIGIN, USER_PRESENT)),
        Err(WebauthnError("wrong challenge"))
    );

    // A signature from a different key.
    let other = SigningKey::from_slice(&[0x43; 32]).unwrap();
    let mut impostor = Authenticator { key: other, id: authenticator.id.clone(), counter: 5 };
    assert_eq!(check(impostor.sign_in(&challenge, "git.example.com", ORIGIN, USER_PRESENT)), Err(WebauthnError("bad signature")));
}

#[test]
fn verifies_ed25519_keys() {
    let signing = ed25519_dalek::SigningKey::from_bytes(&[5; 32]);
    let cose = encode(&Value::Map(vec![
        (Value::Integer(1), Value::Integer(1)),
        (Value::Integer(3), Value::Integer(-8)),
        (Value::Integer(-1), Value::Integer(6)),
        (Value::Integer(-2), Value::Bytes(signing.verifying_key().to_bytes().to_vec())),
    ]));
    let key = PublicKey::from_cose(&cose).unwrap();
    assert_eq!(key.algorithm(), EDDSA);
    let signature = ed25519_dalek::Signer::sign(&signing, b"message");
    assert!(key.verify(b"message", &signature.to_bytes()));
    assert!(!key.verify(b"other", &signature.to_bytes()));
}

#[test]
fn refuses_unsupported_keys() {
    let cose = encode(&Value::Map(vec![(Value::Integer(1), Value::Integer(2)), (Value::Integer(3), Value::Integer(-35))]));
    assert!(PublicKey::from_cose(&cose).is_err());
    assert!(PublicKey::from_cose(&[0xff]).is_err());
}
