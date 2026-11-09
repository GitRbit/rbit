//! Shared application state handed to every handler.

use std::{ops::Deref, sync::Arc};

use rbit_core::Config;
use rbit_services::Services;
use rbit_transport::ProcessTransport;

use crate::{
    monitoring::Monitor,
    render::{Highlighter, document_jobs::DocumentJobs},
    runtime::{EmbeddedRuntime, RuntimeControl},
};

/// Cheaply cloneable handle (one `Arc`) to everything handlers need.
#[derive(Debug, Clone)]
pub struct AppState(Arc<AppStateInner>);

#[derive(Debug)]
pub struct AppStateInner {
    pub config: Arc<Config>,
    pub services: Services,
    pub transport: ProcessTransport,
    pub highlighter: Highlighter,
    /// Admission for rendering untrusted documents on request paths.
    pub documents: DocumentJobs,
    pub monitoring: Arc<crate::monitoring::Monitor>,
    /// The executable subscribes before serving; embedded hosts can leave this unsupported.
    pub restart: Arc<tokio::sync::watch::Sender<bool>>,
    /// Monitoring and restarts, as the host provides them.
    pub runtime: Arc<dyn RuntimeControl>,
}

impl AppState {
    /// State for an embedding host: [`EmbeddedRuntime`] serves monitoring and restarts.
    pub fn new(config: Arc<Config>, services: Services) -> Self {
        Self::with_runtime(config, services, |monitor, restart| Arc::new(EmbeddedRuntime::new(monitor, restart)))
    }

    /// State whose runtime the host builds from the shared monitor and restart signal.
    pub fn with_runtime(
        config: Arc<Config>,
        services: Services,
        runtime: impl FnOnce(Arc<Monitor>, Arc<tokio::sync::watch::Sender<bool>>) -> Arc<dyn RuntimeControl>,
    ) -> Self {
        let transport = ProcessTransport::new(config.repository.git_binary.clone()).with_max_push_bytes(config.repository.max_push_bytes());
        let restart = Arc::new(tokio::sync::watch::channel(false).0);
        let monitoring = Monitor::new();
        let runtime = runtime(monitoring.clone(), restart.clone());
        Self(Arc::new(AppStateInner {
            config,
            services,
            transport,
            highlighter: Highlighter::new(),
            documents: DocumentJobs::default(),
            monitoring,
            restart,
            runtime,
        }))
    }
}

impl Deref for AppState {
    type Target = AppStateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
