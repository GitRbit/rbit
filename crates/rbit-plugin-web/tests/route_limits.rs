//! Aggregate budgets must include invisible preparations as well as live routes.

use std::time::Duration;

use askama::Template;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::{ExtensionRegistryError, PluginScope};
use rbit_plugin_web::{
    CompiledPage, MAX_PAGE_PATH_BYTES, MAX_PAGE_ROUTES, PageAudience, PageDescriptor, PageError, PageRegistration, PageRegistry, PageScope,
    compiled_page,
};

#[derive(Template)]
#[template(source = "<p>Compiled page</p>", ext = "html")]
struct Fragment;

fn pages() -> Vec<PageRegistration> {
    (0..32)
        .map(|index| {
            let id = format!("page-{index}");
            let descriptor = PageDescriptor::new(&id, PageScope::Global, &id, PageAudience::Public, Duration::from_secs(1)).unwrap();
            compiled_page(descriptor, |_| async { CompiledPage::new("Page", Fragment) })
        })
        .collect()
}

#[test]
fn aggregate_budget_counts_staged_routes_and_recovers_after_revocation() {
    let registry = PageRegistry::new();
    let mut scopes = Vec::new();
    let mut registrations = Vec::new();
    for index in 0..4 {
        let scope = PluginScope::new(Generation::INITIAL);
        let owner = format!("example.budget-{index}").parse().unwrap();
        registrations.push(registry.stage(owner, &scope, scope.generation(), pages()).unwrap());
        scopes.push(scope);
    }
    assert!(registry.snapshot().is_empty(), "all 128 preparations are invisible");
    let extra_scope = PluginScope::new(Generation::new(2).unwrap());
    let extra_owner: PluginId = "example.extra".parse().unwrap();
    assert!(matches!(
        registry.stage(extra_owner.clone(), &extra_scope, extra_scope.generation(), pages()),
        Err(PageError::Registry(ExtensionRegistryError::Capacity))
    ));
    for registration in &registrations {
        registration.publish().unwrap();
    }
    let full = registry.snapshot();
    assert_eq!(full.len(), MAX_PAGE_ROUTES);
    let owner: PluginId = "example.budget-0".parse().unwrap();
    let retired = full.find(&owner, PageScope::Global, "page-0").unwrap();
    assert_eq!(scopes[0].in_flight(), 0, "lookup does not consume operation capacity");
    let full_revision = registry.revision();
    assert!(matches!(
        registry.stage(extra_owner.clone(), &extra_scope, extra_scope.generation(), pages()),
        Err(PageError::Registry(ExtensionRegistryError::Capacity))
    ));
    assert_eq!(registry.revision(), full_revision);

    scopes[0].quiesce();
    assert!(retired.admit().is_err());
    assert_eq!(registry.snapshot().len(), MAX_PAGE_ROUTES - 32);
    let replacement = registry.stage(extra_owner.clone(), &extra_scope, extra_scope.generation(), pages()).unwrap();
    replacement.publish().unwrap();
    assert_eq!(registry.snapshot().len(), MAX_PAGE_ROUTES);
    assert!(registry.snapshot().find(&extra_owner, PageScope::Global, "page-31").unwrap().admit().is_ok());
    assert!(!registrations[0].unpublish(), "retired registration cannot withdraw new routes");
}

#[test]
fn maximum_namespaced_targets_fit_the_ui_contract() {
    for invalid in ["a".repeat(MAX_PAGE_PATH_BYTES + 1), "a".repeat(33), ["a"; 9].join("/")] {
        assert!(PageDescriptor::new("invalid", PageScope::Global, &invalid, PageAudience::Public, Duration::from_secs(1)).is_err());
    }
    let owner: PluginId = format!("example.{}", "a".repeat(120)).parse().unwrap();
    assert_eq!(owner.as_str().len(), PluginId::MAX_LEN);
    let path = format!("{}/{}/{}", "a".repeat(32), "b".repeat(32), "c".repeat(30));
    assert_eq!(path.len(), MAX_PAGE_PATH_BYTES);
    for (scope, audience) in [(PageScope::Global, PageAudience::Public), (PageScope::Repository, PageAudience::RepositoryReader)] {
        let descriptor = PageDescriptor::new("maximum", scope, &path, audience, Duration::from_secs(1)).unwrap();
        let target = descriptor.target(&owner);
        let url = target.render(Some(&"u".repeat(39)), Some(&"r".repeat(100))).unwrap();
        assert!(url.ends_with(&path));
        assert!(!url.contains('{'));
    }
}
