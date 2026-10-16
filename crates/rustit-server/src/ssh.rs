//! Starts the built-in SSH server when `[ssh] enabled = true`, and stops it on the same
//! shutdown signals as the HTTP server (Ctrl+C, or SIGTERM on Unix).
//!
//! Milestone 1 workstream H owns this file.

use std::sync::Arc;

use rustit_core::Config;
use rustit_services::Services;

/// Spawns the SSH server in the background. A failure to start (port in use, unreadable
/// host key) is logged; the web interface keeps running without SSH.
pub fn start(config: &Arc<Config>, services: &Services) {
    if !config.ssh.enabled {
        return;
    }
    let config = Arc::clone(config);
    let services = services.clone();
    tokio::spawn(async move {
        let server = match rustit_ssh::start(config, services).await {
            Ok(server) => server,
            Err(error) => {
                tracing::error!(%error, "the SSH server could not start; Git over SSH is unavailable");
                return;
            }
        };
        shutdown_signal().await;
        server.shutdown().await;
        tracing::info!("SSH server stopped");
    });
}

/// Resolves on Ctrl+C, or SIGTERM on Unix, like the HTTP server's shutdown signal.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
