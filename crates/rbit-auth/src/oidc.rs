//! Building blocks for signing in with an external identity provider (OIDC and
//! GitHub's OAuth web flow), with no HTTP or storage:
//!
//! * [`FlowKey`] — keyed with a server secret, it seals the short-lived flow cookie
//!   ([`Flow`]: provider, `state`, where to go next, which account is linking) with
//!   HMAC-SHA256, and derives the flow's PKCE verifier and OIDC `nonce` from its random
//!   `state`, so neither has to be stored anywhere. It also issues the per-session token
//!   that authorises starting a link from the settings page.
//! * [`Pkce`] — RFC 7636 with `S256` only.
//! * [`JwkSet`] and [`verify_id_token`] — ID token validation: compact JWS with `RS256`
//!   (RSA ≥ 2048 bits) or `ES256` only (`none`, `HS*` and everything else are refused),
//!   the key chosen by `kid` (an unknown `kid` is reported as [`OidcError::UnknownKey`] so
//!   the caller can fetch the provider's keys again), then `iss`, `aud` (with `azp` when
//!   there are several audiences), `exp`, `iat` and `nbf` with [`CLOCK_LEEWAY_SECONDS`],
//!   and the `nonce`.
//!
//! Milestone 3 workstream O (external sign-in) owns this file.

use std::fmt;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use p256::ecdsa::signature::Verifier as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::secret::random_bytes;

/// Clock skew tolerated between us and the provider.
pub const CLOCK_LEEWAY_SECONDS: i64 = 60;
/// Longest ID token accepted (real ones are 1–2 KiB).
const MAX_TOKEN_LEN: usize = 16 * 1024;
/// Shortest server key accepted.
const MIN_KEY_LEN: usize = 32;
/// Longest `sub` accepted (OIDC allows 255 ASCII characters).
const MAX_SUBJECT_LEN: usize = 255;
/// Prefix of sealed flow cookies, so the format can change later.
const FLOW_VERSION: &str = "v1";

type HmacSha256 = Hmac<Sha256>;

/// Why an ID token (or a key set) was refused. Messages are for logs, not for users.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OidcError {
    #[error("malformed: {0}")]
    Malformed(&'static str),
    #[error("signature algorithm {0:?} is not accepted (only RS256 and ES256)")]
    UnsupportedAlgorithm(String),
    /// No key in the set matches the token: fetch the keys again (they may have rotated).
    #[error("no matching signing key")]
    UnknownKey,
    #[error("signature does not verify")]
    BadSignature,
    #[error("claim check failed: {0}")]
    Claim(&'static str),
}

// ---------------------------------------------------------------------- PKCE

/// A PKCE code verifier (RFC 7636): 43 characters of base64url.
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce(String);

impl fmt::Debug for Pkce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pkce(<redacted>)")
    }
}

impl Pkce {
    /// The only method offered.
    pub const METHOD: &'static str = "S256";

    fn from_bytes(bytes: &[u8]) -> Self {
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Sent with the token request.
    pub fn verifier(&self) -> &str {
        &self.0
    }

    /// Sent with the authorization request: base64url(SHA-256(verifier)).
    pub fn challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(self.0.as_bytes()))
    }
}

// ---------------------------------------------------------------------- the flow cookie

/// One sign-in (or link) in progress, carried by the browser in a sealed cookie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    /// The provider id the flow was started for.
    #[serde(rename = "p")]
    pub provider: String,
    /// The random `state` sent to the provider (base64url, 256 bits).
    #[serde(rename = "s")]
    pub state: String,
    /// Where to go afterwards (already checked to be a local path).
    #[serde(rename = "n")]
    pub next: String,
    /// The signed-in account that is linking this provider, if this is a link.
    #[serde(rename = "l", default, skip_serializing_if = "Option::is_none")]
    pub link_account: Option<i64>,
    /// Unix seconds.
    #[serde(rename = "t")]
    pub issued_at: i64,
}

impl Flow {
    /// A new flow with a fresh random `state`.
    pub fn new(provider: &str, next: &str, link_account: Option<i64>, now: i64) -> Self {
        Self {
            provider: provider.to_owned(),
            state: URL_SAFE_NO_PAD.encode(random_bytes()),
            next: next.to_owned(),
            link_account,
            issued_at: now,
        }
    }

    /// Whether the `state` the provider sent back is this flow's, in constant time.
    pub fn state_matches(&self, returned: &str) -> bool {
        self.state.len() == returned.len() && bool::from(self.state.as_bytes().ct_eq(returned.as_bytes()))
    }
}

/// The server key that seals flow cookies and derives per-flow secrets.
#[derive(Clone)]
pub struct FlowKey([u8; 32]);

impl fmt::Debug for FlowKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FlowKey(<redacted>)")
    }
}

impl FlowKey {
    /// `None` for keys shorter than 32 bytes. Longer keys are compressed with SHA-256.
    pub fn new(secret: &[u8]) -> Option<Self> {
        (secret.len() >= MIN_KEY_LEN).then(|| Self(Sha256::digest(secret).into()))
    }

    /// HMAC-SHA256 over `label`, a NUL and `data`: each purpose gets its own label, so
    /// a value made for one can never be replayed as another.
    fn mac(&self, label: &str, data: &[u8]) -> [u8; 32] {
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(&self.0).unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
        mac.update(label.as_bytes());
        mac.update(&[0]);
        mac.update(data);
        mac.finalize().into_bytes().into()
    }

    /// The cookie value: `v1.<payload>.<mac>`, both base64url.
    pub fn seal(&self, flow: &Flow) -> String {
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(flow).unwrap_or_default());
        let mac = URL_SAFE_NO_PAD.encode(self.mac("rbit/oauth-flow/v1", payload.as_bytes()));
        format!("{FLOW_VERSION}.{payload}.{mac}")
    }

    /// The flow sealed in `value` if the MAC checks out and it is at most `max_age`
    /// seconds old (and not from the future beyond the clock leeway).
    pub fn open(&self, value: &str, now: i64, max_age: i64) -> Option<Flow> {
        let rest = value.strip_prefix(FLOW_VERSION)?.strip_prefix('.')?;
        let (payload, mac) = rest.split_once('.')?;
        let mac = URL_SAFE_NO_PAD.decode(mac).ok()?;
        let expected = self.mac("rbit/oauth-flow/v1", payload.as_bytes());
        if !bool::from(expected.as_slice().ct_eq(&mac)) {
            return None;
        }
        let flow: Flow = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
        let age = now - flow.issued_at;
        ((-CLOCK_LEEWAY_SECONDS..=max_age).contains(&age)).then_some(flow)
    }

    /// The flow's PKCE verifier, derived from its `state`.
    pub fn pkce(&self, flow: &Flow) -> Pkce {
        Pkce::from_bytes(&self.mac("rbit/oauth-pkce/v1", flow.state.as_bytes()))
    }

    /// The flow's OIDC `nonce`, derived from its `state`.
    pub fn nonce(&self, flow: &Flow) -> String {
        URL_SAFE_NO_PAD.encode(self.mac("rbit/oauth-nonce/v1", flow.state.as_bytes()))
    }

    /// A token for `/login/{provider}?link=…` bound to one session (by the digest of its
    /// token), so another site cannot start a link on a signed-in user's behalf.
    pub fn link_token(&self, session_digest: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(&self.mac("rbit/oauth-link/v1", session_digest)[..16])
    }

    /// Checks a [`Self::link_token`] in constant time.
    pub fn check_link_token(&self, session_digest: &[u8], token: &str) -> bool {
        let expected = self.link_token(session_digest);
        expected.len() == token.len() && bool::from(expected.as_bytes().ct_eq(token.as_bytes()))
    }
}

// ---------------------------------------------------------------------- keys

/// A provider's signing keys (its JWKS document), keeping the supported ones.
#[derive(Clone, Default)]
pub struct JwkSet {
    keys: Vec<Jwk>,
}

impl fmt::Debug for JwkSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.keys.iter().map(|key| (&key.kid, key.key.algorithm()))).finish()
    }
}

#[derive(Clone)]
struct Jwk {
    kid: Option<String>,
    /// The key's `alg`, when the provider pins one.
    alg: Option<String>,
    key: PublicKey,
}

#[derive(Clone)]
enum PublicKey {
    Rs256(Box<rsa::pkcs1v15::VerifyingKey<Sha256>>),
    Es256(p256::ecdsa::VerifyingKey),
}

impl PublicKey {
    const fn algorithm(&self) -> &'static str {
        match self {
            Self::Rs256(_) => "RS256",
            Self::Es256(_) => "ES256",
        }
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        match self {
            Self::Rs256(key) => {
                rsa::pkcs1v15::Signature::try_from(signature).is_ok_and(|signature| key.verify(message, &signature).is_ok())
            }
            // JWS carries the raw `r || s` (RFC 7518 §3.4), not DER.
            Self::Es256(key) => {
                p256::ecdsa::Signature::from_slice(signature).is_ok_and(|signature| key.verify(message, &signature.normalize_s()).is_ok())
            }
        }
    }
}

#[derive(Deserialize)]
struct JwkSetJson {
    keys: Vec<Value>,
}

impl JwkSet {
    /// Parses a JWKS document. Keys of other types, curves or uses (encryption keys,
    /// `oct` keys, RSA keys under 2048 bits) are skipped; a document without a single
    /// usable key is an error.
    pub fn parse(json: &[u8]) -> Result<Self, OidcError> {
        let document: JwkSetJson = serde_json::from_slice(json).map_err(|_| OidcError::Malformed("key set is not a JWKS document"))?;
        let keys: Vec<Jwk> = document.keys.iter().filter_map(parse_jwk).collect();
        if keys.is_empty() {
            return Err(OidcError::Malformed("key set has no RS256 or ES256 signing key"));
        }
        Ok(Self { keys })
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The keys a token signed with `alg` (and `kid`, if it names one) may be checked against.
    fn candidates<'a>(&'a self, alg: &'a str, kid: Option<&'a str>) -> impl Iterator<Item = &'a Jwk> + 'a {
        self.keys.iter().filter(move |key| {
            key.key.algorithm() == alg
                && key.alg.as_deref().is_none_or(|pinned| pinned == alg)
                && kid.is_none_or(|kid| key.kid.as_deref() == Some(kid))
        })
    }
}

fn parse_jwk(value: &Value) -> Option<Jwk> {
    let text = |name: &str| value.get(name).and_then(Value::as_str);
    if text("use").is_some_and(|usage| usage != "sig") {
        return None;
    }
    let bytes = |name: &str| text(name).and_then(|encoded| URL_SAFE_NO_PAD.decode(encoded.trim_end_matches('=')).ok());
    let key = match text("kty")? {
        "RSA" => {
            let n = rsa::BoxedUint::from_be_slice_vartime(&bytes("n")?);
            let e = rsa::BoxedUint::from_be_slice_vartime(&bytes("e")?);
            let key = rsa::RsaPublicKey::new(n, e).ok()?;
            if rsa::traits::PublicKeyParts::size(&key) < 256 {
                return None;
            }
            PublicKey::Rs256(Box::new(rsa::pkcs1v15::VerifyingKey::new(key)))
        }
        "EC" if text("crv") == Some("P-256") => {
            let (x, y) = (bytes("x")?, bytes("y")?);
            if x.len() != 32 || y.len() != 32 {
                return None;
            }
            let point = [&[0x04], x.as_slice(), y.as_slice()].concat();
            PublicKey::Es256(p256::ecdsa::VerifyingKey::from_sec1_bytes(&point).ok()?)
        }
        _ => return None,
    };
    Some(Jwk { kid: text("kid").map(str::to_owned), alg: text("alg").map(str::to_owned), key })
}

// ---------------------------------------------------------------------- ID tokens

/// What the ID token must say.
#[derive(Debug, Clone, Copy)]
pub struct IdTokenCheck<'a> {
    /// The issuer from discovery (compared exactly).
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub nonce: &'a str,
    /// Unix seconds.
    pub now: i64,
}

/// The verified claims rbit uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdToken {
    pub subject: String,
    pub email: Option<String>,
    /// `email_verified` was `true` (or the string `"true"`).
    pub email_verified: bool,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
    /// Authentication methods (`amr`), e.g. `["pwd", "mfa"]`.
    pub amr: Vec<String>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

/// The parts of a compact JWS.
struct Parts<'a> {
    header: Header,
    signed: &'a str,
    payload: Vec<u8>,
    signature: Vec<u8>,
}

fn split(token: &str) -> Result<Parts<'_>, OidcError> {
    if token.len() > MAX_TOKEN_LEN {
        return Err(OidcError::Malformed("token too long"));
    }
    let mut pieces = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) = (pieces.next(), pieces.next(), pieces.next(), pieces.next()) else {
        return Err(OidcError::Malformed("not a compact JWS"));
    };
    let decode = |part: &str| URL_SAFE_NO_PAD.decode(part).map_err(|_| OidcError::Malformed("bad base64url"));
    let header: Header = serde_json::from_slice(&decode(header)?).map_err(|_| OidcError::Malformed("bad header"))?;
    let signed = &token[..token.len() - signature.len() - 1];
    Ok(Parts { header, signed, payload: decode(payload)?, signature: decode(signature)? })
}

/// The `kid` the token names, so the caller can tell whether its cached keys are stale.
pub fn token_key_id(token: &str) -> Result<Option<String>, OidcError> {
    Ok(split(token)?.header.kid)
}

/// Verifies the signature and claims of an ID token; see the module docs.
pub fn verify_id_token(token: &str, keys: &JwkSet, check: &IdTokenCheck<'_>) -> Result<IdToken, OidcError> {
    let parts = split(token)?;
    let alg = parts.header.alg.as_str();
    if !matches!(alg, "RS256" | "ES256") {
        return Err(OidcError::UnsupportedAlgorithm(parts.header.alg.clone()));
    }
    let mut candidates = keys.candidates(alg, parts.header.kid.as_deref()).peekable();
    if candidates.peek().is_none() {
        return Err(OidcError::UnknownKey);
    }
    if !candidates.any(|key| key.key.verify(parts.signed.as_bytes(), &parts.signature)) {
        return Err(OidcError::BadSignature);
    }
    let claims: Value = serde_json::from_slice(&parts.payload).map_err(|_| OidcError::Malformed("claims are not JSON"))?;
    check_claims(&claims, check)
}

fn check_claims(claims: &Value, check: &IdTokenCheck<'_>) -> Result<IdToken, OidcError> {
    let text = |name: &str| claims.get(name).and_then(Value::as_str);
    let number = |name: &str| claims.get(name).and_then(Value::as_i64);
    if text("iss") != Some(check.issuer) {
        return Err(OidcError::Claim("iss"));
    }
    let audiences: Vec<&str> = match claims.get("aud") {
        Some(Value::String(audience)) => vec![audience.as_str()],
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !audiences.contains(&check.client_id) {
        return Err(OidcError::Claim("aud"));
    }
    // OIDC Core §3.1.3.7: with several audiences, `azp` must name us.
    if (audiences.len() > 1 || claims.get("azp").is_some()) && text("azp") != Some(check.client_id) {
        return Err(OidcError::Claim("azp"));
    }
    let exp = number("exp").ok_or(OidcError::Claim("exp"))?;
    if exp + CLOCK_LEEWAY_SECONDS <= check.now {
        return Err(OidcError::Claim("exp"));
    }
    let iat = number("iat").ok_or(OidcError::Claim("iat"))?;
    if iat > check.now + CLOCK_LEEWAY_SECONDS {
        return Err(OidcError::Claim("iat"));
    }
    if number("nbf").is_some_and(|nbf| nbf > check.now + CLOCK_LEEWAY_SECONDS) {
        return Err(OidcError::Claim("nbf"));
    }
    let nonce = text("nonce").unwrap_or_default();
    if nonce.len() != check.nonce.len() || !bool::from(nonce.as_bytes().ct_eq(check.nonce.as_bytes())) {
        return Err(OidcError::Claim("nonce"));
    }
    let subject = text("sub").filter(|sub| !sub.is_empty() && sub.len() <= MAX_SUBJECT_LEN).ok_or(OidcError::Claim("sub"))?;
    let email_verified = match claims.get("email_verified") {
        Some(Value::Bool(verified)) => *verified,
        // Some providers send it as text.
        Some(Value::String(verified)) => verified.eq_ignore_ascii_case("true"),
        _ => false,
    };
    let amr = claims.get("amr").and_then(Value::as_array).map(|list| list.iter().filter_map(Value::as_str).map(str::to_owned).collect());
    Ok(IdToken {
        subject: subject.to_owned(),
        email: text("email").map(str::to_owned),
        email_verified,
        name: text("name").map(str::to_owned),
        preferred_username: text("preferred_username").map(str::to_owned),
        amr: amr.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::{SigningKey, signature::Signer as _};
    use serde_json::json;

    use super::*;

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
    fn pkce_matches_the_rfc_example() {
        // RFC 7636 appendix B.
        let pkce = Pkce("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned());
        assert_eq!(pkce.challenge(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        assert_eq!(Pkce::METHOD, "S256");
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
}
