//! The server's Ed25519 host key: loaded from disk, or generated on first start.
//!
//! Clients pin the host key the first time they connect, so it must survive restarts and
//! upgrades: it lives under the data directory (or `[ssh] host_key_path`) in OpenSSH
//! format, readable by `ssh-keygen -l -f`. New key files are created with mode `0600`
//! (and their directory `0700`) on Unix; on Windows they inherit the data directory's ACL,
//! which should already be private to the service account.

use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use russh::keys::{
    HashAlg, PrivateKey,
    ssh_key::{Algorithm, LineEnding, private::Ed25519Keypair},
};

#[derive(Debug, thiserror::Error)]
pub enum HostKeyError {
    #[error("cannot read or write the SSH host key {path}: {source}")]
    Io { path: String, source: io::Error },
    #[error("the SSH host key {path} is not a valid OpenSSH private key: {detail}")]
    Invalid { path: String, detail: String },
    #[error("the SSH host key {0} is passphrase-protected; the server cannot unlock it")]
    Encrypted(String),
    #[error("the SSH host key {path} is {algorithm}; an Ed25519 key is required")]
    WrongAlgorithm { path: String, algorithm: String },
    #[error("cannot generate an SSH host key: {0}")]
    Random(String),
}

/// Loads the host key at `path`, generating and saving a new one if the file is missing.
pub fn load_or_generate(path: &Path) -> Result<PrivateKey, HostKeyError> {
    match fs::read_to_string(path) {
        Ok(contents) => parse(path, &contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => generate(path),
        Err(source) => Err(io_error(path, source)),
    }
}

fn parse(path: &Path, contents: &str) -> Result<PrivateKey, HostKeyError> {
    let key = PrivateKey::from_openssh(contents)
        .map_err(|error| HostKeyError::Invalid { path: path.display().to_string(), detail: error.to_string() })?;
    if key.is_encrypted() {
        return Err(HostKeyError::Encrypted(path.display().to_string()));
    }
    if key.algorithm() != Algorithm::Ed25519 {
        return Err(HostKeyError::WrongAlgorithm { path: path.display().to_string(), algorithm: key.algorithm().to_string() });
    }
    warn_if_readable_by_others(path);
    Ok(key)
}

fn generate(path: &Path) -> Result<PrivateKey, HostKeyError> {
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|error| HostKeyError::Random(error.to_string()))?;
    let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
    key.set_comment("rustit host key");
    let encoded = key.to_openssh(LineEnding::LF).map_err(|error| HostKeyError::Random(error.to_string()))?;

    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        create_private_dir(parent).map_err(|source| io_error(parent, source))?;
    }
    match create_private_file(path) {
        Ok(mut file) => {
            file.write_all(encoded.as_bytes()).and_then(|()| file.sync_all()).map_err(|source| io_error(path, source))?;
        }
        // Another process won the race; use its key so every instance agrees.
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let contents = fs::read_to_string(path).map_err(|source| io_error(path, source))?;
            return parse(path, &contents);
        }
        Err(source) => return Err(io_error(path, source)),
    }

    tracing::info!(path = %path.display(), fingerprint = %key.public_key().fingerprint(HashAlg::Sha256), "generated a new SSH host key");
    Ok(key)
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
        tracing::warn!(path = %path.display(), "the SSH host key is accessible to other users; run `chmod 600` on it");
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &Path) {}

fn io_error(path: &Path, source: io::Error) -> HostKeyError {
    HostKeyError::Io { path: path.display().to_string(), source }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_once_then_reloads_the_same_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ssh").join("host_ed25519");

        let generated = load_or_generate(&path).unwrap();
        assert_eq!(generated.algorithm(), Algorithm::Ed25519);
        assert!(fs::read_to_string(&path).unwrap().starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));

        let reloaded = load_or_generate(&path).unwrap();
        assert_eq!(reloaded.public_key().key_data(), generated.public_key().key_data());
    }

    #[cfg(unix)]
    #[test]
    fn new_keys_are_private_to_the_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ssh").join("host_ed25519");
        load_or_generate(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn refuses_files_that_are_not_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host_ed25519");
        fs::write(&path, "not a key").unwrap();
        assert!(matches!(load_or_generate(&path), Err(HostKeyError::Invalid { .. })));
    }
}
