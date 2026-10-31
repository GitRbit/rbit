//! Generation-scoped metadata hooks cannot inspect or replace host transport guards.

use axum::{
    extract::{MatchedPath, Request, State},
    http::{HeaderName, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rbit_services::request_hooks::{HookDecision, Rejection, RequestHookRegistry, RequestMetadata, RequestMethod};

use crate::AppState;

/// Hooks execute inside preload and Actions admission guards, before route-owned
/// authentication and CSRF. Their only control flow is continue or host-owned denial.
pub async fn request_hooks(State(state): State<AppState>, request: Request, next: Next) -> Response {
    apply(&state.services.request_hooks(), request, next).await
}

async fn apply(registry: &RequestHookRegistry, request: Request, next: Next) -> Response {
    // Raw paths may contain reset/unsubscribe credentials. Only the router's
    // declared pattern is exposed, with a fixed value for unmatched routes.
    let route = request.extensions().get::<MatchedPath>().map_or("<unmatched>", MatchedPath::as_str);
    // Optional hooks cannot trap administrators or prevent the host from serving
    // its own authentication, recovery, diagnostics and embedded assets.
    if reserved_host_route(route) {
        return next.run(request).await;
    }
    let snapshot = registry.snapshot();
    if snapshot.is_empty() {
        return next.run(request).await;
    }
    let Ok(metadata) = RequestMetadata::new(RequestMethod::from_http(request.method().as_str()), route) else {
        return refused(Rejection::Unavailable);
    };
    let annotations = match snapshot.evaluate(&metadata).await {
        HookDecision::Continue(annotations) => annotations,
        HookDecision::Reject(reason) => return refused(reason),
    };
    let mut response = next.run(request).await;
    for annotation in annotations {
        let (Ok(name), Ok(value)) = (HeaderName::from_bytes(annotation.name().as_bytes()), HeaderValue::from_str(annotation.value()))
        else {
            return refused(Rejection::Unavailable);
        };
        // Host-owned headers, including any existing annotation, always win.
        response.headers_mut().entry(name).or_insert(value);
    }
    response
}

fn reserved_host_route(route: &str) -> bool {
    matches!(
        route,
        "/admin/plugins"
            | "/admin/plugins/actions"
            | "/admin/plugins/i18n"
            | "/admin/plugins/i18n/packs"
            | "/admin/plugins/i18n/packs/{id}/remove"
            | "/login"
            | "/login/two-factor"
            | "/login/{provider}"
            | "/login/{provider}/callback"
            | "/login/{provider}/register"
            | "/logout"
            | "/settings/account"
            | "/settings/account/password"
            | "/password/forgot"
            | "/password/reset/complete"
            | "/password/reset/{token}"
            | "/email/verify/{token}"
            | "/assets/{*path}"
            | "/assets/branding.css"
            | "/healthz"
            | "/readyz"
            | "/metrics"
    )
}

fn refused(reason: Rejection) -> Response {
    let (status, text) = match reason {
        Rejection::Forbidden => (StatusCode::FORBIDDEN, "Request denied by an active plugin."),
        Rejection::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "Too many requests."),
        Rejection::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "Service temporarily unavailable."),
    };
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; frame-ancestors 'none'"),
        ],
        text,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        middleware::{from_fn, from_fn_with_state},
        routing::{get, post},
    };
    use rbit_plugin_api::Generation;
    use rbit_plugin_runtime::PluginScope;
    use rbit_services::request_hooks::{HookFuture, HookRegistration, RequestHook, RequestHookRegistration, ResponseAnnotation};
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tower::ServiceExt as _;

    #[derive(Debug)]
    struct Probe {
        calls: Arc<AtomicUsize>,
        metadata: Arc<Mutex<Option<RequestMetadata>>>,
        decision: HookDecision,
        delay: Duration,
    }
    impl RequestHook for Probe {
        fn call<'a>(&'a self, metadata: &'a RequestMetadata) -> HookFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                *self.metadata.lock().unwrap() = Some(metadata.clone());
                if !self.delay.is_zero() {
                    tokio::time::sleep(self.delay).await;
                }
                self.decision.clone()
            })
        }
    }
    fn register(registry: &RequestHookRegistry, probe: Probe) -> (PluginScope, RequestHookRegistration) {
        register_generation(registry, probe, Generation::INITIAL)
    }
    fn register_generation(registry: &RequestHookRegistry, probe: Probe, generation: Generation) -> (PluginScope, RequestHookRegistration) {
        let scope = PluginScope::new(generation);
        let hook = HookRegistration::new("test", 0, Duration::from_millis(20), Arc::new(probe)).unwrap();
        let registration = registry.stage("rbit.test".parse().unwrap(), &scope, scope.generation(), vec![hook]).unwrap();
        registration.publish().unwrap();
        (scope, registration)
    }
    async fn layer(State(registry): State<RequestHookRegistry>, request: Request, next: Next) -> Response {
        apply(&registry, request, next).await
    }
    fn probe(decision: HookDecision) -> Probe {
        Probe { calls: Arc::default(), metadata: Arc::default(), decision, delay: Duration::ZERO }
    }

    #[tokio::test]
    async fn one_router_observes_publication_withdrawal_and_scope_cleanup() {
        let registry = RequestHookRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let handler_calls = Arc::clone(&calls);
        let router = Router::new()
            .route(
                "/",
                get(move || {
                    let calls = Arc::clone(&handler_calls);
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        "host"
                    }
                }),
            )
            .layer(from_fn_with_state(registry.clone(), layer));
        let request = || Request::builder().uri("/").body(Body::empty()).unwrap();
        assert_eq!(router.clone().oneshot(request()).await.unwrap().status(), StatusCode::OK);
        let (scope, registration) = register(&registry, probe(HookDecision::Reject(Rejection::Forbidden)));
        let response = router.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "denial cannot enter the host handler");
        registration.unpublish();
        assert_eq!(router.clone().oneshot(request()).await.unwrap().status(), StatusCode::OK);
        let (next_scope, _) =
            register_generation(&registry, probe(HookDecision::Reject(Rejection::TooManyRequests)), Generation::new(2).unwrap());
        scope.quiesce();
        assert_eq!(
            router.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS,
            "old scope cleanup cannot revoke a replacement registration"
        );
        next_scope.quiesce();
        assert_eq!(router.oneshot(request()).await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn callbacks_never_receive_raw_tokens_queries_credentials_or_request_bodies() {
        let registry = RequestHookRegistry::new();
        let hook = probe(HookDecision::Continue(Vec::new()));
        let observed = Arc::clone(&hook.metadata);
        let (_scope, _registration) = register(&registry, hook);
        let router = Router::new()
            .route("/email/unsubscribe/{token}", post(|| async { StatusCode::NO_CONTENT }))
            .fallback(|| async { StatusCode::NOT_FOUND })
            .layer(from_fn_with_state(registry, layer));
        let request = Request::builder()
            .method("POST")
            .uri("/email/unsubscribe/private-path-token?query=private-query")
            .header(header::AUTHORIZATION, "Bearer private-header-token")
            .header(header::COOKIE, "private-cookie=secret")
            .body(Body::from("private-request-body"))
            .unwrap();
        assert_eq!(router.clone().oneshot(request).await.unwrap().status(), StatusCode::NO_CONTENT);
        let metadata = observed.lock().unwrap().clone().unwrap();
        assert_eq!(metadata.route(), "/email/unsubscribe/{token}");
        assert_eq!(metadata.method(), RequestMethod::Post);
        assert!(!format!("{metadata:?}").contains("private"));
        let request = Request::builder().uri("/unknown/private-path-token").body(Body::empty()).unwrap();
        assert_eq!(router.oneshot(request).await.unwrap().status(), StatusCode::NOT_FOUND);
        assert_eq!(observed.lock().unwrap().as_ref().unwrap().route(), "<unmatched>");
    }

    #[tokio::test]
    async fn plugin_annotations_preserve_host_authentication_and_security_headers() {
        let registry = RequestHookRegistry::new();
        let (_scope, _registration) =
            register(&registry, probe(HookDecision::Continue(vec![ResponseAnnotation::new("x-rbit-plugin-test", "plugin").unwrap()])));
        let router = Router::new()
            .route(
                "/protected",
                get(|| async {
                    (
                        StatusCode::UNAUTHORIZED,
                        [
                            (header::WWW_AUTHENTICATE.as_str(), "Basic realm=host"),
                            (header::CACHE_CONTROL.as_str(), "no-store"),
                            (header::CONTENT_SECURITY_POLICY.as_str(), "default-src 'none'"),
                            ("x-rbit-plugin-test", "host"),
                        ],
                    )
                }),
            )
            .layer(from_fn_with_state(registry, layer));
        let response = router.oneshot(Request::builder().uri("/protected").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Basic realm=host");
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], "default-src 'none'");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()["x-rbit-plugin-test"], "host", "host annotation wins");
    }

    #[tokio::test]
    async fn mandatory_preload_guard_runs_before_extension_callbacks() {
        let registry = RequestHookRegistry::new();
        let hook = probe(HookDecision::Reject(Rejection::Forbidden));
        let calls = Arc::clone(&hook.calls);
        let (_scope, _registration) = register(&registry, hook);
        let router = Router::new()
            .route("/logout", get(|| async { "must not run" }))
            .layer(from_fn_with_state(registry, layer))
            .layer(from_fn(crate::middleware::preload_guard));
        let response = router
            .oneshot(Request::builder().uri("/logout").header(crate::middleware::PRELOAD_HEADER, "true").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn timed_out_callbacks_return_fixed_unavailable_and_release_their_lease() {
        let registry = RequestHookRegistry::new();
        let mut hook = probe(HookDecision::Continue(Vec::new()));
        hook.delay = Duration::from_secs(10);
        let (scope, _registration) = register(&registry, hook);
        let router = Router::new().route("/", get(|| async { "must not run" })).layer(from_fn_with_state(registry, layer));
        let response = router.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(scope.in_flight(), 0);
        assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap().as_ref(), b"Service temporarily unavailable.");
    }
    #[tokio::test]
    async fn reserved_recovery_routes_keep_host_guards_and_other_admin_routes_remain_hooked() {
        let registry = RequestHookRegistry::new();
        let hook = probe(HookDecision::Reject(Rejection::Forbidden));
        let calls = Arc::clone(&hook.calls);
        let (_scope, _registration) = register(&registry, hook);
        let router = Router::new()
            .route("/admin/plugins", get(|| async { StatusCode::UNAUTHORIZED }))
            .route("/login", get(|| async { StatusCode::OK }).post(|| async { StatusCode::UNAUTHORIZED }))
            .route("/assets/{*path}", get(|| async { "trusted asset" }))
            .route("/admin", get(|| async { "must not run" }))
            .layer(from_fn_with_state(registry, layer));
        for (method, path, status) in [
            ("GET", "/admin/plugins", StatusCode::UNAUTHORIZED),
            ("GET", "/login", StatusCode::OK),
            ("POST", "/login", StatusCode::UNAUTHORIZED),
            ("GET", "/assets/app.bundle.js", StatusCode::OK),
        ] {
            let response = router.clone().oneshot(Request::builder().method(method).uri(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), status, "{method} {path}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let response = router.oneshot(Request::builder().uri("/admin").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no blanket admin exemption");
    }
}
