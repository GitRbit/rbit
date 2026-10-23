//! At-rest encryption ("sealing") of credentials the server must be able to read back:
//! TOTP seeds, webhook and push mirror secrets, the server's own signing keys and the
//! secret fields of the administrator-saved configuration.
//!
//! Values are sealed with AES-256-GCM under a 256-bit key kept *outside* the database, in
//! a key file (`<data_dir>/secret.key` by default, `[storage] secret_key_file` to move
//! it). A copy of the database alone (a stolen backup, a misplaced file) then reveals no
//! usable secret. A sealed value is text:
//!
//! ```text
//! enc:v1:<base64(nonce || ciphertext || tag)>
//! ```
//!
//! Anything without the prefix is a value stored before sealing existed and is returned
//! as is; [`crate::Database::open`] re-seals such rows at start-up.

use std::{
    fmt, fs,
    io::{self, Write},
    path::Path,
    sync::Arc,
};

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Key, Nonce},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

/// The key file's name inside the data directory, unless configured elsewhere.
pub const KEY_FILE_NAME: &str = "secret.key";
/// Marks a sealed value; the version allows a future change of algorithm or key.
pub const PREFIX: &str = "enc:v1:";

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("cannot use the secret key file {path}: {source}")]
    Io { path: String, source: io::Error },
    #[error("the secret key file {path} must hold 32 bytes in base64")]
    InvalidKey { path: String },
    /// The key is not the one the value was sealed with, or the value was altered.
    #[error("a sealed value cannot be opened: wrong secret key or corrupt data")]
    Open,
    #[error("no randomness available: {0}")]
    Random(String),
}

/// The key that seals and opens stored secrets. Cheap to clone.
#[derive(Clone)]
pub struct SealingKey(Arc<Aes256Gcm>);

impl fmt::Debug for SealingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SealingKey(..)")
    }
}

impl SealingKey {
    pub fn from_bytes(bytes: &[u8; KEY_LEN]) -> Self {
        Self(Arc::new(Aes256Gcm::new(&Key::<Aes256Gcm>::from(*bytes))))
    }

    /// A fresh random key, never written anywhere (tests).
    pub fn generate() -> Result<Self, SealError> {
        Ok(Self::from_bytes(&random()?))
    }

    /// Reads the key file at `path`. `None` if it does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>, SealError> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(io_error(path, source)),
        };
        warn_if_readable_by_others(path);
        parse(path, &contents).map(Some)
    }

    /// Writes a new random key file at `path`, readable only by its owner (on Unix). If
    /// another process created it first, its key is used instead, so every instance agrees.
    pub fn create(path: &Path) -> Result<Self, SealError> {
        let bytes = random()?;
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            create_private_dir(parent).map_err(|source| io_error(parent, source))?;
        }
        match create_private_file(path) {
            Ok(mut file) => {
                let encoded = format!("{}\n", STANDARD.encode(bytes));
                file.write_all(encoded.as_bytes()).and_then(|()| file.sync_all()).map_err(|source| io_error(path, source))?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let contents = fs::read_to_string(path).map_err(|source| io_error(path, source))?;
                return parse(path, &contents);
            }
            Err(source) => return Err(io_error(path, source)),
        }
        tracing::info!(path = %path.display(), "generated a new secret key; back it up together with the database");
        Ok(Self::from_bytes(&bytes))
    }

    /// Seals `plaintext` with a fresh random nonce.
    pub fn seal(&self, plaintext: &[u8]) -> Result<String, SealError> {
        let nonce: [u8; NONCE_LEN] = random()?;
        let ciphertext = self.0.encrypt(&Nonce::<Aes256Gcm>::from(nonce), plaintext).map_err(|_| SealError::Open)?;
        let mut sealed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);
        Ok(format!("{PREFIX}{}", STANDARD.encode(sealed)))
    }

    /// The plaintext of a stored value: opened if sealed, as stored otherwise (legacy).
    pub fn open(&self, stored: &[u8]) -> Result<Vec<u8>, SealError> {
        let Some(encoded) = stored.strip_prefix(PREFIX.as_bytes()) else {
            return Ok(stored.to_vec());
        };
        let sealed = STANDARD.decode(encoded).map_err(|_| SealError::Open)?;
        if sealed.len() < NONCE_LEN {
            return Err(SealError::Open);
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| SealError::Open)?;
        self.0.decrypt(&Nonce::<Aes256Gcm>::from(nonce), ciphertext).map_err(|_| SealError::Open)
    }

    /// [`Self::open`] for values that are text.
    pub fn open_str(&self, stored: &str) -> Result<String, SealError> {
        String::from_utf8(self.open(stored.as_bytes())?).map_err(|_| SealError::Open)
    }

    /// Seals every non-empty string under a [secret field](is_secret_field) of a JSON
    /// document, at any depth; sealed ones are kept. `true` if anything changed.
    pub fn seal_secret_fields(&self, value: &mut Value) -> Result<bool, SealError> {
        let mut changed = false;
        match value {
            Value::Object(map) => {
                for (name, value) in map {
                    match value {
                        Value::String(text) if is_secret_field(name) && !text.is_empty() && !is_sealed(text.as_bytes()) => {
                            *text = self.seal(text.as_bytes())?;
                            changed = true;
                        }
                        _ => changed |= self.seal_secret_fields(value)?,
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    changed |= self.seal_secret_fields(value)?;
                }
            }
            _ => {}
        }
        Ok(changed)
    }

    /// Reverses [`Self::seal_secret_fields`]; plaintext (legacy) values stay as they are.
    pub fn open_secret_fields(&self, value: &mut Value) -> Result<(), SealError> {
        match value {
            Value::Object(map) => {
                for (name, value) in map {
                    match value {
                        Value::String(text) if is_secret_field(name) => *text = self.open_str(text)?,
                        _ => self.open_secret_fields(value)?,
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.open_secret_fields(value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Whether a configuration field holds a credential, judged by its name
/// (`smtp_password`, `client_secret`, `metrics_token`, …).
pub fn is_secret_field(name: &str) -> bool {
    name.ends_with("password") || name.ends_with("secret") || name.ends_with("token")
}

/// Whether a stored value is sealed (rather than legacy plaintext).
pub fn is_sealed(stored: &[u8]) -> bool {
    stored.starts_with(PREFIX.as_bytes())
}

fn random<const N: usize>() -> Result<[u8; N], SealError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|error| SealError::Random(error.to_string()))?;
    Ok(bytes)
}

fn parse(path: &Path, contents: &str) -> Result<SealingKey, SealError> {
    let invalid = || SealError::InvalidKey { path: path.display().to_string() };
    let bytes = STANDARD.decode(contents.trim()).map_err(|_| invalid())?;
    let bytes: [u8; KEY_LEN] = bytes.try_into().map_err(|_| invalid())?;
    Ok(SealingKey::from_bytes(&bytes))
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if dir.exists() {
        return Ok(());
    }
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path)
        && metadata.permissions().mode() & 0o077 != 0
    {
        tracing::warn!(path = %path.display(), "the secret key file is accessible to other users; run `chmod 600` on it");
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &Path) {}

fn io_error(path: &Path, source: io::Error) -> SealError {
    SealError::Io { path: path.display().to_string(), source }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_values_round_trip_and_never_repeat() {
        let key = SealingKey::generate().unwrap();
        let first = key.seal(b"hunter2").unwrap();
        let second = key.seal(b"hunter2").unwrap();
        assert!(first.starts_with(PREFIX) && is_sealed(first.as_bytes()));
        assert!(!first.contains("hunter2"));
        assert_ne!(first, second, "every value gets its own nonce");
        assert_eq!(key.open(first.as_bytes()).unwrap(), b"hunter2");
        assert_eq!(key.open_str(&second).unwrap(), "hunter2");
        assert_eq!(key.open_str(&key.seal(b"").unwrap()).unwrap(), "");
    }

    #[test]
    fn legacy_plaintext_is_returned_as_stored() {
        let key = SealingKey::generate().unwrap();
        assert!(!is_sealed(b"plain"));
        assert_eq!(key.open(b"plain").unwrap(), b"plain");
        assert_eq!(key.open(&[0, 159, 255]).unwrap(), [0, 159, 255]);
        assert_eq!(key.open_str("").unwrap(), "");
    }

    #[test]
    fn a_different_key_cannot_open() {
        let sealed = SealingKey::generate().unwrap().seal(b"secret").unwrap();
        let other = SealingKey::generate().unwrap();
        assert!(matches!(other.open(sealed.as_bytes()), Err(SealError::Open)));
    }

    #[test]
    fn tampering_is_detected() {
        let key = SealingKey::generate().unwrap();
        let sealed = key.seal(b"secret").unwrap();
        let mut bytes = STANDARD.decode(&sealed[PREFIX.len()..]).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let tampered = format!("{PREFIX}{}", STANDARD.encode(&bytes));
        assert!(matches!(key.open(tampered.as_bytes()), Err(SealError::Open)));
        assert!(matches!(key.open(format!("{PREFIX}not base64!").as_bytes()), Err(SealError::Open)));
        assert!(matches!(key.open(format!("{PREFIX}AAAA").as_bytes()), Err(SealError::Open)));
    }

    #[test]
    fn only_secret_fields_of_documents_are_sealed() {
        let key = SealingKey::generate().unwrap();
        let original = serde_json::json!({
            "mail": {"smtp_password": "smtp-value", "smtp_host": "mail.example.com", "smtp_port": 587},
            "auth": {"providers": [{"id": "corp", "client_secret": "oidc-value"}, {"id": "none", "client_secret": ""}]},
            "operations": {"metrics_token": "metrics-value"},
        });
        let mut document = original.clone();
        assert!(key.seal_secret_fields(&mut document).unwrap());
        let text = document.to_string();
        assert!(!text.contains("smtp-value") && !text.contains("oidc-value") && !text.contains("metrics-value"));
        assert_eq!(document["mail"]["smtp_host"], "mail.example.com");
        assert_eq!(document["auth"]["providers"][1]["client_secret"], "", "empty values stay empty");
        assert!(!key.seal_secret_fields(&mut document).unwrap(), "sealing twice changes nothing");

        key.open_secret_fields(&mut document).unwrap();
        assert_eq!(document, original);
        let mut legacy = original.clone();
        key.open_secret_fields(&mut legacy).unwrap();
        assert_eq!(legacy, original, "plaintext documents open unchanged");
    }

    #[test]
    fn the_key_file_is_created_once_then_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join(KEY_FILE_NAME);
        assert!(SealingKey::load(&path).unwrap().is_none());
        let created = SealingKey::create(&path).unwrap();
        let sealed = created.seal(b"secret").unwrap();
        let loaded = SealingKey::load(&path).unwrap().unwrap();
        assert_eq!(loaded.open(sealed.as_bytes()).unwrap(), b"secret");
        // A second creator (a racing process) adopts the existing key.
        assert_eq!(SealingKey::create(&path).unwrap().open(sealed.as_bytes()).unwrap(), b"secret");
    }

    #[test]
    fn malformed_key_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(KEY_FILE_NAME);
        fs::write(&path, "too short").unwrap();
        assert!(matches!(SealingKey::load(&path), Err(SealError::InvalidKey { .. })));
        fs::write(&path, STANDARD.encode([7; 16])).unwrap();
        assert!(matches!(SealingKey::load(&path), Err(SealError::InvalidKey { .. })));
    }

    #[cfg(unix)]
    #[test]
    fn new_key_files_are_private_to_the_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join(KEY_FILE_NAME);
        SealingKey::create(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    }
}
