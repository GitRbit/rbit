//! Starts the built-in SSH server when `[ssh] enabled = true`, and stops it on the same
//! shutdown signals as the HTTP server (Ctrl+C, SIGTERM on Unix, or a restart).
//!
//! Milestone 1 workstream H owns this file.

use std::sync::Arc;

use rbit_core::Config;
use rbit_services::Services;
use rbit_web::Restart;

use crate::serve::shutdown_signal;

/// Spawns the SSH server in the background. A failure to start (port in use, unreadable
/// host key) is logged; the web interface keeps running without SSH.
pub fn start(config: &Arc<Config>, services: &Services, restart: &Restart) {
    if !config.ssh.enabled {
        return;
    }
    let config = Arc::clone(config);
    let services = services.clone();
    let restart = restart.clone();
    tokio::spawn(async move {
        let server = match rbit_ssh::start(config, services).await {
            Ok(server) => server,
            Err(error) => {
                tracing::error!(%error, "the SSH server could not start; Git over SSH is unavailable");
                return;
            }
        };
        shutdown_signal(restart).await;
        server.shutdown().await;
        tracing::info!("SSH server stopped");
    });
}
