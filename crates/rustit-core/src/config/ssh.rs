//! `[ssh]`: the built-in SSH server for Git over SSH.
//!
//! ```toml
//! [ssh]
//! enabled = true
//! bind = "0.0.0.0:2222"
//! # What clone URLs show; defaults to the host of `server.public_url` and the bind port.
//! public_host = "git.example.com"
//! public_port = 22
//! # Defaults to <data_dir>/ssh/host_ed25519, generated on first start.
//! # host_key_path = "/etc/rustit/ssh_host_ed25519_key"
//! ```
//!
//! Milestone 1 workstream H (SSH) owns this file.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use super::{ServerConfig, StorageConfig};

/// The SSH user name in clone URLs ([`SshConfig::USER`]).
const SSH_USER: &str = "git";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SshConfig {
    /// Run the built-in SSH server.
    pub enabled: bool,
    /// Address the SSH server binds to.
    pub bind: SocketAddr,
    /// Host name shown in SSH clone URLs. Defaults to the host of `server.public_url`.
    pub public_host: Option<String>,
    /// Port shown in SSH clone URLs, when a proxy or port mapping differs from `bind`.
    pub public_port: Option<u16>,
    /// The Ed25519 host key in OpenSSH format. Defaults to `<data_dir>/ssh/host_ed25519`;
    /// generated on first start if missing.
    pub host_key_path: Option<PathBuf>,
    /// Concurrent connections across all clients; further connections are closed at once.
    pub max_connections: usize,
    /// Concurrent connections from one IP address.
    pub max_connections_per_ip: usize,
    /// Seconds a client has to authenticate after connecting.
    pub auth_timeout_seconds: u64,
    /// Seconds without any traffic before a connection is closed.
    pub idle_timeout_seconds: u64,
    /// Upper bound, in seconds, on a single Git operation (a very large clone or push).
    pub max_session_seconds: u64,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: SocketAddr::from(([127, 0, 0, 1], 2222)),
            public_host: None,
            public_port: None,
            host_key_path: None,
            max_connections: 128,
            max_connections_per_ip: 16,
            auth_timeout_seconds: 30,
            idle_timeout_seconds: 300,
            max_session_seconds: 6 * 60 * 60,
        }
    }
}

impl SshConfig {
    /// The SSH user name in clone URLs. Any name is accepted; the key identifies the user.
    pub const USER: &str = SSH_USER;

    /// Where the host key lives.
    pub fn host_key_path(&self, storage: &StorageConfig) -> PathBuf {
        self.host_key_path.clone().unwrap_or_else(|| storage.data_dir.join("ssh").join("host_ed25519"))
    }

    /// The host clients connect to: `public_host`, or the host of `server.public_url`.
    pub fn public_host(&self, server: &ServerConfig) -> String {
        match self.public_host.as_deref().map(str::trim) {
            Some(host) if !host.is_empty() => host.to_owned(),
            _ => url_host(&server.public_url).unwrap_or("localhost").to_owned(),
        }
    }

    /// The port clients connect to: `public_port`, or the bound port.
    pub fn public_port(&self) -> u16 {
        self.public_port.unwrap_or(self.bind.port())
    }

    /// The SSH clone URL of `owner/repo`: the short scp-like form on port 22
    /// (`git@host:owner/repo.git`), an `ssh://` URL otherwise.
    pub fn clone_url(&self, server: &ServerConfig, full_name: &str) -> String {
        let host = self.public_host(server);
        let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host };
        match self.public_port() {
            22 => format!("{SSH_USER}@{host}:{full_name}.git"),
            port => format!("ssh://{SSH_USER}@{host}:{port}/{full_name}.git"),
        }
    }

    pub fn auth_timeout(&self) -> Duration {
        Duration::from_secs(self.auth_timeout_seconds.max(1))
    }

    pub fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.idle_timeout_seconds.max(1))
    }

    pub fn max_session(&self) -> Duration {
        Duration::from_secs(self.max_session_seconds.max(1))
    }
}

/// The host part of `scheme://[user@]host[:port][/path]`, without brackets for IPv6.
fn url_host(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host =
        if let Some(bracketed) = authority.strip_prefix('[') { bracketed.split_once(']')?.0 } else { authority.split(':').next()? };
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(public_url: &str) -> ServerConfig {
        ServerConfig { public_url: public_url.to_owned(), ..ServerConfig::default() }
    }

    #[test]
    fn extracts_the_host_of_a_url() {
        assert_eq!(url_host("https://git.example.com/"), Some("git.example.com"));
        assert_eq!(url_host("http://localhost:3000"), Some("localhost"));
        assert_eq!(url_host("http://user@host.test:80/sub/path"), Some("host.test"));
        assert_eq!(url_host("http://[::1]:3000"), Some("::1"));
        assert_eq!(url_host("http://"), None);
    }

    #[test]
    fn clone_urls_use_the_short_form_on_port_22() {
        let mut ssh = SshConfig { public_port: Some(22), ..SshConfig::default() };
        assert_eq!(ssh.clone_url(&server("https://git.example.com"), "alice/proj"), "git@git.example.com:alice/proj.git");

        ssh.public_host = Some("ssh.example.com".to_owned());
        assert_eq!(ssh.clone_url(&server("https://git.example.com"), "alice/proj"), "git@ssh.example.com:alice/proj.git");
    }

    #[test]
    fn clone_urls_spell_out_other_ports() {
        let ssh = SshConfig::default();
        assert_eq!(ssh.clone_url(&server("http://localhost:3000"), "alice/proj"), "ssh://git@localhost:2222/alice/proj.git");

        let ipv6 = SshConfig { public_host: Some("::1".to_owned()), ..SshConfig::default() };
        assert_eq!(ipv6.clone_url(&server("http://localhost:3000"), "a/b"), "ssh://git@[::1]:2222/a/b.git");
    }

    #[test]
    fn host_key_defaults_to_the_data_directory() {
        let storage = StorageConfig { data_dir: PathBuf::from("data") };
        assert_eq!(SshConfig::default().host_key_path(&storage), PathBuf::from("data").join("ssh").join("host_ed25519"));
        let custom = SshConfig { host_key_path: Some(PathBuf::from("/etc/key")), ..SshConfig::default() };
        assert_eq!(custom.host_key_path(&storage), PathBuf::from("/etc/key"));
    }
}
