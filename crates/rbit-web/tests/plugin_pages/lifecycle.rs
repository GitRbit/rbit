//! Actual HTTP lifecycle coverage, using the same already-bound host router.

use super::*;
use rbit_plugin_api::{UiContent, UiContribution, UiContributionSet, UiMessage, UiNoticeTone, UiSlot, UiVisibility};
use rbit_services::plugin_pages::PageError;

#[tokio::test]
async fn runtime_route_registration_replaces_and_reenables_without_rebinding_or_stale_authority() {
    let fixture = Fixture::new().await;
    let registry = fixture.services.page_registry();
    let first_scope = scope(8100);
    let calls = Arc::default();
    let contexts = Arc::default();
    let owner: rbit_plugin_api::PluginId = "example.lifecycle".parse().unwrap();
    let old = registry
        .stage(
            owner.clone(),
            &first_scope,
            first_scope.generation(),
            vec![page("first", PageScope::Global, PageAudience::Public, "first generation", &calls, &contexts)],
        )
        .unwrap();
    assert_eq!(fixture.get("/-/plugins/example.lifecycle/first", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    old.publish().unwrap();
    let stale = registry.snapshot().find(&owner, PageScope::Global, "first").unwrap();
    let (status, headers, html) = fixture.get("/-/plugins/example.lifecycle/first", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_host_headers(&headers);
    assert!(html.contains("first generation"));
    old.replace(vec![page("second", PageScope::Global, PageAudience::Public, "atomic replacement", &calls, &contexts)]).unwrap();
    assert!(stale.admit().is_err());
    assert_eq!(fixture.get("/-/plugins/example.lifecycle/first", None).await.0, StatusCode::NOT_FOUND);
    let (status, _, html) = fixture.get("/-/plugins/example.lifecycle/second", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("atomic replacement"));
    first_scope.dispose(Duration::from_secs(2)).await.unwrap();
    assert_eq!(fixture.get("/-/plugins/example.lifecycle/second", None).await.0, StatusCode::NOT_FOUND);

    let next_scope = scope(8101);
    let next = registry
        .stage(
            owner,
            &next_scope,
            next_scope.generation(),
            vec![page("second", PageScope::Global, PageAudience::Public, "new activation", &calls, &contexts)],
        )
        .unwrap();
    next.publish().unwrap();
    assert!(!old.unpublish(), "retired registration cannot withdraw a new generation");
    assert!(old.publish().is_err());
    assert!(stale.admit().is_err());
    let (status, _, html) = fixture.get("/-/plugins/example.lifecycle/second", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("new activation"));
    next_scope.dispose(Duration::from_secs(2)).await.unwrap();
    fixture.services.shutdown().await;
}

struct FactoryDrop {
    scope: PluginScope,
    drops: Arc<AtomicUsize>,
    released_early: Arc<AtomicUsize>,
}
impl Drop for FactoryDrop {
    fn drop(&mut self) {
        if self.scope.in_flight() == 0 {
            self.released_early.fetch_add(1, Ordering::SeqCst);
        }
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn pending_page(
    scope: &PluginScope,
    started: &Arc<AtomicUsize>,
    drops: &Arc<AtomicUsize>,
    released_early: &Arc<AtomicUsize>,
    deadline: Duration,
) -> PageRegistration {
    let scope = scope.clone();
    let started = Arc::clone(started);
    let drops = Arc::clone(drops);
    let released_early = Arc::clone(released_early);
    compiled_page(PageDescriptor::new("pending", PageScope::Global, "pending", PageAudience::Public, deadline).unwrap(), move |_| {
        let guard = FactoryDrop { scope: scope.clone(), drops: Arc::clone(&drops), released_early: Arc::clone(&released_early) };
        let started = Arc::clone(&started);
        async move {
            let _guard = guard;
            started.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<Result<CompiledPage<Fragment>, PageError>>().await
        }
    })
}

async fn pending_http(router: axum::Router) -> axum::response::Response {
    router
        .oneshot(
            Request::builder()
                .uri("/-/plugins/example.pending/pending")
                .header(header::HOST, "localhost:3000")
                .header(header::ORIGIN, "https://localhost:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn wait_for_start(started: &AtomicUsize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("HTTP request must enter its factory");
}

#[tokio::test]
async fn runtime_route_registration_cancels_pending_http_factories_and_caller_aborts_without_leaks() {
    let fixture = Fixture::new().await;
    for (generation, caller_abort) in [(8110, false), (8111, true)] {
        let scope = scope(generation);
        let started = Arc::default();
        let drops = Arc::default();
        let released_early = Arc::default();
        fixture
            .services
            .page_registry()
            .stage(
                "example.pending".parse().unwrap(),
                &scope,
                scope.generation(),
                vec![pending_page(&scope, &started, &drops, &released_early, Duration::from_secs(1))],
            )
            .unwrap()
            .publish()
            .unwrap();
        let request = tokio::spawn(pending_http(fixture.router.clone()));
        wait_for_start(&started).await;
        assert_eq!(scope.in_flight(), 1);
        if caller_abort {
            request.abort();
            assert!(tokio::time::timeout(Duration::from_secs(2), request).await.unwrap().unwrap_err().is_cancelled());
        } else {
            scope.quiesce();
            let response = tokio::time::timeout(Duration::from_secs(2), request).await.unwrap().unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_host_headers(response.headers());
            let body = to_bytes(response.into_body(), 1024).await.unwrap();
            assert_eq!(body.as_ref(), b"Plugin page temporarily unavailable.");
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(released_early.load(Ordering::SeqCst), 0, "factory future drops before operation authority");
        assert_eq!(scope.in_flight(), 0);
        scope.dispose(Duration::from_secs(2)).await.unwrap();
    }
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn runtime_route_registration_timeout_returns_only_host_failure_and_releases_authority() {
    let fixture = Fixture::new().await;
    let scope = scope(8120);
    let started = Arc::default();
    let drops = Arc::default();
    let released_early = Arc::default();
    fixture
        .services
        .page_registry()
        .stage(
            "example.pending".parse().unwrap(),
            &scope,
            scope.generation(),
            vec![pending_page(&scope, &started, &drops, &released_early, Duration::from_millis(20))],
        )
        .unwrap()
        .publish()
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(2), pending_http(fixture.router.clone())).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_host_headers(response.headers());
    assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap().as_ref(), b"Plugin page temporarily unavailable.");
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(released_early.load(Ordering::SeqCst), 0);
    assert_eq!(scope.in_flight(), 0);
    scope.dispose(Duration::from_secs(2)).await.unwrap();
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn runtime_route_registration_revision_refresh_keeps_host_security_and_discards_old_markup() {
    let fixture = Fixture::new().await;
    let page_scope = scope(8130);
    let ui_scope = scope(8131);
    let changed_ui = fixture
        .services
        .ui_registry()
        .stage(
            "example.revision".parse().unwrap(),
            &ui_scope,
            ui_scope.generation(),
            UiContributionSet::new(vec![UiContribution {
                id: "revision".parse().unwrap(),
                slot: UiSlot::AdminPanel,
                order: 0,
                visibility: UiVisibility::InstanceAdmin,
                content: UiContent::Notice {
                    message: UiMessage { namespace: "example.revision".into(), key: "notice".into(), fallback: "New revision".into() },
                    tone: UiNoticeTone::Info,
                },
            }]),
        )
        .unwrap();
    let factory = compiled_page(
        PageDescriptor::new("race", PageScope::Global, "race", PageAudience::Public, Duration::from_millis(100)).unwrap(),
        move |_| {
            let changed_ui = changed_ui.clone();
            async move {
                changed_ui.publish().map_err(|_| PageError::Unavailable)?;
                CompiledPage::new("Revision race", Fragment { text: "old representation must be discarded".into() })
            }
        },
    );
    fixture
        .services
        .page_registry()
        .stage("example.race".parse().unwrap(), &page_scope, page_scope.generation(), vec![factory])
        .unwrap()
        .publish()
        .unwrap();
    let response = fixture.request(Method::GET, "/-/plugins/example.race/race", None, &[]).await;
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers()[header::LOCATION], "/-/plugins/example.race/race");
    assert_eq!(response.headers()["hx-refresh"], "true");
    assert_host_headers(response.headers());
    assert_eq!(response.headers()["rbit-plugin-revision"], fixture.services.plugin_revision().to_string());
    let body = String::from_utf8(to_bytes(response.into_body(), 1024).await.unwrap().to_vec()).unwrap();
    assert!(!body.contains("old representation must be discarded"));
    assert_eq!(page_scope.in_flight(), 0, "discarding a stale page also drops its response operation");
    let (status, headers, html) = fixture.get("/-/plugins/example.race/race", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_host_headers(&headers);
    assert!(html.contains("old representation must be discarded"));
    page_scope.dispose(Duration::from_secs(2)).await.unwrap();
    ui_scope.dispose(Duration::from_secs(2)).await.unwrap();
    fixture.services.shutdown().await;
}
