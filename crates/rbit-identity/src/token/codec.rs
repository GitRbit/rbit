//! Shared primitives for bearer secrets (session cookies, access tokens).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// 256 bits: far beyond brute-force range, so a fast hash is enough for storage.
pub const SECRET_BYTES: usize = 32;

/// The public half of a secret: the key under which it is stored.
pub use super::TokenDigest;

pub fn random_bytes() -> [u8; SECRET_BYTES] {
    let mut bytes = [0; SECRET_BYTES];
    getrandom::fill(&mut bytes).expect("operating system random number generator is unavailable");
    bytes
}

pub fn digest(bytes: &[u8]) -> TokenDigest {
    Sha256::digest(bytes).into()
}

pub fn encode(bytes: &[u8; SECRET_BYTES]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes exactly [`SECRET_BYTES`] bytes of base64url, rejecting anything else.
pub fn decode(encoded: &str) -> Option<[u8; SECRET_BYTES]> {
    let mut bytes = [0; SECRET_BYTES];
    let written = URL_SAFE_NO_PAD.decode_slice(encoded, &mut bytes).ok()?;
    (written == SECRET_BYTES).then_some(bytes)
}
