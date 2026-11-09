//! The secrets key: AES-256-GCM sealing bound to a secret's row, and its key file format.
use std::{fmt, io};

use aes_gcm::{
    Aes256Gcm, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::SecretAddress;

/// What a key file starts with; the version names the format and the cipher.
pub const KEY_PREFIX: &str = "rbit-actions-key-v1:";
/// What the associated data of every sealed value starts with.
const AAD_PREFIX: &[u8] = b"rbit-actions-secret-v1";

/// The secrets key, held in memory for the life of the process and zeroed on drop.
pub struct SecretsKey {
    key: Zeroizing<[u8; 32]>,
    id: [u8; 8],
}

impl fmt::Debug for SecretsKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretsKey").field("id", &self.id).finish_non_exhaustive()
    }
}

impl SecretsKey {
    /// The key made of these bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut id = [0; 8];
        id.copy_from_slice(&digest[..8]);
        Self { key: Zeroizing::new(bytes), id }
    }

    /// A key from fresh randomness.
    pub fn generate() -> io::Result<Self> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut bytes[..]).map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Self::from_bytes(*bytes))
    }

    /// A key file's contents; surrounding whitespace is ignored. `Err` says why not.
    pub fn parse(text: &str) -> Result<Self, String> {
        let body = text.trim().strip_prefix(KEY_PREFIX).ok_or_else(|| format!("it does not start with `{KEY_PREFIX}`"))?;
        let decoded = Zeroizing::new(STANDARD.decode(body).map_err(|_| "its key is not valid base64".to_owned())?);
        let bytes: [u8; 32] = decoded.as_slice().try_into().map_err(|_| "its key is not 32 bytes long".to_owned())?;
        Ok(Self::from_bytes(bytes))
    }

    /// The contents of a key file for this key.
    pub fn encode(&self) -> Zeroizing<String> {
        Zeroizing::new(format!("{KEY_PREFIX}{}\n", STANDARD.encode(&self.key[..])))
    }

    /// The first 8 bytes of SHA-256 of the key, stored with every value it seals.
    pub const fn id(&self) -> [u8; 8] {
        self.id
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new(&Key::<Aes256Gcm>::from(*self.key))
    }

    /// Seals `value` as the secret at `address`: a fresh nonce and the ciphertext with its
    /// tag, stored beside [`Self::id`]. `Err` only when the system has no randomness or the
    /// cipher fails.
    pub fn seal(&self, address: SecretAddress<'_>, value: &str) -> io::Result<([u8; 12], Vec<u8>)> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|error| io::Error::other(error.to_string()))?;
        let aad = associated_data(address);
        let ciphertext = self
            .cipher()
            .encrypt(&Nonce::from(nonce), Payload { msg: value.as_bytes(), aad: &aad })
            .map_err(|_| io::Error::other("sealing a secret failed"))?;
        Ok((nonce, ciphertext))
    }

    /// Opens the secret at `address` stored as `key_id`, `nonce` and `ciphertext`, or `None`
    /// when another key sealed it or the ciphertext belongs to another address.
    pub fn open(&self, address: SecretAddress<'_>, key_id: [u8; 8], nonce: [u8; 12], ciphertext: &[u8]) -> Option<Zeroizing<String>> {
        if key_id != self.id {
            return None;
        }
        let aad = associated_data(address);
        let plain = Zeroizing::new(self.cipher().decrypt(&Nonce::from(nonce), Payload { msg: ciphertext, aad: &aad }).ok()?);
        String::from_utf8(plain.to_vec()).ok().map(Zeroizing::new)
    }

    /// Whether this key sealed a secret with `key_id`.
    pub fn sealed(&self, key_id: [u8; 8]) -> bool {
        self.id == key_id
    }
}

/// `rbit-actions-secret-v1 \0 <scope kind> \0 <scope id> \0 <secret id>`: binds a ciphertext
/// to its row, so a value copied elsewhere does not open.
pub(super) fn associated_data(address: SecretAddress<'_>) -> Vec<u8> {
    let mut aad = AAD_PREFIX.to_vec();
    for part in [address.scope_kind.as_bytes(), address.scope_id.to_string().as_bytes(), address.secret_id.to_string().as_bytes()] {
        aad.push(0);
        aad.extend_from_slice(part);
    }
    aad
}
