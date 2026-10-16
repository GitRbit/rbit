//! `[ssh]`: the built-in SSH server for Git over SSH.
//!
//! Milestone 1 workstream H (SSH) owns this file.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SshConfig {
    /// Run the built-in SSH server.
    pub enabled: bool,
    /// Address the SSH server binds to.
    pub bind: SocketAddr,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self { enabled: false, bind: SocketAddr::from(([127, 0, 0, 1], 2222)) }
    }
}
