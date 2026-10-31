//! Shared application state handed to every handler.

#[cfg(any(feature = "i18n", test))]
use std::sync::Weak;
use std::{ops::Deref, sync::Arc};

use rbit_core::Config;
use rbit_services::Services;
use rbit_transport::ProcessTransport;

use crate::render::Highlighter;

/// Cheaply cloneable handle (one `Arc`) to everything handlers need.
#[derive(Debug, Clone)]
pub struct AppState(Arc<AppStateInner>);

/// Route blueprints keep weak host ownership so service registries cannot form
/// a cycle through their own registered callbacks.
#[derive(Debug, Clone)]
#[cfg(any(feature = "i18n", test))]
pub(crate) struct WeakAppState(Weak<AppStateInner>);

#[cfg(any(feature = "i18n", test))]
impl WeakAppState {
    pub(crate) fn upgrade(&self) -> Option<AppState> {
        self.0.upgrade().map(AppState)
    }
}

#[derive(Debug)]
pub struct AppStateInner {
    pub config: Arc<Config>,
    pub services: Services,
    pub transport: ProcessTransport,
    pub highlighter: Highlighter,
    pub monitoring: Arc<crate::monitoring::Monitor>,
    /// Host proof key; plugin callbacks never receive this key or session material.
    pub plugin_csrf: rbit_auth::PluginCsrfKey,
    pub(crate) plugin_routes_ready: tokio::sync::OnceCell<()>,
    /// The executable subscribes before serving; embedded hosts can leave this unsupported.
    pub restart: tokio::sync::watch::Sender<bool>,
}

impl AppState {
    pub fn new(config: Arc<Config>, services: Services) -> Self {
        let transport = ProcessTransport::new(config.repository.git_binary.clone()).with_max_push_bytes(config.repository.max_push_bytes());
        let (restart, _) = tokio::sync::watch::channel(false);
        Self(Arc::new(AppStateInner {
            config,
            services,
            transport,
            highlighter: Highlighter::new(),
            monitoring: crate::monitoring::Monitor::new(),
            plugin_csrf: rbit_auth::PluginCsrfKey::generate(),
            plugin_routes_ready: tokio::sync::OnceCell::new(),
            restart,
        }))
    }

    #[cfg(any(feature = "i18n", test))]
    pub(crate) fn downgrade(&self) -> WeakAppState {
        WeakAppState(Arc::downgrade(&self.0))
    }
}

impl Deref for AppState {
    type Target = AppStateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
