use askama::Template;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::{DrainError, PluginScope};
use rbit_plugin_web::{
    CompiledPage, MAX_CONCURRENT_PAGES, MAX_PAGE_BODY_BYTES, PageActor, PageAudience, PageContext, PageDescriptor, PageError,
    PageRegistration, PageRegistry, PageRepository, PageScope, compiled_page, validate_page_path,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Template)]
#[template(source = "<p>{{ value }}</p>", ext = "html")]
struct Message {
    value: String,
}
fn owner() -> PluginId {
    "example.pages".parse().unwrap()
}
fn scope() -> PluginScope {
    PluginScope::new(Generation::INITIAL)
}
fn context() -> PageContext {
    PageContext { actor: None, repository: None, locale: LocaleSnapshot::english(0) }
}
fn descriptor(id: &str, path: &str) -> PageDescriptor {
    PageDescriptor::new(id, PageScope::Global, path, PageAudience::Public, Duration::from_secs(1)).unwrap()
}
fn page(id: &str, path: &str, text: &str) -> PageRegistration {
    let text = text.to_owned();
    compiled_page(descriptor(id, path), move |_| {
        let value = text.clone();
        async move { CompiledPage::new("Title", Message { value }) }
    })
}

#[test]
fn exact_routes_are_namespaced_and_cannot_claim_host_paths() {
    for path in ["", "/admin", "admin/", "a//b", "..", "a/../b", "a%2fb", "a?secret", "a#hash", "{owner}", "A", "a\\b"] {
        assert!(!validate_page_path(path), "{path}");
    }
    assert!(validate_page_path("nested/status"));
    assert_eq!(descriptor("global", "admin").target(&owner()).as_str(), "/-/plugins/example.pages/admin");
    let repo =
        PageDescriptor::new("repo", PageScope::Repository, "status", PageAudience::RepositoryReader, Duration::from_secs(1)).unwrap();
    assert_eq!(repo.target(&owner()).render(Some("alice"), Some("rocket")).unwrap(), "/alice/rocket/-/plugins/example.pages/status");
    assert!(PageDescriptor::new("bad", PageScope::Global, "status", PageAudience::RepositoryReader, Duration::from_secs(1)).is_err());
    assert!(PageDescriptor::new("bad", PageScope::Repository, "status", PageAudience::Public, Duration::from_secs(1)).is_err());
    assert!(PageDescriptor::new("bad", PageScope::Global, "status", PageAudience::Public, Duration::from_secs(2)).is_err());
}

#[tokio::test]
async fn compiled_template_escaping_and_atomic_lookup_replacement() {
    let registry = PageRegistry::new();
    let scope = scope();
    let registration =
        registry.stage(owner(), &scope, scope.generation(), vec![page("one", "first", "<script>alert(1)</script>")]).unwrap();
    assert!(registry.snapshot().find(&owner(), PageScope::Global, "first").is_none());
    registration.publish().unwrap();
    let snapshot = registry.snapshot();
    let handle = snapshot.find(&owner(), PageScope::Global, "first").unwrap();
    assert_eq!(scope.in_flight(), 0, "metadata lookup admits no callbacks");
    let mut lease = handle.admit().unwrap();
    let body = lease.render(context()).await.unwrap();
    assert!(body.html().contains("script") && body.html().contains("alert(1)"));
    assert!(!body.html().contains("<script>"));
    assert_eq!(body.title(), "Title");
    drop(lease);
    let revision = registry.revision();
    assert!(registration.replace(vec![page("same", "one", "x"), page("same", "two", "y")]).is_err());
    assert_eq!(registry.revision(), revision);
    registration.replace(vec![page("two", "second", "Updated")]).unwrap();
    assert!(handle.admit().is_err());
    assert!(registry.snapshot().find(&owner(), PageScope::Global, "first").is_none());
    let mut lease = registry.snapshot().find(&owner(), PageScope::Global, "second").unwrap().admit().unwrap();
    assert_eq!(lease.render(context()).await.unwrap().html(), "<p>Updated</p>");
    registration.unpublish();
    assert!(registry.snapshot().is_empty());
}

#[tokio::test]
async fn authorization_is_checked_before_factory_and_scope_families_have_independent_keys() {
    let registry = PageRegistry::new();
    let scope = scope();
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = Arc::clone(&calls);
    let global = compiled_page(
        PageDescriptor::new("same", PageScope::Global, "same", PageAudience::SiteAdmin, Duration::from_secs(1)).unwrap(),
        move |_| {
            factory_calls.fetch_add(1, Ordering::SeqCst);
            async { CompiledPage::new("Title", Message { value: "Admin".into() }) }
        },
    );
    let repo = compiled_page(
        PageDescriptor::new("same", PageScope::Repository, "same", PageAudience::RepositoryAdmin, Duration::from_secs(1)).unwrap(),
        |_| async { CompiledPage::new("Repo", Message { value: "Repository".into() }) },
    );
    registry.stage(owner(), &scope, scope.generation(), vec![global, repo]).unwrap().publish().unwrap();
    let mut global = registry.snapshot().find(&owner(), PageScope::Global, "same").unwrap().admit().unwrap();
    assert!(matches!(global.render(context()).await, Err(PageError::Forbidden)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let mut admin = context();
    admin.actor = Some(PageActor { account_id: 1, is_admin: true });
    global.render(admin).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut repo = registry.snapshot().find(&owner(), PageScope::Repository, "same").unwrap().admit().unwrap();
    assert!(matches!(repo.render(context()).await, Err(PageError::Forbidden)));
    let mut reader = context();
    reader.actor = Some(PageActor { account_id: 1, is_admin: false });
    reader.repository = Some(PageRepository { repository_id: 2, owner: "alice".into(), name: "rocket".into(), can_admin: false });
    assert!(matches!(repo.render(reader.clone()).await, Err(PageError::Forbidden)));
    reader.repository.as_mut().unwrap().can_admin = true;
    repo.render(reader).await.unwrap();
}

#[tokio::test]
async fn body_bounds_and_post_render_revocation_or_deadline_never_return_partial_success() {
    let registry = PageRegistry::new();
    let scope = scope();
    registry
        .stage(owner(), &scope, scope.generation(), vec![page("large", "large", &"x".repeat(MAX_PAGE_BODY_BYTES + 1))])
        .unwrap()
        .publish()
        .unwrap();
    let mut large = registry.snapshot().find(&owner(), PageScope::Global, "large").unwrap().admit().unwrap();
    assert!(matches!(large.render(context()).await, Err(PageError::Render)));
    let late = PageRegistry::new();
    let late_scope = PluginScope::new(Generation::new(2).unwrap());
    let delayed = compiled_page(
        PageDescriptor::new("late", PageScope::Global, "late", PageAudience::Public, Duration::from_millis(1)).unwrap(),
        |_| async {
            std::thread::sleep(Duration::from_millis(5));
            CompiledPage::new("Late", Message { value: "Late".into() })
        },
    );
    late.stage(owner(), &late_scope, late_scope.generation(), vec![delayed]).unwrap().publish().unwrap();
    let mut lease = late.snapshot().find(&owner(), PageScope::Global, "late").unwrap().admit().unwrap();
    assert!(matches!(lease.render(context()).await, Err(PageError::Timeout)));
    let revoked = PageRegistry::new();
    let revoked_scope = PluginScope::new(Generation::new(3).unwrap());
    let close = revoked_scope.clone();
    let closing = compiled_page(descriptor("close", "close"), move |_| {
        close.quiesce();
        async { CompiledPage::new("Closed", Message { value: "Never publish".into() }) }
    });
    revoked.stage(owner(), &revoked_scope, revoked_scope.generation(), vec![closing]).unwrap().publish().unwrap();
    let mut lease = revoked.snapshot().find(&owner(), PageScope::Global, "close").unwrap().admit().unwrap();
    assert!(matches!(lease.render(context()).await, Err(PageError::Unavailable)));
}

struct FutureDrop {
    scope: PluginScope,
    drops: Arc<AtomicUsize>,
}
impl Drop for FutureDrop {
    fn drop(&mut self) {
        assert!(self.scope.in_flight() > 0, "callback future drops before its operation lease");
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn replacement_cancels_pending_factory_before_releasing_its_lease() {
    let registry = PageRegistry::new();
    let scope = scope();
    let started = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let captured_scope = scope.clone();
    let started_factory = Arc::clone(&started);
    let factory_drops = Arc::clone(&drops);
    let pending = compiled_page(descriptor("pending", "pending"), move |_| {
        let guard = FutureDrop { scope: captured_scope.clone(), drops: Arc::clone(&factory_drops) };
        let started = Arc::clone(&started_factory);
        async move {
            let _guard = guard;
            started.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<()>().await;
            CompiledPage::new("Never", Message { value: "Never".into() })
        }
    });
    let registration = registry.stage(owner(), &scope, scope.generation(), vec![pending]).unwrap();
    registration.publish().unwrap();
    let mut lease = registry.snapshot().find(&owner(), PageScope::Global, "pending").unwrap().admit().unwrap();
    let task = tokio::spawn(async move {
        let result = lease.render(context()).await;
        (result, lease)
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("factory should start");
    registration.replace(vec![page("new", "new", "New")]).unwrap();
    let (result, lease) =
        tokio::time::timeout(Duration::from_secs(2), task).await.expect("replacement must cancel the pending factory").unwrap();
    assert!(matches!(result, Err(PageError::Unavailable)));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(scope.in_flight(), 1);
    drop(lease);
    assert_eq!(scope.in_flight(), 0);
    scope.dispose(Duration::from_secs(1)).await.unwrap();
    assert!(registry.snapshot().is_empty());
}

#[tokio::test]
async fn response_operation_retains_capacity_and_scope_ownership_until_body_drop() {
    let registry = PageRegistry::new();
    let scope = scope();
    registry.stage(owner(), &scope, scope.generation(), vec![page("page", "page", "Body")]).unwrap().publish().unwrap();
    let handle = registry.snapshot().find(&owner(), PageScope::Global, "page").unwrap();
    let mut leases = Vec::new();
    for _ in 0..MAX_CONCURRENT_PAGES {
        leases.push(handle.admit().unwrap());
    }
    assert!(matches!(handle.admit(), Err(PageError::Unavailable)));
    let mut first = leases.pop().unwrap();
    first.render(context()).await.unwrap();
    let (operation, cancellation) = first.into_parts();
    assert!(matches!(handle.admit(), Err(PageError::Unavailable)));
    drop(leases);
    assert_eq!(scope.in_flight(), 1);
    assert!(matches!(scope.dispose(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 1 })));
    assert!(cancellation.is_cancelled());
    drop(operation);
    scope.dispose(Duration::from_secs(1)).await.unwrap();
}

#[tokio::test]
async fn aborting_a_caller_drops_its_factory_before_authority_and_reuses_capacity() {
    let registry = PageRegistry::new();
    let scope = scope();
    let started = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let captured_scope = scope.clone();
    let started_factory = Arc::clone(&started);
    let factory_drops = Arc::clone(&drops);
    let pending = compiled_page(descriptor("pending", "pending"), move |_| {
        let guard = FutureDrop { scope: captured_scope.clone(), drops: Arc::clone(&factory_drops) };
        let started = Arc::clone(&started_factory);
        async move {
            let _guard = guard;
            started.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<()>().await;
            CompiledPage::new("Never", Message { value: "Never".into() })
        }
    });
    let registration = registry.stage(owner(), &scope, scope.generation(), vec![pending]).unwrap();
    registration.publish().unwrap();
    let mut lease = registry.snapshot().find(&owner(), PageScope::Global, "pending").unwrap().admit().unwrap();
    let task = tokio::spawn(async move { lease.render(context()).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap_err().is_cancelled());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(scope.in_flight(), 0);
    registration.replace(vec![page("new", "new", "New")]).unwrap();
    let handle = registry.snapshot().find(&owner(), PageScope::Global, "new").unwrap();
    let leases: Vec<_> = (0..MAX_CONCURRENT_PAGES).map(|_| handle.admit().unwrap()).collect();
    assert!(handle.admit().is_err());
    drop(leases);
    assert_eq!(scope.in_flight(), 0);
}

#[test]
fn total_staged_route_capacity_and_maximum_target_lengths_are_bounded() {
    let registry = PageRegistry::new();
    let scope = scope();
    let mut registrations = Vec::new();
    for index in 0..4 {
        let pages = (0..32).map(|page_index| page(&format!("page-{page_index}"), &format!("page-{page_index}"), "Value")).collect();
        registrations.push(registry.stage(format!("example.owner-{index}").parse().unwrap(), &scope, scope.generation(), pages).unwrap());
    }
    assert!(registry.stage("example.overflow".parse().unwrap(), &scope, scope.generation(), vec![page("one", "one", "x")]).is_err());
    assert!(registry.snapshot().is_empty());
    registrations[0].publish().unwrap();
    assert_eq!(registry.snapshot().len(), 32);
    let longest = format!("{}/{}/{}", "a".repeat(32), "b".repeat(32), "c".repeat(30));
    let owner: PluginId = format!("example.{}", "a".repeat(120)).parse().unwrap();
    let descriptor =
        PageDescriptor::new("max", PageScope::Repository, &longest, PageAudience::RepositoryReader, Duration::from_secs(1)).unwrap();
    assert!(descriptor.target(&owner).as_str().len() <= 256);
    for invalid in ["a".repeat(97), "a".repeat(33), ["a"; 9].join("/")] {
        assert!(!validate_page_path(&invalid));
    }
}
