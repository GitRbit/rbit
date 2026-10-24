//! Runtime configuration. Loaded by the server binary from defaults, an optional TOML
//! file, and `RBIT_*` environment variables (later sources win).
//!
//! Each `[section]` of the TOML file is one struct. Larger areas live in their own
//! submodule so they can evolve independently.

mod api;
mod branding;
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
mod updates;
mod webhooks;

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

pub use api::ApiConfig;
pub use auth::{AuthConfig, MAX_PROVIDER_ID_LEN, ProviderConfig, ProviderKind, ProviderRegistration};
pub use branding::BrandingConfig;
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
pub use updates::UpdateConfig;
pub use webhooks::WebhookConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub updates: UpdateConfig,
    pub server: ServerConfig,
    pub branding: BrandingConfig,
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
#[serde(default, deny_unknown_fields)]
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
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    /// Root directory for the `SQLite` database, the redb store and bare repositories.
    pub data_dir: PathBuf,
    /// The key that seals credentials stored in the database (TOTP seeds, webhook and push
    /// mirror secrets, saved SMTP and OIDC secrets). Defaults to `<data_dir>/secret.key`,
    /// created on first start; set it to keep the key apart from the data, e.g. a mounted
    /// container secret. Back it up: the sealed values are unreadable without it.
    pub secret_key_file: Option<PathBuf>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { data_dir: PathBuf::from("data"), secret_key_file: None }
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

    /// The secret key file: `secret_key_file`, or `secret.key` in the data directory.
    pub fn secret_key_path(&self) -> PathBuf {
        self.secret_key_file.clone().unwrap_or_else(|| self.data_dir.join("secret.key"))
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
#[serde(default, deny_unknown_fields)]
pub struct RepositoryConfig {
    /// Branch that `HEAD` points to in newly created repositories.
    pub default_branch: String,
    /// Path to the `git` executable used by the smart-HTTP transport.
    pub git_binary: PathBuf,
    /// Files larger than this are not rendered inline in the browser.
    pub max_render_bytes: usize,
    /// Git transport processes (clones, fetches and pushes over HTTP and SSH) running at
    /// once across the instance; further requests are refused (`503` over HTTP).
    pub max_git_processes: usize,
    /// Concurrent smart-HTTP Git requests from one IP address (`429` beyond). Behind a
    /// reverse proxy this needs `[security] trusted_proxies`, or every client shares one.
    pub max_git_requests_per_ip: usize,
    /// Seconds a smart-HTTP Git exchange may go without traffic in either direction.
    pub git_idle_timeout_seconds: u64,
    /// Upper bound, in seconds, on a single smart-HTTP Git exchange (a very large clone
    /// or push).
    pub git_max_seconds: u64,
    /// Largest pack one push may send, in bytes (git's `receive.maxInputSize`), over HTTP
    /// and SSH. 0 means unlimited.
    pub max_push_bytes: u64,
    /// Source archive downloads one IP address may have running at once.
    pub max_archives_per_ip: usize,
    /// Seconds an archive download may stall (the client not reading, or git not writing).
    pub archive_idle_timeout_seconds: u64,
    /// Upper bound, in seconds, on one archive download.
    pub archive_max_seconds: u64,
}

impl Default for RepositoryConfig {
    fn default() -> Self {
        Self {
            default_branch: "main".to_owned(),
            git_binary: PathBuf::from("git"),
            max_render_bytes: 1024 * 1024,
            max_git_processes: 128,
            max_git_requests_per_ip: 16,
            git_idle_timeout_seconds: 300,
            git_max_seconds: 6 * 60 * 60,
            max_push_bytes: 2 * 1024 * 1024 * 1024,
            max_archives_per_ip: 2,
            archive_idle_timeout_seconds: 60,
            archive_max_seconds: 60 * 60,
        }
    }
}

impl RepositoryConfig {
    /// [`Self::max_git_processes`], at least one.
    pub fn max_git_processes(&self) -> usize {
        self.max_git_processes.max(1)
    }

    /// [`Self::max_git_requests_per_ip`], at least one.
    pub fn max_git_requests_per_ip(&self) -> usize {
        self.max_git_requests_per_ip.max(1)
    }

    pub fn git_idle_timeout(&self) -> Duration {
        Duration::from_secs(self.git_idle_timeout_seconds.max(1))
    }

    pub fn git_max_duration(&self) -> Duration {
        Duration::from_secs(self.git_max_seconds.max(1))
    }

    /// [`Self::max_push_bytes`], or `None` when pushes are unlimited.
    pub fn max_push_bytes(&self) -> Option<u64> {
        (self.max_push_bytes > 0).then_some(self.max_push_bytes)
    }

    /// [`Self::max_archives_per_ip`], at least one.
    pub fn max_archives_per_ip(&self) -> usize {
        self.max_archives_per_ip.max(1)
    }

    pub fn archive_idle_timeout(&self) -> Duration {
        Duration::from_secs(self.archive_idle_timeout_seconds.max(1))
    }

    pub fn archive_max_duration(&self) -> Duration {
        Duration::from_secs(self.archive_max_seconds.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_limits_have_floors() {
        let defaults = RepositoryConfig::default();
        assert_eq!(defaults.max_push_bytes(), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(defaults.git_idle_timeout(), Duration::from_secs(300));
        let zeroed = RepositoryConfig {
            max_git_processes: 0,
            max_git_requests_per_ip: 0,
            git_idle_timeout_seconds: 0,
            git_max_seconds: 0,
            max_push_bytes: 0,
            max_archives_per_ip: 0,
            archive_idle_timeout_seconds: 0,
            archive_max_seconds: 0,
            ..defaults
        };
        assert_eq!(zeroed.max_git_processes(), 1);
        assert_eq!(zeroed.max_git_requests_per_ip(), 1);
        assert_eq!(zeroed.git_idle_timeout(), Duration::from_secs(1));
        assert_eq!(zeroed.git_max_duration(), Duration::from_secs(1));
        assert_eq!(zeroed.max_push_bytes(), None, "0 means unlimited");
        assert_eq!(zeroed.max_archives_per_ip(), 1);
        assert_eq!(zeroed.archive_idle_timeout(), Duration::from_secs(1));
        assert_eq!(zeroed.archive_max_duration(), Duration::from_secs(1));
    }
}
