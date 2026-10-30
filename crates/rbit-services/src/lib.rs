//! Shared forge services composed with optional runtime feature implementations.

#[cfg(feature = "actions")]
pub use rbit_actions::{ActionsService, actions};
use rbit_core::{Config, User};
use rbit_plugin_runtime::PluginStatus;
pub use rbit_services_base::*;
use rbit_storage::{Database, KvStore};
use std::{ops::Deref, sync::Arc};
mod actions_plugin;
pub use actions_plugin::ActionsSlot;

/// Composition root. Core services never depend on optional plugin implementations.
#[derive(Debug, Clone)]
pub struct Services {
    core: Arc<rbit_services_base::Services>,
    pub actions: ActionsSlot,
}

impl Deref for Services {
    type Target = rbit_services_base::Services;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl Services {
    pub async fn new(config: Arc<Config>, database: &Database, kv: &KvStore) -> Result<Self, ServiceError> {
        // Feature preparation has large state; keep it out of every caller's async frame.
        let core = Arc::new(Box::pin(rbit_services_base::Services::new(config, database, kv)).await?);
        let actions = Box::pin(ActionsSlot::new(core.context().clone())).await?;
        Ok(Self { core, actions })
    }
    pub fn spawn_background(&self) {
        self.core.spawn_background();
        self.actions.spawn_background();
    }
    pub fn actions_background_tasks(&self) -> usize {
        self.actions.background_tasks()
    }
    /// Whether this distribution contains the optional Actions implementation.
    #[allow(clippy::unused_self)] // The host-facing capability query is uniform across builds.
    pub const fn actions_available(&self) -> bool {
        cfg!(feature = "actions")
    }
    /// Revision used to invalidate feature-dependent browser navigation snapshots.
    pub fn plugin_revision(&self) -> u64 {
        self.actions.revision()
    }
    pub fn actions_status(&self) -> std::future::Ready<Option<PluginStatus>> {
        std::future::ready(self.actions.status())
    }
    /// Persists administrator intent before changing the live feature.
    pub async fn set_actions_enabled(&self, actor: &User, enabled: bool) -> Result<(), ServiceError> {
        if !actor.is_admin {
            return Err(ServiceError::Forbidden);
        }
        self.actions.set_enabled(actor, enabled, self.core.audit.clone()).await
    }
    pub async fn shutdown(&self) {
        self.actions.shutdown().await;
        self.core.shutdown().await;
    }
}
