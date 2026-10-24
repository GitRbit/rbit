//! Shared application state handed to every handler.

use std::{ops::Deref, sync::Arc};

use rbit_core::Config;
use rbit_services::Services;
use rbit_transport::ProcessTransport;

use crate::render::Highlighter;

/// Cheaply cloneable handle (one `Arc`) to everything handlers need.
#[derive(Debug, Clone)]
pub struct AppState(Arc<AppStateInner>);

#[derive(Debug)]
pub struct AppStateInner {
    pub config: Arc<Config>,
    pub services: Services,
    pub transport: ProcessTransport,
    pub highlighter: Highlighter,
}

impl AppState {
    pub fn new(config: Arc<Config>, services: Services) -> Self {
        let transport = ProcessTransport::new(config.repository.git_binary.clone()).with_max_push_bytes(config.repository.max_push_bytes());
        Self(Arc::new(AppStateInner { config, services, transport, highlighter: Highlighter::new() }))
    }
}

impl Deref for AppState {
    type Target = AppStateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
