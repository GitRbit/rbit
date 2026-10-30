//! Deterministic service dependency resolution before plugin activation.

use std::collections::{BTreeMap, BTreeSet};

use rbit_plugin_api::{ManifestError, PluginId, PluginManifest, ServiceId};

/// Maximum installed plugins accepted by one resolution operation.
pub const MAX_PLUGINS: usize = 128;

/// Dependencies and activation order for the requested enabled plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// Ready plugins in provider-before-consumer order, with lexical tie breaking.
    pub activation_order: Vec<PluginId>,
    /// Enabled plugins missing a required service, directly or transitively.
    pub waiting: BTreeSet<PluginId>,
    /// Known provider plugin IDs required by each enabled plugin.
    pub dependencies: BTreeMap<PluginId, BTreeSet<PluginId>>,
}

/// Why a proposed enabled dependency graph cannot be activated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    #[error("plugin registry exceeds the limit of {MAX_PLUGINS} installed plugins")]
    TooManyPlugins,
    #[error("enabled plugin is not installed: {plugin}")]
    MissingPlugin { plugin: PluginId },
    #[error("manifest {declared} is installed under the different key {key}")]
    ManifestIdMismatch { key: PluginId, declared: PluginId },
    #[error("invalid manifest for {plugin}: {source}")]
    InvalidManifest { plugin: PluginId, source: ManifestError },
    #[error("service {service} has conflicting enabled providers {first} and {second}")]
    ConflictingProviders { service: ServiceId, first: PluginId, second: PluginId },
    #[error("plugin {plugin} requires an incompatible version of service {service} from {provider}")]
    IncompatibleService { plugin: PluginId, service: ServiceId, provider: PluginId },
    /// Contains the plugins in cycles and any consumers blocked behind those cycles.
    #[error("dependency cycle prevents ordering plugins: {plugins:?}")]
    Cycle { plugins: Vec<PluginId> },
}

/// Resolves only enabled service providers; installing a package never activates it.
///
/// Missing providers produce a waiting state. Conflicting providers, incompatible
/// service versions and cycles reject the proposed graph even when a participant
/// would otherwise wait for another missing service. All collections are bounded,
/// and unrelated ready plugins retain deterministic activation order.
pub fn resolve(manifests: &BTreeMap<PluginId, PluginManifest>, enabled: &BTreeSet<PluginId>) -> Result<Resolution, GraphError> {
    if manifests.len() > MAX_PLUGINS || enabled.len() > MAX_PLUGINS {
        return Err(GraphError::TooManyPlugins);
    }
    let mut providers = BTreeMap::new();
    for id in enabled {
        let manifest = manifests.get(id).ok_or_else(|| GraphError::MissingPlugin { plugin: id.clone() })?;
        if &manifest.id != id {
            return Err(GraphError::ManifestIdMismatch { key: id.clone(), declared: manifest.id.clone() });
        }
        manifest.validate().map_err(|source| GraphError::InvalidManifest { plugin: id.clone(), source })?;
        for provided in &manifest.provides {
            if let Some((first, _)) = providers.insert(provided.id.clone(), (id.clone(), &provided.version)) {
                return Err(GraphError::ConflictingProviders { service: provided.id.clone(), first, second: id.clone() });
            }
        }
    }

    let mut dependencies: BTreeMap<PluginId, BTreeSet<PluginId>> = BTreeMap::new();
    let mut dependents: BTreeMap<PluginId, BTreeSet<PluginId>> = BTreeMap::new();
    let mut waiting = BTreeSet::new();
    for id in enabled {
        let manifest = &manifests[id];
        let required = dependencies.entry(id.clone()).or_default();
        for dependency in &manifest.requires {
            let Some((provider, version)) = providers.get(&dependency.id) else {
                waiting.insert(id.clone());
                continue;
            };
            if !dependency.version.matches(version) {
                return Err(GraphError::IncompatibleService {
                    plugin: id.clone(),
                    service: dependency.id.clone(),
                    provider: provider.clone(),
                });
            }
            required.insert(provider.clone());
            dependents.entry(provider.clone()).or_default().insert(id.clone());
        }
    }

    // Spread waiting state through consumer edges without losing the full dependency
    // graph needed for reverse-order unloading and cycle validation.
    let mut pending = waiting.clone();
    while let Some(id) = pending.pop_first() {
        if let Some(consumers) = dependents.get(&id) {
            for consumer in consumers {
                if waiting.insert(consumer.clone()) {
                    pending.insert(consumer.clone());
                }
            }
        }
    }

    let mut remaining: BTreeMap<_, _> = dependencies.iter().map(|(id, providers)| (id.clone(), providers.len())).collect();
    let mut ready: BTreeSet<_> = remaining.iter().filter_map(|(id, count)| (*count == 0).then_some(id.clone())).collect();
    let mut ordered = Vec::with_capacity(enabled.len());
    while let Some(id) = ready.pop_first() {
        if let Some(consumers) = dependents.get(&id) {
            for consumer in consumers {
                if let Some(count) = remaining.get_mut(consumer) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(consumer.clone());
                    }
                }
            }
        }
        ordered.push(id);
    }
    if ordered.len() != enabled.len() {
        return Err(GraphError::Cycle { plugins: remaining.into_iter().filter_map(|(id, count)| (count > 0).then_some(id)).collect() });
    }
    ordered.retain(|id| !waiting.contains(id));
    Ok(Resolution { activation_order: ordered, waiting, dependencies })
}
