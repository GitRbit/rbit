use rbit_plugin_api::{
    Generation, PluginId, UiContent, UiContribution, UiContributionSet, UiIcon, UiLink, UiMessage, UiSlot, UiVisibility,
};
use rbit_plugin_runtime::{PluginScope, UiRegistry, UiRegistryError};
use std::{
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

fn owner() -> PluginId {
    "example.plugin".parse().unwrap()
}
fn scope(value: u64) -> PluginScope {
    PluginScope::new(Generation::new(value).unwrap())
}
fn set(label: &str) -> UiContributionSet {
    UiContributionSet::new(vec![UiContribution {
        id: "example.nav".parse().unwrap(),
        slot: UiSlot::GlobalNav,
        order: 0,
        visibility: UiVisibility::Public,
        content: UiContent::Link(UiLink {
            label: UiMessage { namespace: owner().to_string(), key: "label".into(), fallback: label.into() },
            target: "/plugins/example".parse().unwrap(),
            icon: UiIcon::Box,
        }),
    }])
}
fn label(registry: &UiRegistry) -> String {
    match &registry.snapshot().entries[0].contribution.content {
        UiContent::Link(link) => link.label.fallback.clone(),
        _ => panic!("link expected"),
    }
}

#[tokio::test]
async fn staging_is_invisible_publication_idempotent_and_quiesce_synchronously_withdraws() {
    let registry = UiRegistry::default();
    let scope = scope(1);
    let initial = registry.snapshot();
    let registration = registry.stage(owner(), &scope, scope.generation(), set("First")).unwrap();
    assert!(registry.snapshot().entries.is_empty());
    assert_eq!(registry.snapshot().revision, initial.revision);
    registration.publish().unwrap();
    let published = registry.snapshot();
    assert_eq!(published.entries.len(), 1);
    assert!(published.revision > initial.revision);
    registration.publish().unwrap();
    assert_eq!(registry.snapshot().revision, published.revision);
    assert!(Arc::ptr_eq(&published.entries, &registry.snapshot().entries));
    assert_eq!(scope.in_flight(), 0); // Data snapshots and registration never lease a request.
    scope.dispose(Duration::from_secs(1)).await.unwrap();
    assert!(registry.snapshot().entries.is_empty());
    assert!(registration.publish().is_err());
    assert_eq!(published.entries.len(), 1); // Prior render remains an immutable data snapshot.
}

#[test]
fn scope_owns_registration_after_handles_drop_and_last_scope_drop_cleans_up() {
    let registry = UiRegistry::default();
    let scope = scope(2);
    let clone = scope.clone();
    registry.stage(owner(), &scope, scope.generation(), set("First")).unwrap().publish().unwrap();
    drop(scope);
    assert_eq!(registry.snapshot().entries.len(), 1);
    drop(clone);
    assert!(registry.snapshot().entries.is_empty());
}

#[test]
fn failed_atomic_sets_and_conflicts_leave_prior_publication_unchanged() {
    let registry = UiRegistry::default();
    let scope = scope(3);
    let mut invalid = set("Bad");
    invalid.contributions.push(invalid.contributions[0].clone());
    assert!(registry.stage(owner(), &scope, scope.generation(), invalid).is_err());
    assert!(registry.snapshot().entries.is_empty());
    let handle = registry.stage(owner(), &scope, scope.generation(), set("Good")).unwrap();
    handle.publish().unwrap();
    let revision = registry.snapshot().revision;
    assert!(matches!(registry.stage(owner(), &scope, scope.generation(), set("Other")), Err(UiRegistryError::Conflict)));
    assert_eq!(label(&registry), "Good");
    assert_eq!(registry.snapshot().revision, revision);
}

#[test]
fn stale_generations_and_old_handles_cannot_modify_a_replacement_even_with_reused_generation_numbers() {
    let registry = UiRegistry::default();
    let first = scope(4);
    assert!(registry.stage(owner(), &first, Generation::new(5).unwrap(), set("Wrong")).is_err());
    let old = registry.stage(owner(), &first, first.generation(), set("Old")).unwrap();
    old.publish().unwrap();
    assert!(old.unpublish());
    let second = scope(4);
    let new = registry.stage(owner(), &second, second.generation(), set("New")).unwrap();
    new.publish().unwrap();
    assert!(!old.unpublish());
    assert!(old.publish().is_err());
    assert!(old.replace(set("Stale")).is_err());
    first.quiesce();
    assert_eq!(label(&registry), "New");
    second.quiesce();
    assert!(registry.snapshot().entries.is_empty());
}

#[test]
fn replace_is_atomic_bounded_and_preserves_prior_render_snapshots() {
    let registry = UiRegistry::default();
    let scope = scope(6);
    let handle = registry.stage(owner(), &scope, scope.generation(), set("First")).unwrap();
    handle.replace(set("Prepared")).unwrap();
    assert!(registry.snapshot().entries.is_empty());
    handle.publish().unwrap();
    let old = registry.snapshot();
    assert_eq!(label(&registry), "Prepared");
    let mut invalid = set("Bad");
    invalid.schema_version = 2;
    assert!(handle.replace(invalid).is_err());
    assert_eq!(registry.snapshot().revision, old.revision);
    handle.replace(set("Updated")).unwrap();
    assert_eq!(registry.snapshot().revision, old.revision + 1);
    assert_eq!(label(&registry), "Updated");
    let revision = registry.snapshot().revision;
    handle.replace(set("Updated")).unwrap();
    assert_eq!(registry.snapshot().revision, revision);
    match &old.entries[0].contribution.content {
        UiContent::Link(link) => assert_eq!(link.label.fallback, "Prepared"),
        _ => panic!("link expected"),
    }
    scope.quiesce();
    assert!(handle.replace(set("Late")).is_err());
}

#[test]
fn concurrent_stage_publish_close_never_resurrects_a_generation() {
    for value in 10..110 {
        let registry = UiRegistry::default();
        let scope = scope(value);
        let barrier = Arc::new(Barrier::new(2));
        let worker_registry = registry.clone();
        let worker_scope = scope.clone();
        let worker_barrier = Arc::clone(&barrier);
        let worker = thread::spawn(move || {
            worker_barrier.wait();
            if let Ok(handle) = worker_registry.stage(owner(), &worker_scope, worker_scope.generation(), set("Raced")) {
                let _ = handle.publish();
            }
        });
        barrier.wait();
        scope.quiesce();
        worker.join().unwrap();
        assert!(registry.snapshot().entries.is_empty());
    }
}
