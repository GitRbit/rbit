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
mod i18n_plugin;
pub use i18n_plugin::{I18nSlot, LanguagePackInfo};
pub mod ui;
pub use rbit_plugin_web as request_hooks;

/// Composition root. Core services never depend on optional plugin implementations.
#[derive(Debug, Clone)]
pub struct Services {
    core: Arc<rbit_services_base::Services>,
    pub actions: ActionsSlot,
    pub i18n: I18nSlot,
    ui: rbit_plugin_runtime::UiRegistry,
    request_hooks: request_hooks::RequestHookRegistry,
}

impl Deref for Services {
    type Target = rbit_services_base::Services;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl Services {
    pub async fn new(config: Arc<Config>, database: &Database, kv: &KvStore) -> Result<Self, ServiceError> {
        config.i18n.validate().map_err(ServiceError::Invalid)?;
        // Feature preparation has large state; keep it out of every caller's async frame.
        let core = Arc::new(Box::pin(rbit_services_base::Services::new(config, database, kv)).await?);
        let ui = rbit_plugin_runtime::UiRegistry::default();
        let actions = Box::pin(ActionsSlot::new(core.context().clone(), ui.clone())).await?;
        let i18n = Box::pin(I18nSlot::new(core.context().clone(), ui.clone())).await?;
        Ok(Self { core, actions, i18n, ui, request_hooks: request_hooks::RequestHookRegistry::new() })
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
        self.actions
            .revision()
            .saturating_add(self.i18n.revision())
            .saturating_add(self.ui.snapshot().revision)
            .saturating_add(self.request_hooks.revision())
    }
    /// Immutable presentation data; snapshots never retain plugin authority or leases.
    pub fn ui_snapshot(&self) -> ui::UiSnapshot {
        self.ui.snapshot()
    }
    /// Trusted host composition capability for registering scoped plugin UI.
    /// A registration still requires the plugin's current scope and generation.
    pub fn ui_registry(&self) -> rbit_plugin_runtime::UiRegistry {
        self.ui.clone()
    }
    /// Trusted host capability for registering bounded, generation-scoped request hooks.
    pub fn request_hooks(&self) -> request_hooks::RequestHookRegistry {
        self.request_hooks.clone()
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

    #[allow(clippy::unused_self)] // Uniform capability query in all distribution profiles.
    pub const fn i18n_available(&self) -> bool {
        cfg!(feature = "i18n")
    }

    pub fn i18n_enabled(&self) -> bool {
        self.i18n.is_enabled()
    }

    pub fn i18n_revision(&self) -> u64 {
        self.i18n.revision()
    }

    pub fn i18n_status(&self) -> std::future::Ready<Option<PluginStatus>> {
        std::future::ready(self.i18n.status())
    }

    /// Resolves one immutable catalog identity entirely from the in-memory publication.
    pub fn locale_snapshot(&self, account: Option<&str>, cookie: Option<&str>, accept: Option<&str>) -> rbit_messages::LocaleSnapshot {
        self.i18n.snapshot(account, cookie, accept)
    }

    pub fn available_locales(&self) -> Vec<String> {
        self.i18n.available_locales()
    }

    pub async fn set_i18n_enabled(&self, actor: &User, enabled: bool) -> Result<(), ServiceError> {
        if !actor.is_admin {
            return Err(ServiceError::Forbidden);
        }
        self.i18n.set_enabled(actor, enabled).await
    }

    /// Only administrators may replace text presented by trusted core UI components.
    pub async fn install_language_pack(&self, actor: &User, bytes: &[u8]) -> Result<LanguagePackInfo, ServiceError> {
        if !actor.is_admin {
            return Err(ServiceError::Forbidden);
        }
        self.i18n.install(actor, bytes).await
    }

    pub async fn remove_language_pack(&self, actor: &User, id: &str) -> Result<bool, ServiceError> {
        if !actor.is_admin {
            return Err(ServiceError::Forbidden);
        }
        self.i18n.remove(actor, id).await
    }

    pub async fn language_packs(&self, actor: &User) -> Result<Vec<LanguagePackInfo>, ServiceError> {
        if !actor.is_admin {
            return Err(ServiceError::Forbidden);
        }
        Ok(self.core.context().database.language_packs().metadata().await?)
    }

    /// Preferences outlive both the provider and individual installed translations.
    pub async fn set_account_locale(&self, actor: &User, locale: Option<&str>) -> Result<(), ServiceError> {
        if locale.is_some_and(|locale| !rbit_core::config::valid_locale_preference(locale)) {
            return Err(ServiceError::Invalid("locale must be a language tag of at most 64 ASCII characters".to_owned()));
        }
        if self.core.context().database.accounts().find_active_user(actor.id()).await?.is_none() {
            return Err(ServiceError::Forbidden);
        }
        Ok(self.core.context().database.language_packs().set_account_locale(actor.id(), locale).await?)
    }
    pub async fn shutdown(&self) {
        self.i18n.shutdown().await;
        self.actions.shutdown().await;
        self.core.shutdown().await;
    }
}
