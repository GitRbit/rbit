//! Installed language management cannot be locked out by optional request hooks.

use super::*;
use rbit_plugin_api::Generation;
use rbit_plugin_runtime::PluginScope;
use rbit_services::request_hooks::{HookDecision, HookFuture, HookRegistration, Rejection, RequestHook, RequestMetadata};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

#[derive(Debug)]
struct Deny {
    timeout: bool,
    calls: Arc<AtomicUsize>,
}
impl RequestHook for Deny {
    fn call<'a>(&'a self, _: &'a RequestMetadata) -> HookFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.timeout {
                std::future::pending::<()>().await;
            }
            HookDecision::Reject(Rejection::Forbidden)
        })
    }
}

async fn install(fixture: &Fixture, proof: &str) -> axum::response::Response {
    let wire = pack_wire(proof, &language_pack("ar", "rtl", "استكشاف"));
    let request = Request::builder()
        .method(Method::POST)
        .uri(INSTALL)
        .header(header::HOST, "localhost:3000")
        .header(header::ORIGIN, "http://localhost:3000")
        .header(header::COOKIE, &fixture.admin_cookie)
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=rbitpack")
        .body(Body::from(wire))
        .unwrap();
    fixture.router.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn native_i18n_routes_management_recovers_under_deny_and_timeout_hooks_without_bypassing_security() {
    for timeout in [false, true] {
        let fixture = Fixture::new().await;
        fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
        let scope = PluginScope::new(Generation::INITIAL);
        let calls = Arc::new(AtomicUsize::new(0));
        let hook =
            HookRegistration::new("deny", 0, Duration::from_millis(10), Arc::new(Deny { timeout, calls: Arc::clone(&calls) })).unwrap();
        let registration =
            fixture.services.request_hooks().stage("example.recovery".parse().unwrap(), &scope, scope.generation(), vec![hook]).unwrap();
        registration.publish().unwrap();
        let denial = if timeout { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::FORBIDDEN };
        for path in [LANGUAGE, "/-/plugins/rbit.i18n-admin/packs/unknown", "/-/plugins/example.other/packs"] {
            assert_eq!(fixture.get(path).await.0, denial, "no owner-prefix or wildcard exemption for {path}");
        }
        let expected_calls = calls.load(Ordering::SeqCst);
        assert_eq!(expected_calls, 3);
        let (status, page, headers) = fixture.get(PACKS).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        let upload_proof = native_proof(&page, INSTALL);
        assert_eq!(fixture.get("/admin/plugins").await.0, StatusCode::OK);
        assert_eq!(
            fixture.request(Method::GET, PACKS, Some(&fixture.user_cookie), "http://localhost:3000", "").await.0,
            StatusCode::NOT_FOUND
        );
        let response = install(&fixture, "wrong").await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(fixture.services.language_packs(&fixture.admin).await.unwrap().len(), 0);
        let response = install(&fixture, &upload_proof).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(fixture.services.language_packs(&fixture.admin).await.unwrap().len(), 1);
        let (_, page, _) = fixture.get(PACKS).await;
        let remove_proof = native_proof(&page, REMOVE);
        for (cookie, origin, body, status) in [
            (&fixture.user_cookie, "http://localhost:3000", format!("_csrf={remove_proof}&id=rbit.language.ar"), StatusCode::NOT_FOUND),
            (&fixture.admin_cookie, "https://attacker.example", format!("_csrf={remove_proof}&id=rbit.language.ar"), StatusCode::FORBIDDEN),
            (&fixture.admin_cookie, "http://localhost:3000", "id=rbit.language.ar".to_owned(), StatusCode::FORBIDDEN),
            (&fixture.admin_cookie, "http://localhost:3000", "_csrf=wrong&id=rbit.language.ar".to_owned(), StatusCode::FORBIDDEN),
        ] {
            assert_eq!(fixture.request(Method::POST, REMOVE, Some(cookie), origin, &body).await.0, status);
        }
        assert_eq!(
            fixture.services.language_packs(&fixture.admin).await.unwrap().len(),
            1,
            "refused remove requests cannot change metadata"
        );
        assert_eq!(
            fixture
                .request(
                    Method::POST,
                    REMOVE,
                    Some(&fixture.admin_cookie),
                    "http://localhost:3000",
                    &format!("_csrf={remove_proof}&id=rbit.language.ar")
                )
                .await
                .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(fixture.services.language_packs(&fixture.admin).await.unwrap().len(), 0);
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls, "recovery requests still run host security, not optional hooks");
        scope.dispose(Duration::from_secs(1)).await.unwrap();
        fixture.services.shutdown().await;
    }
}
