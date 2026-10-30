//! Serialized embedded-plugin lifecycle with dependency-aware activation and retirement.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use rbit_plugin_api::{DesiredState, Generation, ManifestError, PluginId, PluginManifest, PluginState};
use semver::Version;
use tokio::time::Instant;

use crate::{GraphError, MAX_PLUGINS, PluginScope, Resolution, resolve};

// A recreated controller must not reuse a generation carried by an older host handle.
// Persistent credentials additionally need an instance epoch across process restarts.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn allocate_generation() -> Result<Generation, ManagerError> {
    let mut value = NEXT_GENERATION.load(Ordering::Relaxed);
    loop {
        if value == 0 {
            return Err(ManagerError::GenerationExhausted);
        }
        let next = value.checked_add(1).unwrap_or(0);
        match NEXT_GENERATION.compare_exchange_weak(value, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Generation::new(value).map_err(|_| ManagerError::GenerationExhausted),
            Err(current) => value = current,
        }
    }
}

/// Owned callback future, retained by the manager if its caller cancels an operation.
pub type PluginFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'static>>;

/// Trusted embedded implementation. Installation calls only [`Self::manifest`].
///
/// Callbacks must return promptly and perform asynchronous work in their returned future.
/// All work must use the supplied scope; arbitrary native side effects are not sandboxed.
/// Teardown may run after partial startup and must be safe to retry after an error.
pub trait Plugin: Debug + Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    fn start(&self, scope: PluginScope) -> PluginFuture;
    fn stop(&self) -> PluginFuture {
        Box::pin(async { Ok(()) })
    }
}

/// Administrative intent and the current activation, without granting operation access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginStatus {
    pub id: PluginId,
    pub desired: DesiredState,
    pub state: PluginState,
    pub generation: Option<Generation>,
    pub in_flight: usize,
}

/// Lifecycle errors never imply a timed-out generation was successfully unloaded.
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("plugin is not installed: {0}")]
    NotInstalled(PluginId),
    #[error("plugin is already installed: {0}")]
    AlreadyInstalled(PluginId),
    #[error("plugin registry capacity exhausted")]
    Capacity,
    #[error("plugin generation counter exhausted")]
    GenerationExhausted,
    #[error("plugin manifest is invalid: {0}")]
    Manifest(#[from] ManifestError),
    #[error("plugin dependency graph is invalid: {0}")]
    Graph(#[from] GraphError),
    #[error("plugin {plugin} has incomplete cleanup; disable it before enabling again")]
    Incomplete { plugin: PluginId },
    #[error("plugin {plugin} {phase} failed: {message}")]
    Callback { plugin: PluginId, phase: &'static str, message: String },
    #[error("plugin {plugin} {phase} timed out")]
    Timeout { plugin: PluginId, phase: &'static str },
    #[error("plugin {plugin} did not drain: {source}")]
    Drain { plugin: PluginId, source: crate::DrainError },
}

enum Callback {
    Start(PluginFuture),
    Stop(PluginFuture),
}

impl Debug for Callback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Start(_) => "Start(..)",
            Self::Stop(_) => "Stop(..)",
        })
    }
}

#[derive(Debug)]
struct Entry {
    plugin: Arc<dyn Plugin>,
    manifest: PluginManifest,
    desired: DesiredState,
    state: PluginState,
    scope: Option<PluginScope>,
    callback: Option<Callback>,
    stop_complete: bool,
}

impl Entry {
    fn is_active(&self) -> bool {
        self.state == PluginState::Active && self.scope.as_ref().is_some_and(|scope| !scope.is_closed())
    }
}

struct PreparationGuard {
    scope: PluginScope,
    armed: bool,
}

impl Drop for PreparationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.scope.quiesce();
        }
    }
}

/// In-memory controller for trusted embedded plugins.
///
/// Mutable access serializes lifecycle changes. This controller owns generations and
/// dependency metadata, not HTTP routes, persistent desired state or capability grants.
/// A host must persist intent and publish its own contributions around these operations.
#[derive(Debug)]
pub struct PluginManager {
    host_api: Version,
    cleanup_timeout: Duration,
    entries: BTreeMap<PluginId, Entry>,
    live_order: Vec<PluginId>,
}

impl PluginManager {
    pub fn new(host_api: Version, cleanup_timeout: Duration) -> Self {
        Self { host_api, cleanup_timeout, entries: BTreeMap::new(), live_order: Vec::new() }
    }

    /// Validates metadata without starting plugin code or changing other installations.
    pub fn install(&mut self, plugin: Arc<dyn Plugin>) -> Result<(), ManagerError> {
        let manifest = plugin.manifest().clone();
        manifest.check_host(&self.host_api)?;
        if self.entries.contains_key(&manifest.id) {
            return Err(ManagerError::AlreadyInstalled(manifest.id));
        }
        if self.entries.len() >= MAX_PLUGINS {
            return Err(ManagerError::Capacity);
        }
        self.entries.insert(
            manifest.id.clone(),
            Entry {
                plugin,
                manifest,
                desired: DesiredState::Disabled,
                state: PluginState::Installed,
                scope: None,
                callback: None,
                stop_complete: false,
            },
        );
        Ok(())
    }

    pub fn status(&self, id: &PluginId) -> Option<PluginStatus> {
        let entry = self.entries.get(id)?;
        let state = if matches!(entry.state, PluginState::Preparing | PluginState::Active)
            && entry.scope.as_ref().is_some_and(PluginScope::is_closed)
        {
            PluginState::RestartRequired
        } else {
            entry.state
        };
        Some(PluginStatus {
            id: id.clone(),
            desired: entry.desired,
            state,
            generation: entry.scope.as_ref().map(PluginScope::generation),
            in_flight: entry.scope.as_ref().map_or(0, PluginScope::in_flight),
        })
    }

    pub fn statuses(&self) -> Vec<PluginStatus> {
        self.entries.keys().filter_map(|id| self.status(id)).collect()
    }

    /// Returns only an active scope. Retained scopes still reject operations after quiesce.
    pub fn scope(&self, id: &PluginId) -> Option<PluginScope> {
        self.entries.get(id).filter(|entry| entry.is_active()).and_then(|entry| entry.scope.clone())
    }

    pub async fn enable(&mut self, id: &PluginId) -> Result<(), ManagerError> {
        let entry = self.entries.get_mut(id).ok_or_else(|| ManagerError::NotInstalled(id.clone()))?;
        if entry.scope.is_some() && !entry.is_active() {
            return Err(ManagerError::Incomplete { plugin: id.clone() });
        }
        let previous = (entry.desired, entry.state);
        entry.desired = DesiredState::Enabled;
        if entry.scope.is_none() {
            entry.state = PluginState::Waiting;
        }
        let resolution = match self.resolution() {
            Ok(resolution) => resolution,
            Err(error) => {
                if let Some(entry) = self.entries.get_mut(id) {
                    (entry.desired, entry.state) = previous;
                }
                return Err(error);
            }
        };
        self.reconcile(resolution).await
    }

    pub async fn disable(&mut self, id: &PluginId) -> Result<(), ManagerError> {
        let entry = self.entries.get_mut(id).ok_or_else(|| ManagerError::NotInstalled(id.clone()))?;
        entry.desired = DesiredState::Disabled;
        if entry.scope.is_none() {
            entry.state = PluginState::Disabled;
        }
        let resolution = self.resolution()?;
        self.reconcile(resolution).await
    }

    /// Replaces the generation while preserving enabled intent for dependent plugins.
    pub async fn reload(&mut self, id: &PluginId) -> Result<(), ManagerError> {
        let entry = self.entries.get(id).ok_or_else(|| ManagerError::NotInstalled(id.clone()))?;
        if entry.desired == DesiredState::Disabled {
            return Err(ManagerError::Incomplete { plugin: id.clone() });
        }
        let resolution = self.resolution()?;
        let mut affected = BTreeSet::from([id.clone()]);
        loop {
            let before = affected.len();
            for (consumer, dependencies) in &resolution.dependencies {
                if !dependencies.is_disjoint(&affected) {
                    affected.insert(consumer.clone());
                }
            }
            if affected.len() == before {
                break;
            }
        }
        let retiring: Vec<_> = self.live_order.iter().rev().filter(|id| affected.contains(*id)).cloned().collect();
        self.quiesce(&retiring);
        for id in retiring {
            self.retire(&id).await?;
        }
        if let Some(entry) = self.entries.get_mut(id)
            && entry.scope.is_none()
        {
            // Reload is an explicit retry after a previous, fully cleaned startup failure.
            entry.state = PluginState::Waiting;
        }
        self.reconcile(resolution).await
    }

    pub async fn uninstall(&mut self, id: &PluginId) -> Result<(), ManagerError> {
        self.disable(id).await?;
        self.entries.remove(id);
        Ok(())
    }

    fn resolution(&self) -> Result<Resolution, ManagerError> {
        let manifests = self.entries.iter().map(|(id, entry)| (id.clone(), entry.manifest.clone())).collect();
        let enabled = self.entries.iter().filter(|(_, entry)| entry.desired == DesiredState::Enabled).map(|(id, _)| id.clone()).collect();
        Ok(resolve(&manifests, &enabled)?)
    }

    fn quiesce(&mut self, ids: &[PluginId]) {
        for id in ids {
            if let Some(entry) = self.entries.get_mut(id) {
                if let Some(scope) = &entry.scope {
                    scope.quiesce();
                }
                entry.state = PluginState::Quiescing;
            }
        }
    }

    async fn reconcile(&mut self, resolution: Resolution) -> Result<(), ManagerError> {
        let unexpectedly_closed: BTreeSet<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.state == PluginState::Active && !entry.is_active())
            .map(|(id, _)| id.clone())
            .collect();
        let mut unavailable = resolution.waiting.clone();
        unavailable.extend(self.entries.iter().filter(|(_, entry)| !entry.is_active()).map(|(id, _)| id.clone()));
        loop {
            let before = unavailable.len();
            for (consumer, dependencies) in &resolution.dependencies {
                if !dependencies.is_disjoint(&unavailable) {
                    unavailable.insert(consumer.clone());
                }
            }
            if before == unavailable.len() {
                break;
            }
        }
        let retiring: Vec<_> = self
            .live_order
            .iter()
            .rev()
            .filter(|id| self.entries.get(*id).is_some_and(|entry| entry.desired == DesiredState::Disabled || unavailable.contains(*id)))
            .cloned()
            .collect();
        self.quiesce(&retiring);
        for id in retiring {
            self.retire(&id).await?;
            if unexpectedly_closed.contains(&id)
                && let Some(entry) = self.entries.get_mut(&id)
            {
                entry.state = PluginState::Failed;
            }
        }
        for entry in self.entries.values_mut().filter(|entry| entry.scope.is_none()) {
            if entry.desired == DesiredState::Disabled && entry.state != PluginState::Installed {
                entry.state = PluginState::Disabled;
            }
        }
        for id in resolution.activation_order {
            let dependencies_ready = resolution.dependencies.get(&id).is_none_or(|dependencies| {
                dependencies.iter().all(|dependency| self.entries.get(dependency).is_some_and(Entry::is_active))
            });
            let Some(entry) = self.entries.get_mut(&id) else { continue };
            if entry.scope.is_some() || entry.state == PluginState::Failed {
                continue;
            }
            entry.state = PluginState::Waiting;
            if dependencies_ready {
                self.activate(&id).await?;
            }
        }
        Ok(())
    }

    async fn activate(&mut self, id: &PluginId) -> Result<(), ManagerError> {
        let generation = allocate_generation()?;
        let scope = PluginScope::new(generation);
        let mut guard = PreparationGuard { scope: scope.clone(), armed: true };
        let entry = self.entries.get_mut(id).ok_or_else(|| ManagerError::NotInstalled(id.clone()))?;
        entry.scope = Some(scope.clone());
        entry.state = PluginState::Preparing;
        entry.stop_complete = false;
        self.live_order.push(id.clone());
        entry.callback = Some(Callback::Start(entry.plugin.start(scope)));
        let Some(Callback::Start(callback)) = entry.callback.as_mut() else { return Err(ManagerError::Incomplete { plugin: id.clone() }) };
        let outcome = tokio::time::timeout(self.cleanup_timeout, callback).await;
        match outcome {
            Ok(Ok(())) if !guard.scope.is_closed() => {
                entry.callback = None;
                entry.state = PluginState::Active;
                guard.armed = false;
                Ok(())
            }
            Ok(result) => {
                let message = result.err().unwrap_or_else(|| "plugin closed its admission during startup".to_owned());
                entry.callback = None;
                guard.scope.quiesce();
                self.retire(id).await?;
                if let Some(entry) = self.entries.get_mut(id) {
                    entry.state = PluginState::Failed;
                }
                Err(ManagerError::Callback { plugin: id.clone(), phase: "start", message })
            }
            Err(_) => {
                entry.state = PluginState::RestartRequired;
                Err(ManagerError::Timeout { plugin: id.clone(), phase: "start" })
            }
        }
    }

    async fn retire(&mut self, id: &PluginId) -> Result<(), ManagerError> {
        let started = Instant::now();
        let entry = self.entries.get_mut(id).ok_or_else(|| ManagerError::NotInstalled(id.clone()))?;
        let Some(scope) = entry.scope.clone() else { return Ok(()) };
        scope.quiesce();
        entry.state = PluginState::Quiescing;
        if !entry.stop_complete {
            if !matches!(entry.callback, Some(Callback::Stop(_))) {
                // Drop an interrupted startup future before invoking its teardown factory.
                entry.callback = None;
                entry.callback = Some(Callback::Stop(entry.plugin.stop()));
            }
            let Some(Callback::Stop(callback)) = entry.callback.as_mut() else {
                return Err(ManagerError::Incomplete { plugin: id.clone() });
            };
            match tokio::time::timeout(self.cleanup_timeout, callback).await {
                Ok(Ok(())) => {
                    entry.callback = None;
                    entry.stop_complete = true;
                }
                Ok(Err(message)) => {
                    entry.callback = None;
                    entry.state = PluginState::RestartRequired;
                    return Err(ManagerError::Callback { plugin: id.clone(), phase: "stop", message });
                }
                Err(_) => {
                    entry.state = PluginState::RestartRequired;
                    return Err(ManagerError::Timeout { plugin: id.clone(), phase: "stop" });
                }
            }
        }
        if let Err(source) = scope.dispose(self.cleanup_timeout.saturating_sub(started.elapsed())).await {
            entry.state = PluginState::RestartRequired;
            return Err(ManagerError::Drain { plugin: id.clone(), source });
        }
        entry.scope = None;
        entry.state = if entry.desired == DesiredState::Disabled { PluginState::Disabled } else { PluginState::Waiting };
        self.live_order.retain(|live| live != id);
        Ok(())
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        // Drop requests cancellation even if a host still retains a scope clone. Async
        // cleanup must be awaited through disable before dropping the controller.
        for entry in self.entries.values() {
            if let Some(scope) = &entry.scope {
                scope.quiesce();
            }
        }
    }
}
