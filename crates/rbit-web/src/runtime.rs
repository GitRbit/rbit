//! The process-runtime port: monitoring, and restarts. The web layer asks; the host
//! decides. [`AppState::new`](crate::AppState::new) installs [`EmbeddedRuntime`], which
//! supports a restart only while something subscribes to the state's restart signal;
//! the `rbit` server installs its own implementation and owns restart orchestration.

use std::sync::Arc;

use tokio::sync::watch;

use crate::monitoring::{Monitor, Snapshot};

/// What the administration pages may ask of the process that hosts them.
pub trait RuntimeControl: Send + Sync + std::fmt::Debug {
    /// Recent request telemetry for the dashboard and the monitoring panel.
    fn monitoring(&self) -> Snapshot;
    /// Whether a restart request would be served.
    fn restart_supported(&self) -> bool;
    /// Whether a restart has been requested.
    fn restarting(&self) -> bool;
    /// Requests a restart, coalescing repeated requests into the one pending restart.
    fn request_restart(&self);
}

/// The runtime of a host that embeds the router without a restart supervisor.
#[derive(Debug)]
pub struct EmbeddedRuntime {
    monitor: Arc<Monitor>,
    restart: Arc<watch::Sender<bool>>,
}

impl EmbeddedRuntime {
    pub fn new(monitor: Arc<Monitor>, restart: Arc<watch::Sender<bool>>) -> Self {
        Self { monitor, restart }
    }
}

impl RuntimeControl for EmbeddedRuntime {
    fn monitoring(&self) -> Snapshot {
        self.monitor.snapshot()
    }

    fn restart_supported(&self) -> bool {
        self.restart.receiver_count() > 0
    }

    fn restarting(&self) -> bool {
        *self.restart.borrow()
    }

    fn request_restart(&self) {
        // Coalesce repeat clicks into the same shutdown, rather than queueing restarts.
        self.restart.send_if_modified(|pending| {
            let changed = !*pending;
            *pending = true;
            changed
        });
    }
}
