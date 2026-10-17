//! Passkeys and security keys (`WebAuthn` Level 2) as a second factor.
//!
//! This is a deliberately small relying party: it asks for no attestation (`"none"`), so
//! registration trusts the browser the same way TOTP enrolment trusts the signed-in user,
//! and it verifies the parts that matter for security on every ceremony:
//!
//! * the client data: ceremony type, our single-use [`Challenge`] and our exact origin;
//! * the authenticator data: our RP ID hash and the user-presence flag;
//! * on sign-in, the signature over `authenticatorData || SHA-256(clientDataJSON)` with the
//!   stored public key, and a signature counter that must move forward (a counter that goes
//!   backwards means a cloned authenticator).
//!
//! Supported algorithms are ES256, `EdDSA` (Ed25519) and RS256, which between them cover
//! platform passkeys (Apple, Google, Windows Hello) and FIDO2 security keys.
//!
//! Options go to the browser as JSON with binary fields in base64url; the browser's answer
//! comes back in the same shape as `PublicKeyCredential.toJSON()`.

mod cbor;

use std::fmt;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::signature::Verifier as _;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use self::cbor::Value;
use crate::secret::random_bytes;

/// How long the browser may wait for the user, in milliseconds.
pub const CEREMONY_TIMEOUT_MS: u64 = 120_000;
/// The longest credential ID accepted (the spec's maximum).
pub const MAX_CREDENTIAL_ID_LEN: usize = 1023;

const ES256: i64 = -7;
const EDDSA: i64 = -8;
const RS256: i64 = -257;

/// Authenticator data flags.
const USER_PRESENT: u8 = 0x01;
const ATTESTED_CREDENTIAL_DATA: u8 = 0x40;

/// Why a `WebAuthn` response was refused. The reason is for logs, not for users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("passkey response refused: {0}")]
pub struct WebauthnError(pub &'static str);

type Result<T> = std::result::Result<T, WebauthnError>;

/// A single-use random challenge for one ceremony.
#[derive(Clone, PartialEq, Eq)]
pub struct Challenge([u8; 32]);

impl Challenge {
    pub fn generate() -> Self {
        Self(random_bytes())
    }

    fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }

    /// The challenge a browser's response (to either ceremony) claims to answer, so a
    /// caller holding several outstanding challenges can pick the right one. The response
    /// is still verified against it in full.
    pub fn answered_by(response_json: &str) -> Result<Self> {
        let credential: CredentialJson = serde_json::from_str(response_json).map_err(|_| WebauthnError("malformed response"))?;
        let client_data = base64url(&credential.response.client_data_json)?;
        let data: ClientData<'_> = serde_json::from_slice(&client_data).map_err(|_| WebauthnError("malformed client data"))?;
        base64url(&data.challenge)?.try_into().map(Self).map_err(|_| WebauthnError("wrong challenge"))
    }
}

impl fmt::Debug for Challenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Challenge(<redacted>)")
    }
}

/// Who we are to the authenticator: the RP ID (a host name) and the exact origin pages are
/// served from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelyingParty {
    id: String,
    origin: String,
    name: String,
}

impl RelyingParty {
    /// The relying party of a site served at `public_url` (e.g. `https://git.example.com`):
    /// its host is the RP ID and its scheme, host and port are the origin. `None` if the URL
    /// has no host.
    pub fn from_public_url(public_url: &str, name: &str) -> Option<Self> {
        let (scheme, rest) = public_url.trim().split_once("://")?;
        let authority = rest.split(['/', '?', '#']).next()?;
        let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
        let host =
            if let Some(bracketed) = authority.strip_prefix('[') { bracketed.split_once(']')?.0 } else { authority.split(':').next()? };
        if host.is_empty() {
            return None;
        }
        let scheme = scheme.to_ascii_lowercase();
        let mut authority = authority.to_ascii_lowercase();
        // Browsers leave default ports out of the origin.
        for default in [("http", ":80"), ("https", ":443")] {
            if scheme == default.0
                && let Some(stripped) = authority.strip_suffix(default.1)
            {
                authority = stripped.to_owned();
            }
        }
        Some(Self { id: host.to_ascii_lowercase(), origin: format!("{scheme}://{authority}"), name: name.to_owned() })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// `PublicKeyCredentialCreationOptions` as JSON for `navigator.credentials.create`.
    ///
    /// `user_handle` must be stable per account and contain nothing personal; `exclude` lists
    /// the account's existing credential IDs so the same authenticator isn't added twice.
    pub fn creation_options(&self, challenge: &Challenge, user_handle: &[u8], user_name: &str, exclude: &[Vec<u8>]) -> String {
        let algorithms: Vec<_> = [ES256, EDDSA, RS256].iter().map(|alg| json!({ "type": "public-key", "alg": alg })).collect();
        json!({
            "rp": { "id": self.id, "name": self.name },
            "user": { "id": URL_SAFE_NO_PAD.encode(user_handle), "name": user_name, "displayName": user_name },
            "challenge": challenge.encode(),
            "pubKeyCredParams": algorithms,
            "timeout": CEREMONY_TIMEOUT_MS,
            "excludeCredentials": descriptors(exclude),
            // A passkey if the authenticator can store one, a plain security key otherwise.
            "authenticatorSelection": { "residentKey": "preferred", "userVerification": "discouraged" },
            "attestation": "none",
        })
        .to_string()
    }

    /// `PublicKeyCredentialRequestOptions` as JSON for `navigator.credentials.get`, limited
    /// to the account's credentials.
    pub fn request_options(&self, challenge: &Challenge, allow: &[Vec<u8>]) -> String {
        json!({
            "rpId": self.id,
            "challenge": challenge.encode(),
            "timeout": CEREMONY_TIMEOUT_MS,
            "allowCredentials": descriptors(allow),
            "userVerification": "discouraged",
        })
        .to_string()
    }

    /// Checks the browser's answer to [`Self::creation_options`] and extracts the new
    /// credential.
    pub fn verify_registration(&self, challenge: &Challenge, response_json: &str) -> Result<NewCredential> {
        let credential: CredentialJson = serde_json::from_str(response_json).map_err(|_| WebauthnError("malformed response"))?;
        let attestation_object = credential.response.attestation_object.as_deref().ok_or(WebauthnError("no attestation object"))?;
        let client_data = base64url(&credential.response.client_data_json)?;
        self.check_client_data(&client_data, "webauthn.create", challenge)?;

        let attestation = cbor::decode(&base64url(attestation_object)?).ok_or(WebauthnError("malformed attestation object"))?;
        let auth_data = attestation.get_text("authData").and_then(Value::as_bytes).ok_or(WebauthnError("no authenticator data"))?;
        let parsed = self.check_authenticator_data(auth_data)?;
        if parsed.flags & ATTESTED_CREDENTIAL_DATA == 0 {
            return Err(WebauthnError("no attested credential"));
        }
        let rest = &auth_data[37..];
        let id_len = usize::from(u16::from_be_bytes(rest.get(16..18).ok_or(WebauthnError("truncated"))?.try_into().unwrap_or_default()));
        if id_len == 0 || id_len > MAX_CREDENTIAL_ID_LEN {
            return Err(WebauthnError("bad credential ID length"));
        }
        let id = rest.get(18..18 + id_len).ok_or(WebauthnError("truncated"))?.to_vec();
        let key_bytes = &rest[18 + id_len..];
        let (_, key_len) = cbor::decode_prefix(key_bytes).ok_or(WebauthnError("malformed public key"))?;
        let public_key = key_bytes[..key_len].to_vec();
        let algorithm = PublicKey::from_cose(&public_key)?.algorithm();
        if base64url(&credential.raw_id())? != id {
            return Err(WebauthnError("credential ID mismatch"));
        }
        let transports = credential.response.transports.unwrap_or_default().into_iter().filter(|t| t.len() <= 32).take(8).collect();
        Ok(NewCredential { id, public_key, algorithm, sign_count: parsed.sign_count, transports })
    }

    /// The credential ID a [`Self::request_options`] answer claims to come from, so the
    /// caller can look up its public key.
    pub fn assertion_credential_id(response_json: &str) -> Result<Vec<u8>> {
        let credential: CredentialJson = serde_json::from_str(response_json).map_err(|_| WebauthnError("malformed response"))?;
        let id = base64url(&credential.raw_id())?;
        if id.is_empty() || id.len() > MAX_CREDENTIAL_ID_LEN {
            return Err(WebauthnError("bad credential ID length"));
        }
        Ok(id)
    }

    /// Checks the browser's answer to [`Self::request_options`] against the stored public
    /// key and counter of the credential it names. Returns the new counter to store.
    pub fn verify_assertion(&self, challenge: &Challenge, response_json: &str, public_key: &[u8], stored_sign_count: u32) -> Result<u32> {
        let credential: CredentialJson = serde_json::from_str(response_json).map_err(|_| WebauthnError("malformed response"))?;
        let response = credential.response;
        let client_data = base64url(&response.client_data_json)?;
        self.check_client_data(&client_data, "webauthn.get", challenge)?;
        let auth_data = base64url(response.authenticator_data.as_deref().ok_or(WebauthnError("no authenticator data"))?)?;
        let parsed = self.check_authenticator_data(&auth_data)?;
        let signature = base64url(response.signature.as_deref().ok_or(WebauthnError("no signature"))?)?;

        let mut signed = auth_data;
        signed.extend_from_slice(&Sha256::digest(&client_data));
        if !PublicKey::from_cose(public_key)?.verify(&signed, &signature) {
            return Err(WebauthnError("bad signature"));
        }
        // Authenticators without a counter always send 0; otherwise it must increase.
        if (parsed.sign_count != 0 || stored_sign_count != 0) && parsed.sign_count <= stored_sign_count {
            return Err(WebauthnError("signature counter went backwards"));
        }
        Ok(parsed.sign_count)
    }

    fn check_client_data(&self, client_data: &[u8], ceremony: &str, challenge: &Challenge) -> Result<()> {
        let data: ClientData<'_> = serde_json::from_slice(client_data).map_err(|_| WebauthnError("malformed client data"))?;
        if data.kind != ceremony {
            return Err(WebauthnError("wrong ceremony type"));
        }
        if base64url(&data.challenge)? != challenge.0 {
            return Err(WebauthnError("wrong challenge"));
        }
        if data.origin != self.origin {
            return Err(WebauthnError("wrong origin"));
        }
        if data.cross_origin == Some(true) {
            return Err(WebauthnError("cross-origin request"));
        }
        Ok(())
    }

    fn check_authenticator_data(&self, auth_data: &[u8]) -> Result<AuthenticatorData> {
        if auth_data.len() < 37 {
            return Err(WebauthnError("truncated authenticator data"));
        }
        if auth_data[..32] != *Sha256::digest(self.id.as_bytes()) {
            return Err(WebauthnError("wrong RP ID"));
        }
        let flags = auth_data[32];
        if flags & USER_PRESENT == 0 {
            return Err(WebauthnError("user not present"));
        }
        let sign_count = u32::from_be_bytes(auth_data[33..37].try_into().unwrap_or_default());
        Ok(AuthenticatorData { flags, sign_count })
    }
}

/// A credential that passed registration, ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCredential {
    pub id: Vec<u8>,
    /// The COSE key exactly as the authenticator sent it.
    pub public_key: Vec<u8>,
    /// COSE algorithm identifier (-7, -8 or -257).
    pub algorithm: i64,
    pub sign_count: u32,
    /// Hints such as `usb`, `nfc`, `internal`, `hybrid`, passed back on sign-in.
    pub transports: Vec<String>,
}

/// `[{type, id}]` for `excludeCredentials` / `allowCredentials`.
fn descriptors(ids: &[Vec<u8>]) -> Vec<serde_json::Value> {
    ids.iter().map(|id| json!({ "type": "public-key", "id": URL_SAFE_NO_PAD.encode(id) })).collect()
}

fn base64url(value: &str) -> Result<Vec<u8>> {
    // Some browsers pad; the spec doesn't.
    URL_SAFE_NO_PAD.decode(value.trim_end_matches('=')).map_err(|_| WebauthnError("bad base64url"))
}

struct AuthenticatorData {
    flags: u8,
    sign_count: u32,
}

#[derive(Deserialize)]
struct ClientData<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    challenge: String,
    origin: String,
    #[serde(rename = "crossOrigin")]
    cross_origin: Option<bool>,
}

#[derive(Deserialize)]
struct CredentialJson {
    id: String,
    #[serde(rename = "rawId")]
    raw_id: Option<String>,
    response: ResponseJson,
}

impl CredentialJson {
    fn raw_id(&self) -> String {
        self.raw_id.clone().unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseJson {
    #[serde(rename = "clientDataJSON")]
    client_data_json: String,
    attestation_object: Option<String>,
    transports: Option<Vec<String>>,
    authenticator_data: Option<String>,
    signature: Option<String>,
}

/// A stored credential public key.
enum PublicKey {
    Es256(p256::ecdsa::VerifyingKey),
    EdDsa(ed25519_dalek::VerifyingKey),
    Rs256(Box<rsa::pkcs1v15::VerifyingKey<Sha256>>),
}

impl PublicKey {
    /// Parses a `COSE_Key` (RFC 9053) of a supported algorithm.
    fn from_cose(bytes: &[u8]) -> Result<Self> {
        let key = cbor::decode(bytes).ok_or(WebauthnError("malformed public key"))?;
        let int = |label| key.get_int(label).and_then(Value::as_int);
        let bytes = |label| key.get_int(label).and_then(Value::as_bytes);
        let unsupported = WebauthnError("unsupported public key");
        match (int(1), int(3)) {
            // kty EC2, crv P-256.
            (Some(2), Some(-7)) if int(-1) == Some(1) => {
                let (x, y) = (bytes(-2).ok_or(unsupported)?, bytes(-3).ok_or(unsupported)?);
                if x.len() != 32 || y.len() != 32 {
                    return Err(unsupported);
                }
                let point = [&[0x04], x, y].concat();
                p256::ecdsa::VerifyingKey::from_sec1_bytes(&point).map(Self::Es256).map_err(|_| unsupported)
            }
            // kty OKP, crv Ed25519.
            (Some(1), Some(-8)) if int(-1) == Some(6) => {
                let x: &[u8; 32] = bytes(-2).and_then(|x| x.try_into().ok()).ok_or(unsupported)?;
                ed25519_dalek::VerifyingKey::from_bytes(x).map(Self::EdDsa).map_err(|_| unsupported)
            }
            // kty RSA.
            (Some(3), Some(-257)) => {
                let n = rsa::BoxedUint::from_be_slice_vartime(bytes(-1).ok_or(unsupported)?);
                let e = rsa::BoxedUint::from_be_slice_vartime(bytes(-2).ok_or(unsupported)?);
                let key = rsa::RsaPublicKey::new(n, e).map_err(|_| unsupported)?;
                if rsa::traits::PublicKeyParts::size(&key) < 256 {
                    return Err(WebauthnError("RSA key shorter than 2048 bits"));
                }
                Ok(Self::Rs256(Box::new(rsa::pkcs1v15::VerifyingKey::new(key))))
            }
            _ => Err(unsupported),
        }
    }

    fn algorithm(&self) -> i64 {
        match self {
            Self::Es256(_) => ES256,
            Self::EdDsa(_) => EDDSA,
            Self::Rs256(_) => RS256,
        }
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        match self {
            Self::Es256(key) => p256::ecdsa::Signature::from_der(signature)
                // Some authenticators produce high-S signatures; both forms are valid ECDSA.
                .is_ok_and(|signature| key.verify(message, &signature.normalize_s()).is_ok()),
            Self::EdDsa(key) => {
                ed25519_dalek::Signature::from_slice(signature).is_ok_and(|signature| key.verify_strict(message, &signature).is_ok())
            }
            Self::Rs256(key) => {
                rsa::pkcs1v15::Signature::try_from(signature).is_ok_and(|signature| key.verify(message, &signature).is_ok())
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
}
