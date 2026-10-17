//! # rbit-ssh
//!
//! The built-in SSH server for Git over SSH, built on russh.
//!
//! * **Host key** ([`host_key`]): Ed25519, loaded from `<data_dir>/ssh/host_ed25519` (or
//!   `[ssh] host_key_path`), generated with private permissions on first start.
//! * **Authentication**: public keys only. A key's SHA256 fingerprint resolves to a user
//!   (refused while suspended) or a repository deploy key; failures are throttled per
//!   client address (`ThrottleScope::SshAuth`). The SSH user name is ignored.
//! * **Commands** ([`command`]): only `git-upload-pack` / `git-receive-pack` with a
//!   strictly parsed `owner/repo[.git]` path, and Git LFS's `git-lfs-authenticate`
//!   (a signed header for the HTTP LFS API). No shell is ever started, no PTY, X11,
//!   agent or port forwarding is granted; `ssh git@host` just prints a greeting.
//! * **Authorisation**: identical to Git over HTTP. User keys go through
//!   `RepositoryService::open`; deploy keys reach only their repository, read-only
//!   unless they were allowed to write. The repository path on disk always comes from
//!   the database (`RepoContext::disk_path`), never from the command.
//! * **Transport**: git runs in interactive mode (`rbit-transport`), streaming in both
//!   directions; a successful push records the push and publishes
//!   `DomainEvent::RepositoryPushed`.
//! * **Bounds**: global and per-address connection limits, an authentication deadline,
//!   an idle timeout, a limit on channels per connection and on a single operation's
//!   duration.
//!
//! Start it with [`start`]; stop it with [`RunningSshServer::shutdown`].

pub mod command;
pub mod host_key;
mod server;
mod session;

use std::net::SocketAddr;

pub use host_key::HostKeyError;
pub use server::{RunningSshServer, start};

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error(transparent)]
    HostKey(#[from] HostKeyError),
    #[error("cannot listen for SSH on {address}: {source}")]
    Bind {
        address: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("background task failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}
