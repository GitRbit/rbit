//! Runtime pages stay inside the forge's authorization and rendering boundaries.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use askama::Template;
use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use rbit_core::{Config, Visibility};
use rbit_plugin_api::Generation;
use rbit_plugin_runtime::PluginScope;
use rbit_services::{
    CreateRepository, Services, SignUp,
    plugin_pages::{CompiledPage, PageAudience, PageContext, PageDescriptor, PageRegistration, PageScope, compiled_page},
};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt as _;

struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    database: Database,
    router: axum::Router,
    admin: rbit_core::User,
    user: rbit_core::User,
    admin_cookie: String,
    user_cookie: String,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.updates.check_for_updates = false;
        "https://localhost:3000".clone_into(&mut config.server.public_url);
        config.security.hsts = true;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let admin = services
            .accounts
            .create_user(
                SignUp { username: "alice".into(), email: "alice@example.com".into(), password: "correct horse battery".into() },
                Some(true),
            )
            .await
            .unwrap();
        let user = services
            .accounts
            .create_user(
                SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
                Some(false),
            )
            .await
            .unwrap();
        for (name, visibility) in [("rocket", Visibility::Public), ("secret", Visibility::Private)] {
            services
                .repositories
                .create(&admin, CreateRepository { name: name.into(), description: String::new(), visibility })
                .await
                .unwrap();
        }
        let cookie = |token: rbit_auth::SessionToken| format!("rbit_session={}", token.encode());
        let admin_cookie = cookie(services.accounts.start_session(&admin).await.unwrap());
        let user_cookie = cookie(services.accounts.start_session(&user).await.unwrap());
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _directory: directory, services, database, router, admin, user, admin_cookie, user_cookie }
    }
    async fn request(&self, method: Method, path: &str, cookie: Option<&str>, extra: &[(&str, &str)]) -> axum::response::Response {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "https://localhost:3000");
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        for (name, value) in extra {
            request
                .headers_mut()
                .unwrap()
                .insert(axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(), axum::http::HeaderValue::from_str(value).unwrap());
        }
        self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
    }
    async fn get(&self, path: &str, cookie: Option<&str>) -> (StatusCode, HeaderMap, String) {
        let response = self.request(Method::GET, path, cookie, &[]).await;
        let (parts, body) = response.into_parts();
        let body = String::from_utf8(to_bytes(body, 2 * 1024 * 1024).await.unwrap().to_vec()).unwrap();
        (parts.status, parts.headers, body)
    }
}

#[derive(Template)]
#[template(source = "<p>{{ text }}</p>", ext = "html")]
struct Fragment {
    text: String,
}
fn scope(generation: u64) -> PluginScope {
    PluginScope::new(Generation::new(generation).unwrap())
}
fn page(
    path: &str,
    scope: PageScope,
    audience: PageAudience,
    text: &str,
    calls: &Arc<AtomicUsize>,
    contexts: &Arc<Mutex<Vec<PageContext>>>,
) -> PageRegistration {
    let descriptor = PageDescriptor::new(path.replace('/', "-").as_str(), scope, path, audience, Duration::from_millis(100)).unwrap();
    let text = text.to_owned();
    let calls = Arc::clone(calls);
    let contexts = Arc::clone(contexts);
    compiled_page(descriptor, move |context| {
        let calls = Arc::clone(&calls);
        let contexts = Arc::clone(&contexts);
        let text = text.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            contexts.lock().unwrap().push(context);
            CompiledPage::new("Runtime <title>", Fragment { text })
        }
    })
}

fn assert_host_headers(headers: &HeaderMap) {
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
    assert!(headers.contains_key(header::STRICT_TRANSPORT_SECURITY));
    assert!(headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("frame-ancestors 'none'"));
}

#[tokio::test]
async fn runtime_route_registration_enforces_host_actor_and_repository_access() {
    let fixture = Fixture::new().await;
    let scope = scope(1);
    let calls = Arc::default();
    let contexts = Arc::default();
    let registration = fixture
        .services
        .page_registry()
        .stage(
            "example.pages".parse().unwrap(),
            &scope,
            scope.generation(),
            vec![
                page("public", PageScope::Global, PageAudience::Public, "<script>private text</script>", &calls, &contexts),
                page("signed", PageScope::Global, PageAudience::SignedIn, "signed", &calls, &contexts),
                page("admin", PageScope::Global, PageAudience::SiteAdmin, "admin", &calls, &contexts),
                page("read", PageScope::Repository, PageAudience::RepositoryReader, "reader", &calls, &contexts),
                page("settings", PageScope::Repository, PageAudience::RepositoryAdmin, "manager", &calls, &contexts),
            ],
        )
        .unwrap();
    assert_eq!(fixture.get("/-/plugins/example.pages/public", None).await.0, StatusCode::NOT_FOUND);
    registration.publish().unwrap();
    let (status, headers, html) = fixture.get("/-/plugins/example.pages/public", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_host_headers(&headers);
    assert!(html.contains("<html lang=\"en\" dir=\"ltr\""));
    assert!(html.contains("hx-history=\"false\""));
    assert!(html.contains("&#60;script&#62;private text&#60;/script&#62;"));
    assert!(html.contains("Runtime &#60;title&#62;"));
    assert!(!html.contains("<script>private text</script>"));
    assert_eq!(fixture.get("/-/plugins/example.pages/signed", None).await.0, StatusCode::SEE_OTHER);
    assert_eq!(fixture.get("/-/plugins/example.pages/signed", Some(&fixture.user_cookie)).await.0, StatusCode::OK);
    assert_eq!(fixture.get("/-/plugins/example.pages/admin", Some(&fixture.user_cookie)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(fixture.get("/-/plugins/example.pages/admin", Some(&fixture.admin_cookie)).await.0, StatusCode::OK);
    let before = calls.load(Ordering::SeqCst);
    for cookie in [None, Some(fixture.user_cookie.as_str())] {
        assert_eq!(fixture.get("/alice/secret/-/plugins/example.pages/read", cookie).await.0, StatusCode::NOT_FOUND);
    }
    assert_eq!(fixture.get("/alice/rocket/-/plugins/example.pages/settings", Some(&fixture.user_cookie)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(calls.load(Ordering::SeqCst), before, "privacy and manager gates run before callbacks");
    let (_, _, html) = fixture.get("/alice/rocket/-/plugins/example.pages/read", None).await;
    assert!(html.contains("id=\"repo-subnav\""));
    assert_eq!(fixture.get("/alice/secret/-/plugins/example.pages/settings", Some(&fixture.admin_cookie)).await.0, StatusCode::OK);
    {
        let captured = contexts.lock().unwrap();
        assert!(
            captured.iter().any(|context| context.actor.as_ref().is_some_and(|actor| actor.account_id == fixture.user.id().0)
                && context.repository.is_none())
        );
        assert!(captured.iter().any(|context| context.actor.as_ref().is_some_and(|actor| actor.account_id == fixture.admin.id().0)
            && context.repository.as_ref().is_some_and(|repo| repo.name == "secret" && repo.can_admin)));
    }
    scope.quiesce();
    assert_eq!(fixture.get("/-/plugins/example.pages/public", None).await.0, StatusCode::NOT_FOUND);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn runtime_route_registration_is_read_only_canonical_and_never_preloaded() {
    let fixture = Fixture::new().await;
    let scope = scope(2);
    let calls = Arc::default();
    let contexts = Arc::default();
    let registration = fixture
        .services
        .page_registry()
        .stage(
            "example.pages".parse().unwrap(),
            &scope,
            scope.generation(),
            vec![page("nested/report", PageScope::Global, PageAudience::Public, "readonly", &calls, &contexts)],
        )
        .unwrap();
    registration.publish().unwrap();
    for path in [
        "/-/plugins/example%2epages/nested/report",
        "/-/plugins/example.pages/nested%2freport",
        "/-/plugins/example.pages/nested/%2e%2e/report",
        "/-/plugins/example.pages/nested/%00",
        "/-/plugins/example.pages/nested/%FF",
        "/-/plugins/example.pages/nested/UNKNOWN",
        "/-/plugins/example.pages/missing",
        "/-/plugins/example.pages",
    ] {
        let response = fixture.request(Method::GET, path, None, &[]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store", "{path}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let response = fixture.request(Method::GET, "/-/plugins/example.pages/nested/report", None, &[("HX-Preloaded", "true")]).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    for method in [Method::POST, Method::OPTIONS] {
        let response = fixture.request(method, "/-/plugins/example.pages/nested/report", None, &[]).await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let response =
        fixture.request(Method::POST, "/-/plugins/example.pages/nested/report", None, &[("Origin", "https://evil.example")]).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let response = fixture.request(Method::HEAD, "/-/plugins/example.pages/nested/report?ignored=private", None, &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(to_bytes(response.into_body(), 1024).await.unwrap().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "HEAD shares the declared read-only callback contract");
    assert_eq!(fixture.get("/admin/plugins", Some(&fixture.admin_cookie)).await.0, StatusCode::OK);

    let user = fixture
        .database
        .accounts()
        .create_user_with_initial_password(rbit_storage::NewUser {
            name: "temporary".parse().unwrap(),
            email: "temporary@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: false,
        })
        .await
        .unwrap();
    let cookie = format!("rbit_session={}", fixture.services.accounts.start_session(&user).await.unwrap().encode());
    let response = fixture.request(Method::GET, "/-/plugins/example.pages/nested/report", Some(&cookie), &[]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/settings/account");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "forced-password sessions cannot reach optional page callbacks");
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn runtime_route_registration_retains_capacity_for_full_and_htmx_unread_bodies() {
    let fixture = Fixture::new().await;
    for (index, partial) in [false, true].into_iter().enumerate() {
        let scope = scope(10 + u64::try_from(index).unwrap());
        let calls = Arc::default();
        let contexts = Arc::default();
        let registry = fixture.services.page_registry();
        let registration = registry
            .stage(
                "example.pages".parse().unwrap(),
                &scope,
                scope.generation(),
                vec![page("read", PageScope::Repository, PageAudience::RepositoryReader, "private output", &calls, &contexts)],
            )
            .unwrap();
        registration.publish().unwrap();
        let headers = if partial { vec![("HX-Request", "true"), ("Rbit-Partial", "/alice/rocket")] } else { Vec::new() };
        let mut responses = Vec::new();
        for _ in 0..64 {
            let response = fixture.request(Method::GET, "/alice/rocket/-/plugins/example.pages/read", None, &headers).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            if partial {
                assert_eq!(response.headers()["rbit-partial"], "true");
            }
            responses.push(response);
        }
        assert_eq!(scope.in_flight(), 64, "unpolled transformed bodies retain generation authority");
        let response = fixture.request(Method::GET, "/alice/rocket/-/plugins/example.pages/read", None, &headers).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(calls.load(Ordering::SeqCst), 64, "overload never invokes the renderer");
        drop(responses.pop());
        tokio::time::timeout(Duration::from_secs(1), async {
            while scope.in_flight() != 63 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let response = fixture.request(Method::GET, "/alice/rocket/-/plugins/example.pages/read", None, &headers).await;
        assert_eq!(response.status(), StatusCode::OK);
        responses.push(response);
        assert_eq!(scope.in_flight(), 64);
        scope.dispose(Duration::from_secs(1)).await.unwrap();
        assert_eq!(scope.in_flight(), 0, "cancellation drains without any peer polling");
        for response in responses {
            assert!(to_bytes(response.into_body(), 1024 * 1024).await.is_err());
        }
        assert_eq!(fixture.get("/alice/rocket/-/plugins/example.pages/read", None).await.0, StatusCode::NOT_FOUND);
    }
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn runtime_route_registration_preserves_nested_navigation_identity() {
    use rbit_plugin_api::{UiContent, UiContribution, UiContributionSet, UiIcon, UiLink, UiMessage, UiSlot, UiVisibility};
    let fixture = Fixture::new().await;
    let scope = scope(20);
    let owner = "example.pages".parse().unwrap();
    let calls = Arc::default();
    let contexts = Arc::default();
    let mut pages = Vec::new();
    let mut contributions = Vec::new();
    for (id, page_scope, audience, slot, visibility) in [
        ("global", PageScope::Global, PageAudience::Public, UiSlot::GlobalNav, UiVisibility::Public),
        ("repository", PageScope::Repository, PageAudience::RepositoryReader, UiSlot::RepositoryTab, UiVisibility::RepositoryReader),
    ] {
        let descriptor = PageDescriptor::new(id, page_scope, "nested/report", audience, Duration::from_millis(100)).unwrap();
        contributions.push(UiContribution {
            id: id.parse().unwrap(),
            slot,
            order: 0,
            visibility,
            content: UiContent::Link(UiLink {
                label: UiMessage { namespace: "example.pages".into(), key: "label".into(), fallback: "Runtime report".into() },
                target: descriptor.target(&owner),
                icon: UiIcon::Info,
            }),
        });
        pages.push(page("nested/report", page_scope, audience, "nested output", &calls, &contexts));
    }
    let pages = fixture.services.page_registry().stage(owner.clone(), &scope, scope.generation(), pages).unwrap();
    let ui = fixture.services.ui_registry().stage(owner, &scope, scope.generation(), UiContributionSet::new(contributions)).unwrap();
    pages.publish().unwrap();
    ui.publish().unwrap();
    for (path, marker) in [
        ("/-/plugins/example.pages/nested/report", "data-plugin-ui=\"example.pages:global\""),
        ("/alice/rocket/-/plugins/example.pages/nested/report", "data-repo-tab=\"example.pages:repository\""),
    ] {
        let (status, _, html) = fixture.get(path, None).await;
        assert_eq!(status, StatusCode::OK);
        let link = html.split(marker).nth(1).unwrap().split("</a>").next().unwrap();
        assert!(link.contains(&format!("href=\"{path}\"")), "{link}");
        assert!(link.contains("aria-current=\"page\""), "{link}");
    }
    let response = fixture
        .request(
            Method::GET,
            "/alice/rocket/-/plugins/example.pages/nested/report",
            None,
            &[("HX-Request", "true"), ("Rbit-Partial", "/alice/rocket")],
        )
        .await;
    assert_eq!(response.headers()["rbit-partial"], "true");
    let html = String::from_utf8(to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec()).unwrap();
    assert!(html.contains("nested output"));
    assert!(html.contains("hx-history=\"false\""));
    assert!(html.contains("data-repo-tab=\"example.pages:repository\""));
    fixture.services.shutdown().await;
}
