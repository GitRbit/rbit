//! Starts the built-in SSH server when `[ssh] enabled = true`.
//!
//! Milestone 1 workstream H owns this file.

use std::sync::Arc;

use rustit_core::Config;
use rustit_services::Services;

pub fn start(config: &Arc<Config>, services: &Services) {
    let _ = services;
    if config.ssh.enabled {
        tracing::warn!("the SSH server is not available in this build yet");
    }
}
