//! Service graph behavior across installation, enablement and provider changes.

use std::collections::{BTreeMap, BTreeSet};

use rbit_plugin_api::{PluginId, PluginManifest, ServiceDependency, ServiceProvided};
use rbit_plugin_runtime::{GraphError, MAX_PLUGINS, resolve};

fn id(value: &str) -> PluginId {
    value.parse().unwrap()
}

fn manifest(name: &str, requires: &[(&str, &str)], provides: &[(&str, &str)]) -> PluginManifest {
    PluginManifest {
        id: id(name),
        version: "1.0.0".parse().unwrap(),
        host_api: "^1".parse().unwrap(),
        requires: requires
            .iter()
            .map(|(name, version)| ServiceDependency { id: name.parse().unwrap(), version: version.parse().unwrap() })
            .collect(),
        provides: provides
            .iter()
            .map(|(name, version)| ServiceProvided { id: name.parse().unwrap(), version: version.parse().unwrap() })
            .collect(),
        capabilities: Vec::new(),
    }
}

fn installed(manifests: Vec<PluginManifest>) -> BTreeMap<PluginId, PluginManifest> {
    manifests.into_iter().map(|manifest| (manifest.id.clone(), manifest)).collect()
}

#[test]
fn empty_registry_resolves() {
    let resolved = resolve(&BTreeMap::new(), &BTreeSet::new()).unwrap();
    assert!(resolved.activation_order.is_empty());
    assert!(resolved.waiting.is_empty());
    assert!(resolved.dependencies.is_empty());
}

#[test]
fn missing_provider_waits_transitively_without_stalling_unrelated_plugins() {
    let manifests = installed(vec![
        manifest("example.consumer", &[("example.middle", "^1")], &[]),
        manifest("example.middle", &[("example.missing", "^1")], &[("example.middle", "1.0.0")]),
        manifest("example.unrelated", &[], &[]),
    ]);
    let resolved = resolve(&manifests, &manifests.keys().cloned().collect()).unwrap();
    assert_eq!(resolved.activation_order, vec![id("example.unrelated")]);
    assert_eq!(resolved.waiting, BTreeSet::from([id("example.consumer"), id("example.middle")]));
    assert_eq!(resolved.dependencies[&id("example.consumer")], BTreeSet::from([id("example.middle")]));
}

#[test]
fn installed_provider_is_unavailable_until_enabled() {
    let manifests = installed(vec![
        manifest("example.consumer", &[("example.service", "^1")], &[]),
        manifest("example.provider", &[], &[("example.service", "1.2.0")]),
    ]);
    let resolved = resolve(&manifests, &BTreeSet::from([id("example.consumer")])).unwrap();
    assert_eq!(resolved.waiting, BTreeSet::from([id("example.consumer")]));
    let resolved = resolve(&manifests, &manifests.keys().cloned().collect()).unwrap();
    assert!(resolved.waiting.is_empty());
    assert_eq!(resolved.activation_order, vec![id("example.provider"), id("example.consumer")]);
}

#[test]
fn topological_order_uses_lexical_ties_and_counts_provider_once() {
    let manifests = installed(vec![
        manifest("example.alpha", &[("example.first", "^1"), ("example.second", "^1")], &[]),
        manifest("example.beta", &[], &[]),
        manifest("example.provider", &[], &[("example.first", "1.0.0"), ("example.second", "1.0.0")]),
    ]);
    let resolved = resolve(&manifests, &manifests.keys().cloned().collect()).unwrap();
    assert_eq!(resolved.activation_order, vec![id("example.beta"), id("example.provider"), id("example.alpha")]);
    assert_eq!(resolved.dependencies[&id("example.alpha")].len(), 1);
}

#[test]
fn duplicate_enabled_providers_are_rejected_even_when_waiting() {
    let manifests = installed(vec![
        manifest("example.first", &[("example.missing", "^1")], &[("example.service", "1.0.0")]),
        manifest("example.second", &[], &[("example.service", "1.0.0")]),
    ]);
    assert!(matches!(resolve(&manifests, &manifests.keys().cloned().collect()), Err(GraphError::ConflictingProviders { .. })));
    resolve(&manifests, &BTreeSet::from([id("example.second")])).unwrap();
}

#[test]
fn incompatible_provider_versions_are_rejected() {
    let manifests = installed(vec![
        manifest("example.consumer", &[("example.service", "^2")], &[]),
        manifest("example.provider", &[], &[("example.service", "1.0.0")]),
    ]);
    assert!(matches!(resolve(&manifests, &manifests.keys().cloned().collect()), Err(GraphError::IncompatibleService { .. })));
}

#[test]
fn cycles_are_rejected_even_with_a_missing_dependency() {
    let manifests = installed(vec![
        manifest("example.first", &[("example.second", "^1"), ("example.missing", "^1")], &[("example.first", "1.0.0")]),
        manifest("example.second", &[("example.first", "^1")], &[("example.second", "1.0.0")]),
    ]);
    assert!(matches!(resolve(&manifests, &manifests.keys().cloned().collect()), Err(GraphError::Cycle { plugins }) if plugins.len() == 2));
}

#[test]
fn enabled_missing_plugins_and_mismatched_manifest_ids_are_rejected() {
    assert!(matches!(resolve(&BTreeMap::new(), &BTreeSet::from([id("example.missing")])), Err(GraphError::MissingPlugin { .. })));
    let manifests = BTreeMap::from([(id("example.key"), manifest("example.other", &[], &[]))]);
    assert!(matches!(resolve(&manifests, &BTreeSet::from([id("example.key")])), Err(GraphError::ManifestIdMismatch { .. })));
}

#[test]
fn invalid_in_memory_manifests_cannot_bypass_validation() {
    let manifests = installed(vec![manifest("example.provider", &[], &[("example.service", "1.0.0"), ("example.service", "1.0.0")])]);
    assert!(matches!(resolve(&manifests, &manifests.keys().cloned().collect()), Err(GraphError::InvalidManifest { .. })));
}

#[test]
fn registry_size_is_bounded_before_resolution() {
    let manifests = installed((0..=MAX_PLUGINS).map(|index| manifest(&format!("example.plugin-{index}"), &[], &[])).collect());
    assert_eq!(resolve(&manifests, &BTreeSet::new()), Err(GraphError::TooManyPlugins));
}
