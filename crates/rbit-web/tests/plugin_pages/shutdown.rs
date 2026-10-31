//! Host shutdown fences plugin work before HTTP drain, including stalled peers.

use super::*;
use rbit_services::request_hooks::{HookDecision, HookFuture, HookRegistration, RequestHook, RequestMetadata};

async fn wait_for_count(value: &AtomicUsize, expected: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while value.load(Ordering::SeqCst) != expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn runtime_route_registration_shutdown_drains_unread_full_and_partial_responses_and_fences_clones() {
    let fixture = Fixture::new().await;
    #[cfg(feature = "actions")]
    {
        fixture.services.set_actions_enabled(&fixture.admin, true).await.unwrap();
        assert!(fixture.services.actions.is_some());
    }
    #[cfg(feature = "i18n")]
    {
        fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
        assert!(fixture.services.i18n_enabled());
    }
    let scope = scope(8200);
    let calls = Arc::default();
    let contexts = Arc::default();
    let registry = fixture.services.page_registry();
    let cloned_registry = registry.clone();
    let owner = "example.shutdown".parse().unwrap();
    let registration = registry
        .stage(
            owner,
            &scope,
            scope.generation(),
            vec![page("read", PageScope::Repository, PageAudience::RepositoryReader, "shutdown private output", &calls, &contexts)],
        )
        .unwrap();
    registration.publish().unwrap();
    let stale = registry.snapshot().find(&"example.shutdown".parse().unwrap(), PageScope::Repository, "read").unwrap();
    let mut responses = Vec::new();
    for index in 0..64 {
        let headers = if index % 2 == 0 { vec![("HX-Request", "true"), ("Rbit-Partial", "/alice/rocket")] } else { Vec::new() };
        let response = fixture.request(Method::GET, "/alice/rocket/-/plugins/example.shutdown/read", None, &headers).await;
        assert_eq!(response.status(), StatusCode::OK);
        responses.push(response);
    }
    assert_eq!(scope.in_flight(), 64);
    assert_eq!(calls.load(Ordering::SeqCst), 64);
    assert_eq!(fixture.get("/alice/rocket/-/plugins/example.shutdown/read", None).await.0, StatusCode::SERVICE_UNAVAILABLE);
    let services_clone = fixture.services.clone();
    fixture.services.prepare_shutdown();
    services_clone.prepare_shutdown();
    tokio::time::timeout(Duration::from_secs(2), async {
        while scope.in_flight() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("host shutdown cancels all unread responses without polling the peers");
    for response in responses {
        assert!(to_bytes(response.into_body(), 1024 * 1024).await.is_err());
    }
    assert!(stale.admit().is_err());
    assert!(registration.publish().is_err());
    assert!(
        registration
            .replace(vec![page("read", PageScope::Repository, PageAudience::RepositoryReader, "replacement", &calls, &contexts)])
            .is_err()
    );
    let next_scope = super::scope(8201);
    assert!(
        cloned_registry
            .stage(
                "example.restart".parse().unwrap(),
                &next_scope,
                next_scope.generation(),
                vec![page("new", PageScope::Global, PageAudience::Public, "new", &calls, &contexts)],
            )
            .is_err()
    );
    assert_eq!(fixture.get("/alice/rocket/-/plugins/example.shutdown/read", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(calls.load(Ordering::SeqCst), 64, "closed services cannot invoke any more plugin renderers");
    assert!(services_clone.actions.is_none());
    assert!(!services_clone.i18n_enabled());
    #[cfg(feature = "actions")]
    assert!(services_clone.set_actions_enabled(&fixture.admin, true).await.is_err());
    #[cfg(feature = "i18n")]
    assert!(services_clone.set_i18n_enabled(&fixture.admin, true).await.is_err());
    assert_eq!(fixture.get("/explore", None).await.0, StatusCode::OK);
    assert_eq!(fixture.get("/admin/plugins", Some(&fixture.admin_cookie)).await.0, StatusCode::OK);
    assert_eq!(fixture.get("/login", None).await.0, StatusCode::OK);
    fixture.services.shutdown().await;
}

#[derive(Debug)]
struct PendingHook {
    started: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}
struct PendingDrop(Arc<AtomicUsize>);
impl Drop for PendingDrop {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl RequestHook for PendingHook {
    fn call<'a>(&'a self, _: &'a RequestMetadata) -> HookFuture<'a> {
        Box::pin(async move {
            let _drop = PendingDrop(Arc::clone(&self.dropped));
            self.started.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<HookDecision>().await
        })
    }
}

async fn public_request(router: axum::Router, path: &str) -> axum::response::Response {
    router.oneshot(Request::builder().uri(path).header(header::HOST, "localhost:3000").body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn runtime_route_registration_shutdown_cancels_pending_http_page_and_hook_callbacks() {
    let fixture = Fixture::new().await;
    let page_scope = scope(8210);
    let hook_scope = scope(8211);
    let page_started = Arc::new(AtomicUsize::new(0));
    let page_dropped = Arc::new(AtomicUsize::new(0));
    let factory = {
        let started = Arc::clone(&page_started);
        let dropped = Arc::clone(&page_dropped);
        compiled_page(
            PageDescriptor::new("pending", PageScope::Global, "pending", PageAudience::Public, Duration::from_secs(1)).unwrap(),
            move |_| {
                let started = Arc::clone(&started);
                let dropped = Arc::clone(&dropped);
                async move {
                    let _drop = PendingDrop(dropped);
                    started.fetch_add(1, Ordering::SeqCst);
                    std::future::pending::<Result<CompiledPage<Fragment>, rbit_services::plugin_pages::PageError>>().await
                }
            },
        )
    };
    fixture
        .services
        .page_registry()
        .stage("example.shutdown".parse().unwrap(), &page_scope, page_scope.generation(), vec![factory])
        .unwrap()
        .publish()
        .unwrap();
    let page_request = tokio::spawn(public_request(fixture.router.clone(), "/-/plugins/example.shutdown/pending"));
    wait_for_count(&page_started, 1).await;
    let hook_started = Arc::new(AtomicUsize::new(0));
    let hook_dropped = Arc::new(AtomicUsize::new(0));
    let hooks = fixture.services.request_hooks();
    let hook_registration = hooks
        .stage(
            "example.shutdown".parse().unwrap(),
            &hook_scope,
            hook_scope.generation(),
            vec![
                HookRegistration::new(
                    "pending",
                    0,
                    Duration::from_millis(100),
                    Arc::new(PendingHook { started: Arc::clone(&hook_started), dropped: Arc::clone(&hook_dropped) }),
                )
                .unwrap(),
            ],
        )
        .unwrap();
    hook_registration.publish().unwrap();
    let stale_hooks = hooks.snapshot();
    let hook_request = tokio::spawn(public_request(fixture.router.clone(), "/explore"));
    wait_for_count(&hook_started, 1).await;
    assert_eq!(page_scope.in_flight(), 1);
    assert_eq!(hook_scope.in_flight(), 1);
    fixture.services.prepare_shutdown();
    for request in [page_request, hook_request] {
        let response = tokio::time::timeout(Duration::from_millis(500), request).await.unwrap().unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
    assert_eq!(page_dropped.load(Ordering::SeqCst), 1);
    assert_eq!(hook_dropped.load(Ordering::SeqCst), 1);
    assert_eq!(page_scope.in_flight(), 0);
    assert_eq!(hook_scope.in_flight(), 0);
    assert!(hook_registration.publish().is_err());
    assert!(
        hooks
            .stage(
                "example.restart".parse().unwrap(),
                &hook_scope,
                hook_scope.generation(),
                vec![
                    HookRegistration::new(
                        "new",
                        0,
                        Duration::from_millis(100),
                        Arc::new(PendingHook { started: Arc::clone(&hook_started), dropped: Arc::clone(&hook_dropped) }),
                    )
                    .unwrap()
                ]
            )
            .is_err()
    );
    let metadata = RequestMetadata::new(rbit_services::request_hooks::RequestMethod::Get, "/explore").unwrap();
    assert!(matches!(stale_hooks.evaluate(&metadata).await, HookDecision::Reject(_)));
    assert_eq!(hook_started.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.get("/explore", None).await.0, StatusCode::OK);
    assert_eq!(fixture.get("/admin/plugins", Some(&fixture.admin_cookie)).await.0, StatusCode::OK);
    fixture.services.shutdown().await;
}
