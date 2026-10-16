//! Runtime configuration. Loaded by the server binary from defaults, an optional TOML
//! file, and `RUSTIT_*` environment variables (later sources win).

use std::{net::SocketAddr, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub security: SecurityConfig,
    pub repository: RepositoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Address the HTTP server binds to.
    pub bind: SocketAddr,
    /// Public base URL, used for clone URLs and the CSRF origin check.
    pub public_url: String,
    /// Site name shown in the header and the PWA manifest.
    pub site_name: String,
    /// Allow anyone to create an account from the sign-up page.
    pub open_registration: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 3000)),
            public_url: "http://localhost:3000".to_owned(),
            site_name: "Rustit".to_owned(),
            open_registration: true,
        }
    }
}

impl ServerConfig {
    /// Cookies get the `Secure` attribute whenever the site is served over HTTPS.
    pub fn is_https(&self) -> bool {
        self.public_url.starts_with("https://")
    }

    /// The public URL without a trailing slash.
    pub fn base_url(&self) -> &str {
        self.public_url.trim_end_matches('/')
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Root directory for the SQLite database, the redb store and bare repositories.
    pub data_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { data_dir: PathBuf::from("data") }
    }
}

impl StorageConfig {
    pub fn sqlite_path(&self) -> PathBuf {
        self.data_dir.join("rustit.db")
    }

    pub fn kv_path(&self) -> PathBuf {
        self.data_dir.join("rustit.redb")
    }

    pub fn repositories_dir(&self) -> PathBuf {
        self.data_dir.join("repositories")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// How long a session stays valid without activity, in days.
    pub session_lifetime_days: u32,
    /// Minimum password length accepted at sign-up.
    pub min_password_length: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self { session_lifetime_days: 30, min_password_length: 10 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RepositoryConfig {
    /// Branch that `HEAD` points to in newly created repositories.
    pub default_branch: String,
    /// Path to the `git` executable used by the smart-HTTP transport.
    pub git_binary: PathBuf,
    /// Files larger than this are not rendered inline in the browser.
    pub max_render_bytes: usize,
}

impl Default for RepositoryConfig {
    fn default() -> Self {
        Self { default_branch: "main".to_owned(), git_binary: PathBuf::from("git"), max_render_bytes: 1024 * 1024 }
    }
}
