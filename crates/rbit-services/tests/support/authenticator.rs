//! Shared software authenticator for integration and private real-store race tests.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer as _};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const ORIGIN: &str = "http://localhost:3000";

/// A software authenticator holding one P-256 key, with a signature counter.
pub(super) struct Authenticator {
    key: SigningKey,
    id: Vec<u8>,
    counter: std::cell::Cell<u32>,
}

impl Authenticator {
    pub(super) fn new(seed: u8) -> Self {
        Self { key: SigningKey::from_slice(&[seed; 32]).unwrap(), id: vec![seed; 20], counter: std::cell::Cell::new(0) }
    }

    fn auth_data(&self, rp_id: &str, attested: bool) -> Vec<u8> {
        let mut data = Sha256::digest(rp_id.as_bytes()).to_vec();
        data.push(if attested { 0x41 } else { 0x01 });
        data.extend(self.counter.get().to_be_bytes());
        if attested {
            let point = self.key.verifying_key().to_sec1_point(false);
            let point = point.as_bytes();
            data.extend([0; 16]);
            data.extend(u16::try_from(self.id.len()).unwrap().to_be_bytes());
            data.extend(&self.id);
            // COSE_Key {1: 2, 3: -7, -1: 1, -2: x, -3: y}
            data.extend([0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20]);
            data.extend(&point[1..33]);
            data.extend([0x22, 0x58, 0x20]);
            data.extend(&point[33..65]);
        }
        data
    }

    fn client_data(kind: &str, options: &Value) -> Vec<u8> {
        json!({ "type": kind, "challenge": options["challenge"], "origin": ORIGIN }).to_string().into_bytes()
    }

    /// Answers `navigator.credentials.create`.
    pub(super) fn create(&self, options: &str) -> String {
        let options: Value = serde_json::from_str(options).unwrap();
        let auth_data = self.auth_data(options["rp"]["id"].as_str().unwrap(), true);
        // {"fmt": "none", "attStmt": {}, "authData": <bytes>}
        let mut attestation = vec![0xa3, 0x63, b'f', b'm', b't', 0x64, b'n', b'o', b'n', b'e'];
        attestation.extend([0x67, b'a', b't', b't', b'S', b't', b'm', b't', 0xa0]);
        attestation.extend([0x68, b'a', b'u', b't', b'h', b'D', b'a', b't', b'a', 0x59]);
        attestation.extend(u16::try_from(auth_data.len()).unwrap().to_be_bytes());
        attestation.extend(auth_data);
        let id = URL_SAFE_NO_PAD.encode(&self.id);
        json!({
            "id": id, "rawId": id, "type": "public-key",
            "response": {
                "clientDataJSON": URL_SAFE_NO_PAD.encode(Self::client_data("webauthn.create", &options)),
                "attestationObject": URL_SAFE_NO_PAD.encode(attestation),
                "transports": ["usb"],
            }
        })
        .to_string()
    }

    /// Answers `navigator.credentials.get`.
    pub(super) fn get(&self, options: &str) -> String {
        let options: Value = serde_json::from_str(options).unwrap();
        self.counter.set(self.counter.get() + 1);
        let auth_data = self.auth_data(options["rpId"].as_str().unwrap(), false);
        let client_data = Self::client_data("webauthn.get", &options);
        let mut signed = auth_data.clone();
        signed.extend(Sha256::digest(&client_data));
        let signature: Signature = self.key.sign(&signed);
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
