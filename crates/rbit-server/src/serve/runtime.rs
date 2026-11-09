//! The server's process runtime behind the web layer's [`RuntimeControl`] port. The serve
//! loop subscribes to the restart signal before accepting connections and drains HTTP and
//! SSH when a restart is requested; this side answers the administration pages.

use std::sync::Arc;

use rbit_core::Config;
use rbit_services::Services;
use rbit_web::{
    AppState,
    monitoring::{Monitor, Snapshot},
    runtime::RuntimeControl,
};
use tokio::sync::watch;

#[derive(Debug)]
struct ServerRuntime {
    monitor: Arc<Monitor>,
    restart: Arc<watch::Sender<bool>>,
}

impl RuntimeControl for ServerRuntime {
    fn monitoring(&self) -> Snapshot {
        self.monitor.snapshot()
    }

    /// Served while the serve loop (and so the restart supervisor) is listening.
    fn restart_supported(&self) -> bool {
        self.restart.receiver_count() > 0
    }

    fn restarting(&self) -> bool {
        *self.restart.borrow()
    }

    fn request_restart(&self) {
        // One pending restart: repeated requests join it rather than queueing another.
        self.restart.send_if_modified(|pending| !std::mem::replace(pending, true));
    }
}

/// The web state with the server's runtime installed.
pub(crate) fn state(config: Arc<Config>, services: Services) -> AppState {
    AppState::with_runtime(config, services, |monitor, restart| Arc::new(ServerRuntime { monitor, restart }))
}
