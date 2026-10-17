//! Runtime configuration. Loaded by the server binary from defaults, an optional TOML
//! file, and `RBIT_*` environment variables (later sources win).
//!
//! Each `[section]` of the TOML file is one struct. Larger areas live in their own
//! submodule so they can evolve independently.

mod api;
// Milestone 3: editing files in the browser (E).
mod auth;
mod editor;
// Milestone 3: importing and push mirrors (M).
mod imports;
mod issues;
mod lfs;
mod mail;
mod mirrors;
mod notifications;
mod operations;
mod pulls;
mod releases;
mod security;
mod ssh;
mod webhooks;

use std::{net::SocketAddr, path::PathBuf};

use serde::{Deserialize, Serialize};

pub use api::ApiConfig;
pub use auth::{AuthConfig, MAX_PROVIDER_ID_LEN, ProviderConfig, ProviderKind, ProviderRegistration};
pub use editor::EditorConfig;
// Milestone 3: importing and push mirrors (M).
pub use imports::{AuthorMapping, ImportConfig, PushMirrorConfig};
pub use issues::IssueConfig;
pub use lfs::LfsConfig;
pub use mail::{MailConfig, MailTransport, SmtpSecurity};
pub use mirrors::MirrorConfig;
pub use notifications::{EmailPreference, NotificationConfig};
pub use operations::OperationsConfig;
pub use pulls::PullConfig;
pub use releases::ReleaseConfig;
pub use security::{ProxyNetwork, ProxyNetworkError, SecurityConfig};
pub use ssh::SshConfig;
pub use webhooks::WebhookConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub security: SecurityConfig,
    pub repository: RepositoryConfig,
    pub mail: MailConfig,
    pub ssh: SshConfig,
    pub operations: OperationsConfig,
    pub api: ApiConfig,
    pub webhooks: WebhookConfig,
    pub releases: ReleaseConfig,
    pub mirrors: MirrorConfig,
    pub issues: IssueConfig,
    pub pulls: PullConfig,
    // Milestone 3: email notifications (N).
    pub notifications: NotificationConfig,
    // Milestone 3: editing files in the browser (E).
    pub editor: EditorConfig,
    // Milestone 3: Git LFS (L).
    pub lfs: LfsConfig,
    // Milestone 3: external sign-in (O).
    pub auth: AuthConfig,
    // Milestone 3: importing and push mirrors (M).
    pub imports: ImportConfig,
    pub push_mirrors: PushMirrorConfig,
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
            site_name: "rbit".to_owned(),
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
    /// `rbit.db`, or `rustit.db` in a data directory created before the rename to rbit.
    pub fn sqlite_path(&self) -> PathBuf {
        self.named_or_legacy("rbit.db", "rustit.db")
    }

    /// `rbit.redb`, or `rustit.redb` in a data directory created before the rename to rbit.
    pub fn kv_path(&self) -> PathBuf {
        self.named_or_legacy("rbit.redb", "rustit.redb")
    }

    fn named_or_legacy(&self, name: &str, legacy: &str) -> PathBuf {
        let path = self.data_dir.join(name);
        let legacy = self.data_dir.join(legacy);
        if !path.exists() && legacy.exists() { legacy } else { path }
    }

    pub fn repositories_dir(&self) -> PathBuf {
        self.data_dir.join("repositories")
    }

    /// Files attached to releases: `<attachments_dir>/<repo_id>/<asset_id>`.
    pub fn attachments_dir(&self) -> PathBuf {
        self.data_dir.join("attachments")
    }

    /// Scratch space for merges and object overlays (`<tmp_dir>/merges`, `<tmp_dir>/overlays`).
    /// Its contents are disposable: swept at start-up and never backed up.
    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
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
