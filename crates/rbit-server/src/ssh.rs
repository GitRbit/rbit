//! Starts the built-in SSH server when `[ssh] enabled = true`, and stops it on the same
//! shutdown signals as the HTTP server (Ctrl+C, or SIGTERM on Unix).
//!
//! Milestone 1 workstream H owns this file.

use std::sync::Arc;

use rbit_core::Config;
use rbit_kernel::TaskStop;
use rbit_services::Services;

/// Spawns the SSH server in the background. A failure to start (port in use, unreadable
/// host key) is logged; the web interface keeps running without SSH.
pub fn start(
    config: &Arc<Config>,
    services: &Services,
    restart: tokio::sync::watch::Receiver<bool>,
    stop: TaskStop,
) -> Option<tokio::task::JoinHandle<()>> {
    if !config.ssh.enabled {
        return None;
    }
    let config = Arc::clone(config);
    let services = services.clone();
    Some(tokio::spawn(async move {
        let server = match rbit_ssh::start(config, services).await {
            Ok(server) => server,
            Err(error) => {
                tracing::error!(%error, "the SSH server could not start; Git over SSH is unavailable");
                return;
            }
        };
        shutdown_signal(restart, stop).await;
        server.shutdown().await;
        tracing::info!("SSH server stopped");
    }))
}

/// Resolves on Ctrl+C, or SIGTERM on Unix, like the HTTP server's shutdown signal.
async fn shutdown_signal(mut restart: tokio::sync::watch::Receiver<bool>, stop: TaskStop) {
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
        () = stop.cancelled() => {},
        () = ctrl_c => {},
        () = terminate => {},
        _ = restart.wait_for(|requested| *requested) => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_cancellation_stops_ssh_without_a_signal_or_restart() {
        let owner = rbit_kernel::TaskLifecycle::new();
        let (_restart, receiver) = tokio::sync::watch::channel(false);
        owner.request_stop();
        tokio::time::timeout(std::time::Duration::from_secs(1), shutdown_signal(receiver, owner.stop_signal())).await.unwrap();
    }
}
