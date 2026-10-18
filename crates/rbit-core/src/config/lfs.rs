//! `[lfs]`: Git Large File Storage.
//!
//! ```toml
//! [lfs]
//! # Serve the Git LFS API (batch, transfers, locks) and `git-lfs-authenticate` over SSH.
//! # When off, those endpoints answer 404 and the SSH command is refused.
//! enabled = true
//! # Largest object a client may upload, in bytes.
//! max_object_bytes = 2147483648
//! # How long the signed headers handed to clients (batch actions and
//! # `git-lfs-authenticate`) stay valid.
//! token_lifetime_seconds = 3600
//! ```
//!
//! Objects are stored under `<data_dir>/lfs` (see [`StorageConfig::lfs_dir`]).
//!
//! Milestone 3 workstream L (Git LFS) owns this file.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use super::StorageConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LfsConfig {
    /// Serve the LFS API and the SSH `git-lfs-authenticate` command.
    pub enabled: bool,
    /// Largest object, in bytes.
    pub max_object_bytes: u64,
    /// Lifetime of the signed transfer headers, in seconds.
    pub token_lifetime_seconds: u64,
}

impl Default for LfsConfig {
    fn default() -> Self {
        Self { enabled: true, max_object_bytes: 2 * 1024 * 1024 * 1024, token_lifetime_seconds: 60 * 60 }
    }
}

impl LfsConfig {
    /// [`Self::token_lifetime_seconds`], at least one second.
    pub fn token_lifetime(&self) -> Duration {
        Duration::from_secs(self.token_lifetime_seconds.max(1))
    }
}

impl StorageConfig {
    /// LFS objects: `<lfs_dir>/<aa>/<bb>/<oid>`, named by their SHA-256 digest.
    pub fn lfs_dir(&self) -> PathBuf {
        self.data_dir.join("lfs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_lifetime() {
        let config = LfsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.token_lifetime(), Duration::from_secs(3600));
        assert_eq!(LfsConfig { token_lifetime_seconds: 0, ..config }.token_lifetime(), Duration::from_secs(1));
        let storage = StorageConfig { data_dir: PathBuf::from("data") };
        assert_eq!(storage.lfs_dir(), PathBuf::from("data").join("lfs"));
    }
}
