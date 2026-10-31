//! Shared application state handed to every handler.

use std::{ops::Deref, sync::Arc};

use rbit_core::Config;
use rbit_services::Services;
use rbit_transport::ProcessTransport;
use tokio::sync::watch;

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
    pub restart: Restart,
}

impl AppState {
    pub fn new(config: Arc<Config>, services: Services) -> Self {
        let transport = ProcessTransport::new(config.repository.git_binary.clone()).with_max_push_bytes(config.repository.max_push_bytes());
        Self(Arc::new(AppStateInner { config, services, transport, highlighter: Highlighter::new(), restart: Restart::new() }))
    }
}

impl Deref for AppState {
    type Target = AppStateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// An administrator's request to restart the server. The binary's serve loop waits on it
/// alongside the operating system's shutdown signals, shuts down, then starts again.
#[derive(Debug, Clone)]
pub struct Restart(Arc<watch::Sender<bool>>);

impl Restart {
    pub fn new() -> Self {
        Self(Arc::new(watch::Sender::new(false)))
    }

    pub fn request(&self) {
        self.0.send_replace(true);
    }

    /// True once a restart was asked for. A restarted server starts over at false, so a
    /// page polling for it can tell the new process from the old one.
    pub fn is_requested(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves once a restart is requested.
    pub async fn requested(&self) {
        let mut receiver = self.0.subscribe();
        // The sender lives in `self`, so the channel cannot close while this waits.
        let _ = receiver.wait_for(|requested| *requested).await;
    }
}

impl Default for Restart {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn waiters_see_a_restart_requested_before_or_after_they_start_waiting() {
        let restart = Restart::new();
        assert!(!restart.is_requested());
        let waiting = tokio::spawn({
            let restart = restart.clone();
            async move { restart.requested().await }
        });
        restart.request();
        waiting.await.unwrap();
        restart.requested().await;
        assert!(restart.is_requested());
    }
}
