//! Short-lived signed tokens for the LFS API.
//!
//! Clients are handed an `Authorization` header twice: per object in a batch response
//! (scoped to one object, one operation and its size) and by `git-lfs-authenticate` over
//! SSH (scoped to one repository and operation). The header carries
//! `Bearer lfs1.<payload>.<mac>`: the payload says what the token allows, the MAC is
//! HMAC-SHA256 under a key drawn from the OS at start-up. Nothing is stored, so tokens
//! end when they expire or when the server restarts (clients then authenticate again).
//!
//! Payload layout (little-endian, base64url without padding):
//!
//! ```text
//! version u8 = 1 | repo i64 | subject u8 (0 anonymous, 1 user, 2 deploy key) | subject id i64
//! | write u8 | expires_at i64 (Unix seconds) | object u8 (0/1) [| oid [u8; 32] | size u64]
//! ```

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use rbit_core::{AccountId, RepoId, Timestamp, lfs::LfsOid};
use rbit_storage::sqlite::ssh_keys::DeployKeyId;
use sha2::Sha256;

/// Marks rbit LFS tokens (and their format version).
const PREFIX: &str = "lfs1.";
const VERSION: u8 = 1;
/// Separates the MAC input from anything else signed with the key.
const DOMAIN: &[u8] = b"rbit-lfs-token-v1\0";

/// Who a token acts for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSubject {
    /// Downloads from a public repository without credentials.
    Anonymous,
    User(AccountId),
    /// A repository deploy key over SSH.
    DeployKey(DeployKeyId),
}

/// What a valid token allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsGrant {
    pub repo: RepoId,
    pub subject: TokenSubject,
    /// Uploads (and locks) as well as downloads.
    pub write: bool,
    /// For a transfer token: the one object, and its size, it may move.
    pub object: Option<(LfsOid, u64)>,
}

/// The signing key, random per process.
#[derive(Clone)]
pub(crate) struct TokenKey([u8; 32]);

impl std::fmt::Debug for TokenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenKey(..)")
    }
}

impl TokenKey {
    pub(crate) fn random() -> Self {
        let mut key = [0u8; 32];
        // The OS random source failing is unrecoverable; refuse to run with a weak key.
        getrandom::fill(&mut key).expect("the operating system's random number generator is unavailable");
        Self(key)
    }

    fn mac(&self) -> Hmac<Sha256> {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&self.0).expect("HMAC accepts keys of any length");
        mac.update(DOMAIN);
        mac
    }

    /// The token for `grant`, valid until `expires_at`.
    pub(crate) fn sign(&self, grant: &LfsGrant, expires_at: Timestamp) -> String {
        let payload = encode(grant, expires_at);
        let mut mac = self.mac();
        mac.update(&payload);
        let tag = mac.finalize().into_bytes();
        format!("{PREFIX}{}.{}", URL_SAFE_NO_PAD.encode(&payload), URL_SAFE_NO_PAD.encode(tag))
    }

    /// The grant of `token` if it was signed with this key and has not expired at `now`.
    /// The MAC is compared in constant time.
    pub(crate) fn verify(&self, token: &str, now: Timestamp) -> Option<LfsGrant> {
        let (payload, tag) = token.strip_prefix(PREFIX)?.split_once('.')?;
        let payload = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let tag = URL_SAFE_NO_PAD.decode(tag).ok()?;
        let mut mac = self.mac();
        mac.update(&payload);
        mac.verify_slice(&tag).ok()?;
        let (grant, expires_at) = decode(&payload)?;
        (now < expires_at).then_some(grant)
    }
}

/// Whether a credential has the shape of an LFS token (and so is never tried as anything else).
pub fn looks_like_token(value: &str) -> bool {
    value.starts_with(PREFIX)
}

fn encode(grant: &LfsGrant, expires_at: Timestamp) -> Vec<u8> {
    let mut out = Vec::with_capacity(72);
    out.push(VERSION);
    out.extend_from_slice(&grant.repo.0.to_le_bytes());
    let (kind, id) = match grant.subject {
        TokenSubject::Anonymous => (0u8, 0i64),
        TokenSubject::User(id) => (1, id.0),
        TokenSubject::DeployKey(id) => (2, id.0),
    };
    out.push(kind);
    out.extend_from_slice(&id.to_le_bytes());
    out.push(u8::from(grant.write));
    out.extend_from_slice(&expires_at.seconds().to_le_bytes());
    match &grant.object {
        None => out.push(0),
        Some((oid, size)) => {
            out.push(1);
            out.extend_from_slice(&digest_of(oid));
            out.extend_from_slice(&size.to_le_bytes());
        }
    }
    out
}

fn decode(payload: &[u8]) -> Option<(LfsGrant, Timestamp)> {
    let mut reader = Reader(payload);
    if reader.byte()? != VERSION {
        return None;
    }
    let repo = RepoId(i64::from_le_bytes(reader.array()?));
    let kind = reader.byte()?;
    let id = i64::from_le_bytes(reader.array()?);
    let subject = match kind {
        0 => TokenSubject::Anonymous,
        1 => TokenSubject::User(AccountId(id)),
        2 => TokenSubject::DeployKey(DeployKeyId(id)),
        _ => return None,
    };
    let write = match reader.byte()? {
        0 => false,
        1 => true,
        _ => return None,
    };
    let expires_at = Timestamp(i64::from_le_bytes(reader.array()?));
    let object = match reader.byte()? {
        0 => None,
        1 => {
            let oid = LfsOid::from_digest(reader.array()?);
            Some((oid, u64::from_le_bytes(reader.array()?)))
        }
        _ => return None,
    };
    reader.0.is_empty().then_some((LfsGrant { repo, subject, write, object }, expires_at))
}

/// The 32 bytes an object id spells in hex.
fn digest_of(oid: &LfsOid) -> [u8; 32] {
    let mut digest = [0u8; 32];
    for (byte, pair) in digest.iter_mut().zip(oid.as_str().as_bytes().as_chunks::<2>().0) {
        let hex = |b: u8| if b.is_ascii_digit() { b - b'0' } else { b - b'a' + 10 };
        *byte = (hex(pair[0]) << 4) | hex(pair[1]);
    }
    digest
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let (&first, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(first)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(object: Option<(LfsOid, u64)>) -> LfsGrant {
        LfsGrant { repo: RepoId(7), subject: TokenSubject::User(AccountId(3)), write: true, object }
    }

    #[test]
    fn tokens_round_trip_until_they_expire() {
        let key = TokenKey::random();
        let oid = LfsOid::parse(&"0123456789abcdef".repeat(4)).unwrap();
        for grant in [
            grant(None),
            grant(Some((oid, u64::MAX))),
            LfsGrant { repo: RepoId(1), subject: TokenSubject::Anonymous, write: false, object: None },
            LfsGrant { repo: RepoId(1), subject: TokenSubject::DeployKey(DeployKeyId(9)), write: false, object: None },
        ] {
            let token = key.sign(&grant, Timestamp(100));
            assert!(looks_like_token(&token));
            assert_eq!(key.verify(&token, Timestamp(99)), Some(grant.clone()));
            assert_eq!(key.verify(&token, Timestamp(100)), None, "expired");
        }
    }

    #[test]
    fn tampered_or_foreign_tokens_are_refused() {
        let key = TokenKey::random();
        let token = key.sign(&grant(None), Timestamp(100));
        assert_eq!(TokenKey::random().verify(&token, Timestamp(1)), None, "another key");

        // Flip a write bit in the payload: the MAC no longer matches.
        let (payload, tag) = token.strip_prefix(PREFIX).unwrap().split_once('.').unwrap();
        let mut bytes = URL_SAFE_NO_PAD.decode(payload).unwrap();
        bytes[18] ^= 1;
        let forged = format!("{PREFIX}{}.{tag}", URL_SAFE_NO_PAD.encode(&bytes));
        assert_eq!(key.verify(&forged, Timestamp(1)), None);

        for garbage in ["", "lfs1.", "lfs1.x.y", "rbit_abc", &token.replace("lfs1.", "lfs2."), &format!("{token}A")] {
            assert_eq!(key.verify(garbage, Timestamp(1)), None, "{garbage:?}");
        }
    }

    #[test]
    fn decoding_rejects_trailing_or_unknown_fields() {
        let mut payload = encode(&grant(None), Timestamp(100));
        assert!(decode(&payload).is_some());
        payload.push(0);
        assert!(decode(&payload).is_none(), "trailing bytes");
        let mut bad_subject = encode(&grant(None), Timestamp(100));
        bad_subject[9] = 5;
        assert!(decode(&bad_subject).is_none());
    }
}
