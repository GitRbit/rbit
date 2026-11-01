use askama::Template;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Template)]
#[template(source = "<p>Example</p>", ext = "html")]
struct Example;
fn owner() -> PluginId {
    "example.bundle".parse().unwrap()
}
fn scope() -> PluginScope {
    PluginScope::new(Generation::INITIAL)
}
fn page() -> PageRegistration {
    let descriptor = PageDescriptor::new("page", PageScope::Global, "page", PageAudience::Public, Duration::from_secs(1)).unwrap();
    compiled_page(descriptor, |_| async { CompiledPage::new("Example", Example) })
}
fn mutation() -> MutationRegistration {
    let descriptor = MutationDescriptor::new(
        "save",
        PageScope::Global,
        "save",
        MutationAudience::SignedIn,
        Duration::from_secs(1),
        FormSchema::new(Vec::new()).unwrap(),
    )
    .unwrap();
    compiled_mutation(descriptor, |_, _| async { Ok(MutationOutcome::<Example>::Redirect(PluginRedirect::new("page")?)) })
}
fn bundle() -> RouteBundle {
    RouteBundle { pages: vec![page()], mutations: vec![mutation()] }
}
fn page_handle(registry: &PageRegistry) -> Option<PageRouteHandle> {
    registry.snapshot().find(&owner(), PageScope::Global, "page")
}
fn mutation_handle(registry: &MutationRegistry) -> Option<MutationRouteHandle> {
    registry.snapshot().find(&owner(), PageScope::Global, "save")
}

#[test]
fn staging_is_invisible_and_withdrawal_revokes_retained_handles_without_closing_scope() {
    let (pages, mutations, scope) = (PageRegistry::new(), MutationRegistry::new(), scope());
    let routes = bundle().stage(owner(), &scope, &pages, &mutations).unwrap();
    assert_eq!(pages.snapshot().len(), 0);
    assert_eq!(mutations.snapshot().len(), 0);
    routes.publish().unwrap();
    let page = page_handle(&pages).unwrap();
    let mutation = mutation_handle(&mutations).unwrap();
    drop(page.admit().unwrap());
    drop(mutation.admit().unwrap());
    routes.withdraw();
    routes.withdraw();
    assert!(page.admit().is_err());
    assert!(mutation.admit().is_err());
    assert!(!scope.is_closed());
    drop(scope.try_acquire(scope.generation()).unwrap());
    assert!(routes.publish().is_err(), "withdrawal is terminal for these handles");
    bundle().stage(owner(), &scope, &pages, &mutations).unwrap().publish().unwrap();
}

#[test]
fn failed_first_stage_never_reserves_the_mutation_owner() {
    let (pages, mutations, scope) = (PageRegistry::new(), MutationRegistry::new(), scope());
    let occupied = pages.stage(owner(), &scope, scope.generation(), vec![page()]).unwrap();
    assert!(matches!(bundle().stage(owner(), &scope, &pages, &mutations), Err(RouteBundleError::Page(_))));
    let mutation = mutations.stage(owner(), &scope, scope.generation(), vec![mutation()]).unwrap();
    mutation.publish().unwrap();
    occupied.unpublish();
}

#[test]
fn failed_second_stage_releases_the_staged_page_owner_for_retry() {
    let (pages, mutations, scope) = (PageRegistry::new(), MutationRegistry::new(), scope());
    let occupied = mutations.stage(owner(), &scope, scope.generation(), vec![mutation()]).unwrap();
    assert!(matches!(bundle().stage(owner(), &scope, &pages, &mutations), Err(RouteBundleError::Mutation(_))));
    // No page was visible even before rollback: a fresh reservation, not a
    // snapshot-length assertion, proves the failed attempt released its owner.
    let retry = RouteBundle { pages: vec![page()], mutations: Vec::new() }.stage(owner(), &scope, &pages, &mutations).unwrap();
    retry.publish().unwrap();
    assert!(page_handle(&pages).is_some());
    assert!(!scope.is_closed());
    occupied.unpublish();
}

#[test]
fn dropping_registration_handles_preserves_scope_ownership_and_management_is_independent() {
    let (pages, mutations, active) = (PageRegistry::new(), MutationRegistry::new(), scope());
    let management = scope();
    let admin: PluginId = "example.management".parse().unwrap();
    let routes = bundle().stage(owner(), &active, &pages, &mutations).unwrap();
    routes.publish().unwrap();
    let admin_routes = bundle().stage(admin.clone(), &management, &pages, &mutations).unwrap();
    admin_routes.publish().unwrap();
    drop(routes.clone());
    drop(routes);
    let retained = page_handle(&pages).unwrap();
    drop(retained.admit().unwrap());
    active.quiesce();
    assert!(retained.admit().is_err());
    drop(pages.snapshot().find(&admin, PageScope::Global, "page").unwrap().admit().unwrap());
    drop(mutations.snapshot().find(&admin, PageScope::Global, "save").unwrap().admit().unwrap());
    management.quiesce();
    assert_eq!(pages.snapshot().len(), 0);
    assert_eq!(mutations.snapshot().len(), 0);
}

#[test]
fn empty_bundles_do_not_reserve_or_close_unrelated_registries() {
    let (pages, mutations, scope) = (PageRegistry::new(), MutationRegistry::new(), scope());
    let empty = RouteBundle::default().stage(owner(), &scope, &pages, &mutations).unwrap();
    empty.publish().unwrap();
    empty.withdraw();
    let routes = bundle().stage(owner(), &scope, &pages, &mutations).unwrap();
    routes.publish().unwrap();
    assert!(!scope.is_closed());
}

struct ReentrantDrop {
    pages: PageRegistry,
    scope: PluginScope,
    drops: Arc<AtomicUsize>,
}
impl Drop for ReentrantDrop {
    fn drop(&mut self) {
        let _ = self.pages.snapshot();
        self.scope.quiesce();
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn rollback_destroys_callbacks_outside_registry_and_scope_locks() {
    let (done, completed) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (pages, mutations, scope) = (PageRegistry::new(), MutationRegistry::new(), scope());
        let drops = Arc::new(AtomicUsize::new(0));
        let probe = ReentrantDrop { pages: pages.clone(), scope: scope.clone(), drops: Arc::clone(&drops) };
        let descriptor = PageDescriptor::new("page", PageScope::Global, "page", PageAudience::Public, Duration::from_secs(1)).unwrap();
        let callback = compiled_page(descriptor, move |_| {
            let _ = &probe;
            async { CompiledPage::new("Example", Example) }
        });
        mutations.close();
        let failed = RouteBundle { pages: vec![callback], mutations: vec![mutation()] }.stage(owner(), &scope, &pages, &mutations);
        assert!(matches!(failed, Err(RouteBundleError::Mutation(_))));
        done.send(drops.load(Ordering::SeqCst)).unwrap();
    });
    assert_eq!(completed.recv_timeout(Duration::from_secs(2)).expect("rollback must not deadlock callback destruction"), 1);
}
