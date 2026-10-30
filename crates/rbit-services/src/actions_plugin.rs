//! Runtime publication and persisted lifecycle intent for the optional Actions plugin.

use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::{ServiceContext, ServiceError};
use rbit_core::{Timestamp, User};
use rbit_plugin_runtime::PluginStatus;

const ACTIONS_ID: &str = "rbit.actions";

#[cfg(feature = "actions")]
use rbit_actions::ActionsService;
#[cfg(feature = "actions")]
use rbit_plugin_api::{Generation, PluginId, PluginManifest};
#[cfg(feature = "actions")]
use rbit_plugin_runtime::{Plugin, PluginFuture, PluginManager, PluginScope};
#[cfg(feature = "actions")]
use std::{sync::Mutex, time::Duration};

/// Shared active-service publication slot. Callers take owned snapshots so no lock or
/// borrowed service reference survives an asynchronous boundary.
#[derive(Debug, Clone)]
pub struct ActionsSlot {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    revision: AtomicU64,
    shutting_down: AtomicBool,
    status: RwLock<Option<PluginStatus>>,
    #[cfg(feature = "actions")]
    current: RwLock<Option<ActionsService>>,
    #[cfg(feature = "actions")]
    background_started: Arc<AtomicBool>,
    #[cfg(feature = "actions")]
    observed_scope: Arc<RwLock<Option<PluginScope>>>,
    #[cfg(feature = "actions")]
    desired_enabled: AtomicBool,
    #[cfg(feature = "actions")]
    controller: Arc<tokio::sync::Mutex<Controller>>,
}

#[cfg(feature = "actions")]
#[derive(Debug)]
struct Controller {
    manager: PluginManager,
    plugin: Arc<EmbeddedActions>,
}

#[cfg(feature = "actions")]
#[derive(Debug)]
struct EmbeddedActions {
    manifest: PluginManifest,
    ctx: ServiceContext,
    staged: Arc<Mutex<Option<ActionsService>>>,
    observed_scope: Arc<RwLock<Option<PluginScope>>>,
    cancel_on_stop: AtomicBool,
}

#[cfg(feature = "actions")]
impl EmbeddedActions {
    fn staged(&self) -> Option<ActionsService> {
        self.staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

#[cfg(feature = "actions")]
impl Plugin for EmbeddedActions {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
    fn start(&self, scope: PluginScope) -> PluginFuture {
        *self.observed_scope.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(scope.clone());
        let service = ActionsService::with_scope(self.ctx.clone(), scope);
        *self.staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(service.clone());
        let ctx = self.ctx.clone();
        Box::pin(async move {
            reconcile_pending(&ctx).await.map_err(|error| error.to_string())?;
            Ok(())
        })
    }
    fn stop(&self) -> PluginFuture {
        let service = self.staged();
        let cancel_runs = self.cancel_on_stop.load(Ordering::Acquire);
        let ctx = self.ctx.clone();
        let staged = Arc::clone(&self.staged);
        Box::pin(async move {
            if let Some(service) = service {
                service.stop_background().await.map_err(|error| error.to_string())?;
                if cancel_runs {
                    service.cancel_unfinished().await.map_err(|error| error.to_string())?;
                    ctx.database.plugin_settings().complete_disable(ACTIONS_ID).await.map_err(|error| error.to_string())?;
                }
            }
            staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
            Ok(())
        })
    }
}

/// Reconcile a crash after disable intent was saved but before old task credentials
/// were revoked, using an unpublished generation that cannot admit requests.
#[cfg(feature = "actions")]
async fn reconcile_pending(ctx: &ServiceContext) -> Result<(), ServiceError> {
    if ctx.database.plugin_settings().disable_pending(ACTIONS_ID).await? {
        let service = ActionsService::with_scope(ctx.clone(), PluginScope::new(Generation::INITIAL));
        service.quiesce();
        service.cancel_unfinished().await?;
        ctx.database.plugin_settings().complete_disable(ACTIONS_ID).await?;
    }
    Ok(())
}

impl ActionsSlot {
    #[cfg(not(feature = "actions"))]
    pub(crate) async fn new(ctx: ServiceContext) -> Result<Self, ServiceError> {
        let enabled = ctx.database.plugin_settings().desired_enabled(ACTIONS_ID).await?.unwrap_or(ctx.config.actions.enabled);
        if enabled {
            Err(ServiceError::Invalid("this distribution was built without Actions".to_owned()))
        } else {
            Ok(Self {
                inner: Arc::new(Inner {
                    ctx,
                    revision: AtomicU64::new(1),
                    shutting_down: AtomicBool::new(false),
                    status: RwLock::new(None),
                }),
            })
        }
    }
    #[cfg(feature = "actions")]
    pub(crate) async fn new(ctx: ServiceContext) -> Result<Self, ServiceError> {
        #[cfg(feature = "actions")]
        {
            reconcile_pending(&ctx).await?;
            let id: PluginId = ACTIONS_ID.parse().map_err(ServiceError::internal)?;
            let manifest = PluginManifest {
                id: id.clone(),
                version: env!("CARGO_PKG_VERSION").parse().map_err(ServiceError::internal)?,
                host_api: "^1".parse().map_err(ServiceError::internal)?,
                requires: Vec::new(),
                provides: Vec::new(),
                capabilities: Vec::new(),
            };
            let background_started = Arc::new(AtomicBool::new(false));
            let observed_scope = Arc::new(RwLock::new(None));
            let plugin = Arc::new(EmbeddedActions {
                manifest,
                ctx: ctx.clone(),
                staged: Arc::new(Mutex::new(None)),
                observed_scope: Arc::clone(&observed_scope),
                cancel_on_stop: AtomicBool::new(false),
            });
            let mut manager = PluginManager::new(semver::Version::new(1, 0, 0), Duration::from_secs(30));
            manager.install(plugin.clone()).map_err(ServiceError::internal)?;
            let enabled = ctx.database.plugin_settings().desired_enabled(ACTIONS_ID).await?.unwrap_or(ctx.config.actions.enabled);
            let status = manager.status(&id);
            let slot = Self {
                inner: Arc::new(Inner {
                    ctx,
                    revision: AtomicU64::new(1),
                    shutting_down: AtomicBool::new(false),
                    status: RwLock::new(status),
                    current: RwLock::new(None),
                    background_started,
                    observed_scope,
                    desired_enabled: AtomicBool::new(enabled),
                    controller: Arc::new(tokio::sync::Mutex::new(Controller { manager, plugin })),
                }),
            };
            if enabled {
                let mut controller = slot.inner.controller.lock().await;
                controller.manager.enable(&id).await.map_err(ServiceError::internal)?;
                let service = controller
                    .plugin
                    .staged()
                    .ok_or_else(|| ServiceError::internal(std::io::Error::other("Actions activated without a service")))?;
                slot.publish(&service)?;
                slot.cache_status(controller.manager.status(&id));
            }
            Ok(slot)
        }
    }
    #[cfg(feature = "actions")]
    pub fn snapshot(&self) -> Option<ActionsService> {
        if !self.inner.ctx.actions_availability.is_enabled() {
            return None;
        }
        self.inner.current.read().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().filter(|service| service.is_active()).cloned()
    }
    pub fn is_some(&self) -> bool {
        #[cfg(feature = "actions")]
        {
            self.snapshot().is_some()
        }
        #[cfg(not(feature = "actions"))]
        {
            false
        }
    }
    pub fn is_none(&self) -> bool {
        !self.is_some()
    }
    pub fn revision(&self) -> u64 {
        self.inner.revision.load(Ordering::Acquire)
    }
    #[allow(clippy::unused_self)] // Uniform no-op lifecycle hook in a core-only distribution.
    pub(crate) fn spawn_background(&self) {
        #[cfg(feature = "actions")]
        {
            self.inner.background_started.store(true, Ordering::Release);
            if let Some(service) = self.snapshot() {
                service.spawn_background();
            }
        }
    }
    #[allow(clippy::unused_self)] // Core-only builds own no Actions task scope.
    pub(crate) fn background_tasks(&self) -> usize {
        #[cfg(feature = "actions")]
        {
            self.inner.observed_scope.read().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().map_or(0, PluginScope::in_flight)
        }
        #[cfg(not(feature = "actions"))]
        {
            0
        }
    }
    pub(crate) fn status(&self) -> Option<PluginStatus> {
        let status = self.inner.status.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        #[cfg(feature = "actions")]
        {
            let mut status = status;
            if let Some(status) = &mut status
                && let Some(scope) = self.inner.observed_scope.read().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref()
            {
                status.generation = Some(scope.generation());
                status.in_flight = scope.in_flight();
                if status.state == rbit_plugin_api::PluginState::Active && scope.is_closed() {
                    status.state = rbit_plugin_api::PluginState::RestartRequired;
                }
            }
            status
        }
        #[cfg(not(feature = "actions"))]
        {
            status
        }
    }
    #[cfg(feature = "actions")]
    fn cache_status(&self, mut status: Option<PluginStatus>) {
        if let Some(status) = &mut status {
            status.desired = if self.inner.desired_enabled.load(Ordering::Acquire) {
                rbit_plugin_api::DesiredState::Enabled
            } else {
                rbit_plugin_api::DesiredState::Disabled
            };
        }
        if status.as_ref().is_none_or(|status| status.generation.is_none()) {
            *self.inner.observed_scope.write().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
        *self.inner.status.write().unwrap_or_else(std::sync::PoisonError::into_inner) = status;
    }
    pub(crate) async fn set_enabled(&self, actor: &User, enabled: bool, audit: crate::AuditService) -> Result<(), ServiceError> {
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(ServiceError::Conflict("server shutdown"));
        }
        #[cfg(not(feature = "actions"))]
        {
            if enabled {
                return Err(ServiceError::Invalid("this distribution was built without Actions".to_owned()));
            }
            self.inner.ctx.database.plugin_settings().set_desired_enabled(ACTIONS_ID, false, Some(actor.id()), Timestamp::now()).await?;
            record_change(&audit, actor, false).await;
            Ok(())
        }
        #[cfg(feature = "actions")]
        {
            let mut controller =
                Arc::clone(&self.inner.controller).try_lock_owned().map_err(|_| ServiceError::Conflict("plugin lifecycle operation"))?;
            let slot = self.clone();
            let actor = actor.clone();
            // Dropping an HTTP request only detaches its waiter. The bounded, serialized
            // host operation keeps intent, cleanup, publication and audit together.
            tokio::spawn(async move {
                let id = controller.plugin.manifest.id.clone();
                let mut status = controller.manager.status(&id);
                if let Some(status) = &mut status {
                    status.desired = if enabled { rbit_plugin_api::DesiredState::Enabled } else { rbit_plugin_api::DesiredState::Disabled };
                    status.state = if enabled { rbit_plugin_api::PluginState::Preparing } else { rbit_plugin_api::PluginState::Quiescing };
                }
                slot.cache_status(status);
                let result = slot.change_enabled(&actor, enabled, &mut controller, &audit).await;
                slot.cache_status(controller.manager.status(&id));
                result
            })
            .await
            .map_err(ServiceError::internal)?
        }
    }
    #[cfg(feature = "actions")]
    async fn change_enabled(
        &self,
        actor: &User,
        enabled: bool,
        controller: &mut Controller,
        audit: &crate::AuditService,
    ) -> Result<(), ServiceError> {
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(ServiceError::Conflict("server shutdown"));
        }
        self.inner.ctx.database.plugin_settings().set_desired_enabled(ACTIONS_ID, enabled, Some(actor.id()), Timestamp::now()).await?;
        self.inner.desired_enabled.store(enabled, Ordering::Release);
        self.cache_status(self.status());
        if !enabled {
            // No database or audit await may leave a persisted disable still admitting work.
            self.unpublish();
            controller.plugin.cancel_on_stop.store(true, Ordering::Release);
        }
        record_change(audit, actor, enabled).await;
        let id = controller.plugin.manifest.id.clone();
        if enabled {
            controller.manager.enable(&id).await.map_err(ServiceError::internal)?;
            let service = controller
                .plugin
                .staged()
                .ok_or_else(|| ServiceError::internal(std::io::Error::other("Actions activated without a service")))?;
            if !self.is_some() {
                self.publish(&service)?;
            }
        } else {
            controller.manager.disable(&id).await.map_err(ServiceError::internal)?;
            reconcile_pending(&self.inner.ctx).await?;
        }
        Ok(())
    }
    #[cfg(feature = "actions")]
    fn publish(&self, service: &ActionsService) -> Result<(), ServiceError> {
        let mut current = self.inner.current.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.inner.shutting_down.load(Ordering::Acquire) {
            service.quiesce();
            return Err(ServiceError::Conflict("server shutdown"));
        }
        // The listener is attached synchronously before callers can observe activation.
        if self.inner.background_started.load(Ordering::Acquire) {
            service.spawn_background();
        }
        *current = Some(service.clone());
        self.inner.ctx.actions_availability.set_enabled(true);
        self.inner.revision.fetch_add(1, Ordering::AcqRel);
        drop(current);
        Ok(())
    }
    #[cfg(feature = "actions")]
    fn unpublish(&self) {
        self.inner.ctx.actions_availability.set_enabled(false);
        let mut current = self.inner.current.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        // A publisher may have held this lock before the first revocation store.
        self.inner.ctx.actions_availability.set_enabled(false);
        let service = current.take();
        if let Some(service) = service {
            service.quiesce();
        }
        self.inner.revision.fetch_add(1, Ordering::AcqRel);
    }
    #[allow(clippy::unused_async)] // The core-only build has no worker to drain.
    pub(crate) async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::Release);
        #[cfg(feature = "actions")]
        {
            self.unpublish();
            let mut controller = self.inner.controller.lock().await;
            // Finish revocation after any operation that was already preparing while
            // shutdown began. No new lifecycle operation can pass the shutdown flag.
            self.unpublish();
            controller.plugin.cancel_on_stop.store(false, Ordering::Release);
            let id = controller.plugin.manifest.id.clone();
            if let Err(error) = controller.manager.disable(&id).await {
                tracing::warn!(%error, "Actions did not finish shutting down");
            }
            self.cache_status(controller.manager.status(&id));
        }
    }
}

async fn record_change(audit: &crate::AuditService, actor: &User, enabled: bool) {
    audit
        .record(
            crate::audit::AuditEntry::new(crate::audit::AuditAction::InstanceSettingsChanged).by(actor).on(ACTIONS_ID).detail(if enabled {
                "Actions enable requested"
            } else {
                "Actions disable requested"
            }),
        )
        .await;
}

#[cfg(all(test, feature = "actions"))]
mod tests {
    use super::*;
    use rbit_core::Config;
    use rbit_storage::{Database, KvStore};

    #[tokio::test]
    async fn shutdown_rejects_new_publication_and_clears_a_previously_staged_service() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.updates.check_for_updates = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = crate::Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let slot = services.actions.clone();
        let mut controller = slot.inner.controller.lock().await;
        let id = controller.plugin.manifest.id.clone();
        controller.manager.enable(&id).await.unwrap();
        let service = controller.plugin.staged().unwrap();

        let shutting_down = services.clone();
        let shutdown = tokio::spawn(async move { shutting_down.shutdown().await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while !slot.inner.shutting_down.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(slot.publish(&service).is_err());

        // Simulate a publication that passed an earlier shutdown check while the
        // controller operation still held its lock. Final shutdown must remove it.
        *slot.inner.current.write().unwrap() = Some(service);
        slot.inner.ctx.actions_availability.set_enabled(true);
        drop(controller);
        shutdown.await.unwrap();
        assert!(slot.inner.current.read().unwrap().is_none());
        assert!(!slot.inner.ctx.actions_availability.is_enabled());
        assert!(slot.is_none());
    }
}
