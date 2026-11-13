//! Provider signing-key selection and compact ID-token signature/claim validation.

use super::{CLOCK_LEEWAY_SECONDS, OidcError};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::signature::Verifier as _;
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use std::fmt;
use subtle::ConstantTimeEq;

/// Longest ID token accepted (real ones are 1–2 KiB).
const MAX_TOKEN_LEN: usize = 16 * 1024;
/// Longest `sub` accepted (OIDC allows 255 ASCII characters).
const MAX_SUBJECT_LEN: usize = 255;

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
#[derive(Clone, Copy)]
pub struct IdTokenCheck<'a> {
    /// The issuer from discovery (compared exactly).
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub nonce: &'a str,
    /// Unix seconds.
    pub now: i64,
}

/// The verified claims rbit uses.
#[derive(Clone, PartialEq, Eq)]
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
        Some(Value::Array(list)) => list.iter().map(Value::as_str).collect::<Option<Vec<_>>>().ok_or(OidcError::Claim("aud"))?,
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
    if i128::from(exp) + i128::from(CLOCK_LEEWAY_SECONDS) <= i128::from(check.now) {
        return Err(OidcError::Claim("exp"));
    }
    let iat = number("iat").ok_or(OidcError::Claim("iat"))?;
    let latest = i128::from(check.now) + i128::from(CLOCK_LEEWAY_SECONDS);
    if i128::from(iat) > latest {
        return Err(OidcError::Claim("iat"));
    }
    if claims.get("nbf").is_some() {
        let nbf = number("nbf").ok_or(OidcError::Claim("nbf"))?;
        if i128::from(nbf) > latest {
            return Err(OidcError::Claim("nbf"));
        }
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

impl fmt::Debug for IdTokenCheck<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("IdTokenCheck").finish_non_exhaustive()
    }
}

impl fmt::Debug for IdToken {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("IdToken").finish_non_exhaustive()
    }
}
