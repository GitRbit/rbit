use super::super::super::{ApiMethod, auth::task_route, test_support::Fixture};
use super::super::tests::{fixture, seed_native, stalled};
use super::*;
use axum::{
    body::Bytes,
    http::{HeaderName, HeaderValue, Request, Response, header},
};
use rbit_core::{Timestamp, TokenScope, User};
use rbit_plugin_api::{ExpectedPolicyRevision, InstalledPackageRef, PackageReviewIntent, PackageReviewRequest, PolicyGrantRequests};
use rbit_services::PackagePolicyCommand;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::Poll,
    time::Duration,
};

fn path(key: &str) -> String {
    format!("/api/v1/admin/plugin-policy-renewals/{key}")
}
fn request(token: Option<&str>, key: &str, body: Body) -> Request<Body> {
    let mut builder = Request::builder().method("PUT").uri(path(key)).header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(body).unwrap()
}
async fn send(fixture: &Fixture, token: &str, key: &str, input: &PackagePolicyRenewalRequest) -> (Response<Body>, Value) {
    fixture.send("PUT", &path(key), Some(token), Some(serde_json::to_value(input).unwrap())).await
}

// This asserts an already-sealed fixture, not an actual capacity-triggering write.
// Storage tests cover genuine exhaustion; transport tests prove renewal/recovery.
async fn seal_current(fixture: &Fixture, admin: &User, package: &InstalledPackageRef, key: &str) -> PackagePolicyRenewalRequest {
    use sqlx::{ConnectOptions as _, Connection as _};
    let gate = fixture.services.package_policy.gate(admin).await.unwrap();
    assert_eq!(gate.state, rbit_services::PackagePolicyGateState::Open);
    let fingerprint = gate.fingerprint.unwrap();
    let rejected = PackageReviewRequest::new(
        package.clone(),
        ExpectedPolicyRevision::Absent {},
        fingerprint.clone(),
        PackageReviewIntent::NativeFullServiceUserTrust {},
        PolicyGrantRequests::new(Vec::new()).unwrap(),
    )
    .unwrap();
    let canonical = PackagePolicyCommand::Review(rejected).canonical_request().unwrap();
    let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(fixture.services.context().config.storage.sqlite_path())
        .foreign_keys(true)
        .connect()
        .await
        .unwrap();
    let updated=sqlx::query("UPDATE plugin_policy_gate SET state=2,seal_reason=0,seal_actor=?,seal_operation=?,seal_request=?,sealed_at=? WHERE id=1 AND state=1")
        .bind(admin.id().0).bind(key).bind(canonical).bind(Timestamp::now().0).execute(&mut connection).await.unwrap();
    assert_eq!(updated.rows_affected(), 1);
    connection.close().await.unwrap();
    PackagePolicyRenewalRequest::new(fingerprint, key).unwrap()
}

#[tokio::test]
async fn authority_and_task_exclusion_precede_renewal_body_polling() {
    let (fixture, admin, token) = fixture().await;
    let ordinary = fixture.user("renewal-reader").await;
    let nonadmin = fixture.token(&ordinary, TokenScope::Admin).await;
    let read = fixture.token(&admin, TokenScope::Read).await;
    let write = fixture.token(&admin, TokenScope::Write).await;
    for (token, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(nonadmin.as_str()), StatusCode::FORBIDDEN),
        (Some(read.as_str()), StatusCode::FORBIDDEN),
        (Some(write.as_str()), StatusCode::FORBIDDEN),
    ] {
        let polls = Arc::new(AtomicUsize::new(0));
        let response =
            tokio::time::timeout(Duration::from_secs(5), fixture.oneshot(request(token, "unpolled", stalled(Arc::clone(&polls)))))
                .await
                .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
    }
    let cookie = fixture.services.accounts.start_session(&admin).await.unwrap();
    let polls = Arc::new(AtomicUsize::new(0));
    let mut browser = request(None, "cookie", stalled(Arc::clone(&polls)));
    browser.headers_mut().insert(header::COOKIE, HeaderValue::from_str(&format!("rbit_session={}", cookie.encode())).unwrap());
    assert_eq!(fixture.oneshot(browser).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let database = &fixture.services.context().database;
    assert_eq!(database.admin().set_admin(ordinary.id(), true).await.unwrap(), rbit_storage::sqlite::admin::Moderation::Applied);
    assert_eq!(database.admin().set_admin(admin.id(), false).await.unwrap(), rbit_storage::sqlite::admin::Moderation::Applied);
    let polls = Arc::new(AtomicUsize::new(0));
    assert_eq!(fixture.oneshot(request(Some(&token), "demoted", stalled(Arc::clone(&polls)))).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert!(task_route(ApiMethod::Put, OPERATIONS).is_none());
    assert!(task_route(ApiMethod::Get, OPERATIONS).is_none());
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn renewal_headers_reject_before_body_and_disabled_api_stays_disabled() {
    let (fixture, _, token) = fixture().await;
    for (name, value, expected) in [
        ("content-type", "application/octet-stream", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("content-encoding", "gzip", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("content-length", "1025", StatusCode::PAYLOAD_TOO_LARGE),
        ("content-length", "invalid", StatusCode::BAD_REQUEST),
    ] {
        let polls = Arc::new(AtomicUsize::new(0));
        let mut request = request(Some(&token), "bad-headers", stalled(Arc::clone(&polls)));
        request.headers_mut().insert(HeaderName::from_static(name), HeaderValue::from_static(value));
        let response = tokio::time::timeout(Duration::from_secs(5), fixture.oneshot(request)).await.unwrap();
        assert_eq!(response.status(), expected);
        assert!(response.headers()[header::LOCATION].to_str().unwrap().ends_with("/bad-headers"));
        assert_eq!(polls.load(Ordering::SeqCst), 0);
    }
    let input =
        PackagePolicyRenewalRequest::new(rbit_plugin_api::HostPolicyFingerprint::new("b".repeat(64).parse().unwrap()), "observed-seal")
            .unwrap();
    let mut exact = serde_json::to_vec(&input).unwrap();
    exact.resize(MAX_POLICY_RENEWAL_REQUEST_BYTES, b' ');
    let mut unknown = serde_json::to_value(&input).unwrap();
    unknown["force"] = json!(true);
    for (body, expected) in [
        (exact, StatusCode::CONFLICT), // Valid exact-cap input reaches uninitialized-gate CAS.
        (serde_json::to_vec(&unknown).unwrap(), StatusCode::UNPROCESSABLE_ENTITY),
        (vec![b'x'; MAX_POLICY_RENEWAL_REQUEST_BYTES + 1], StatusCode::PAYLOAD_TOO_LARGE),
        (b"{".to_vec(), StatusCode::BAD_REQUEST),
    ] {
        let response = fixture.oneshot(request(Some(&token), "strict-input", Body::from(body))).await;
        assert_eq!(response.status(), expected);
    }
    fixture.services.shutdown().await;
    let disabled = Fixture::new(|config| {
        config.api.enabled = false;
        config.actions.enabled = false;
        config.i18n.enabled = false;
        config.updates.check_for_updates = false;
    })
    .await;
    let polls = Arc::new(AtomicUsize::new(0));
    assert_eq!(disabled.oneshot(request(None, "disabled", stalled(Arc::clone(&polls)))).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    disabled.services.shutdown().await;
}

#[tokio::test]
async fn complete_json_quota_is_one_kib_with_strict_data_and_cooperative_deadline() {
    use std::future::Future as _;
    let input =
        PackagePolicyRenewalRequest::new(rbit_plugin_api::HostPolicyFingerprint::new("b".repeat(64).parse().unwrap()), "observed-seal")
            .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let mut bytes = serde_json::to_vec(&input).unwrap();
    bytes.resize(MAX_POLICY_RENEWAL_REQUEST_BYTES, b' ');
    let parsed: PackagePolicyRenewalRequest =
        read_json_limited(Body::from(bytes.clone()), &headers, Instant::now() + COMMAND_TIMEOUT, MAX_POLICY_RENEWAL_REQUEST_BYTES)
            .await
            .unwrap();
    assert_eq!(parsed, input);
    let chunks = vec![Ok::<_, std::io::Error>(Bytes::from(bytes)), Ok(Bytes::from_static(b" "))];
    assert_eq!(
        read_json_limited::<PackagePolicyRenewalRequest>(
            Body::from_stream(futures_util::stream::iter(chunks)),
            &headers,
            Instant::now() + COMMAND_TIMEOUT,
            MAX_POLICY_RENEWAL_REQUEST_BYTES
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut unknown = serde_json::to_value(&input).unwrap();
    unknown["enable"] = json!(true);
    for (body, expected) in [
        (serde_json::to_vec(&unknown).unwrap(), StatusCode::UNPROCESSABLE_ENTITY),
        (b"{".to_vec(), StatusCode::BAD_REQUEST),
        (vec![b'x'; 1025], StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        assert_eq!(
            read_json_limited::<PackagePolicyRenewalRequest>(
                Body::from(body),
                &headers,
                Instant::now() + COMMAND_TIMEOUT,
                MAX_POLICY_RENEWAL_REQUEST_BYTES
            )
            .await
            .unwrap_err()
            .status,
            expected
        );
    }
    let polls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&polls);
    let body = Body::from_stream(futures_util::stream::poll_fn(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(Some(Ok::<_, std::io::Error>(Bytes::new())))
    }));
    let mut read = Box::pin(read_json_limited::<PackagePolicyRenewalRequest>(
        body,
        &headers,
        Instant::now() + Duration::from_millis(20),
        MAX_POLICY_RENEWAL_REQUEST_BYTES,
    ));
    std::future::poll_fn(|cx| {
        assert!(read.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(tokio::time::timeout(Duration::from_secs(1), read).await.unwrap().unwrap_err().status, StatusCode::REQUEST_TIMEOUT);
}

#[tokio::test]
async fn real_routes_renew_replay_refuse_stale_and_preserve_retention_full() {
    let (fixture, admin, token) = fixture().await;
    let package = seed_native(&fixture, &admin).await;
    fixture.services.package_policy.prepare(&admin).await.unwrap();
    let first = seal_current(&fixture, &admin, &package, "http-seal-one").await;
    let mut stale = first.clone();
    "different-seal".clone_into(&mut stale.expected_seal_operation);
    assert_eq!(send(&fixture, &token, "stale-seal", &stale).await.0.status(), StatusCode::CONFLICT);
    let (response, applied) = send(&fixture, &token, "renew-one", &first).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::LOCATION].to_str().unwrap().ends_with("/renew-one"));
    assert_eq!(applied["outcome"], "applied");
    assert_eq!(applied["execution"], "unavailable");
    assert_eq!(applied["receipt"]["source_host_policy"], first.expected_host_policy.as_str());
    assert_ne!(applied["receipt"]["source_epoch"], applied["receipt"]["destination_epoch"]);
    assert_ne!(applied["receipt"]["source_host_policy"], applied["receipt"]["destination_host_policy"]);
    assert_eq!(applied["gate"]["epoch"], applied["receipt"]["destination_epoch"]);
    assert_eq!(applied["gate"]["host_policy"], applied["receipt"]["destination_host_policy"]);
    assert_eq!(applied["receipt"]["actor_id"], admin.id().0.to_string());
    assert!(applied["receipt"].get("request").is_none());
    assert!(!applied["receipt"]["historical"].as_bool().unwrap());
    let (_, receipt) = fixture.send("GET", &path("renew-one"), Some(&token), None).await;
    assert_eq!(receipt["outcome"], "historical_receipt");
    assert_eq!(receipt["receipt"]["historical"], true);
    assert!(receipt.get("gate").is_none());
    assert!(fixture.services.package_policy.inspect_package(&admin, &package).await.unwrap().policy.review.is_none());
    assert_eq!(send(&fixture, &token, "stale-fingerprint", &first).await.0.status(), StatusCode::CONFLICT);
    let second = seal_current(&fixture, &admin, &package, "http-seal-two").await;
    assert_eq!(send(&fixture, &token, "renew-two", &second).await.0.status(), StatusCode::OK);
    let (response, historical) = send(&fixture, &token, "renew-one", &first).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(historical["outcome"], "historical_receipt");
    assert_eq!(historical["receipt"]["destination_epoch"], applied["receipt"]["destination_epoch"]);
    assert_ne!(historical["gate"]["epoch"], historical["receipt"]["destination_epoch"]);
    let third = seal_current(&fixture, &admin, &package, "http-seal-three").await;
    let (response, full) = send(&fixture, &token, "renew-three", &third).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(full["outcome"], "retention_full");
    assert!(full["receipt"].is_null());
    assert_eq!(full["gate"]["state"], "sealed");
    assert_eq!(full["gate"]["host_policy"], third.expected_host_policy.as_str());
    assert_eq!(fixture.send("GET", &path("renew-three"), Some(&token), None).await.0.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        fixture.send("GET", "/api/v1/admin/plugin-policy-operations/renew-one", Some(&token), None).await.0.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(fixture.services.context().database.plugin_settings().desired_enabled(package.plugin_id().as_str()).await.unwrap(), None);
    fixture.services.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn dropped_admitted_renewal_waiter_has_queryable_receipt() {
    use futures_util::StreamExt as _;
    use sqlx::{ConnectOptions as _, Connection as _};
    use std::future::Future as _;
    let (fixture, admin, token) = fixture().await;
    let package = seed_native(&fixture, &admin).await;
    fixture.services.package_policy.prepare(&admin).await.unwrap();
    let input = seal_current(&fixture, &admin, &package, "lost-http-seal").await;
    let bytes = serde_json::to_vec(&input).unwrap();
    let mut blocker = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(fixture.services.context().config.storage.sqlite_path())
        .connect()
        .await
        .unwrap();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let data = futures_util::stream::once(async move {
        // PAT authentication touches storage. Lock only at first body poll,
        // after that write and live-authority preflight, before renewal admission.
        sqlx::query("BEGIN IMMEDIATE").execute(&mut blocker).await.unwrap();
        sender.send(blocker).unwrap();
        Ok::<_, std::io::Error>(Bytes::from(bytes))
    });
    let eof = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&eof);
    let end = futures_util::stream::poll_fn(move |_| {
        observed.store(true, Ordering::SeqCst);
        Poll::Ready(None)
    });
    let mut response = Box::pin(fixture.oneshot(request(Some(&token), "lost-renewal", Body::from_stream(data.chain(end)))));
    tokio::time::timeout(
        Duration::from_secs(3),
        std::future::poll_fn(|cx| {
            assert!(response.as_mut().poll(cx).is_pending());
            if eof.load(Ordering::SeqCst) { Poll::Ready(()) } else { Poll::Pending }
        }),
    )
    .await
    .unwrap();
    // EOF-to-owned admission has no intervening Pending; this drops an admitted
    // HTTP waiter on one thread, not a body reader or the owned transaction.
    drop(response);
    let mut blocker = receiver.await.unwrap();
    sqlx::query("COMMIT").execute(&mut blocker).await.unwrap();
    blocker.close().await.unwrap();
    let receipt = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (response, value) = fixture.send("GET", &path("lost-renewal"), Some(&token), None).await;
            if response.status() == StatusCode::OK {
                break value;
            }
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(receipt["outcome"], "historical_receipt");
    assert_eq!(receipt["receipt"]["source_host_policy"], input.expected_host_policy.as_str());
    assert_eq!(receipt["receipt"]["historical"], true);
    let gate = fixture.services.package_policy.gate(&admin).await.unwrap();
    assert_eq!(receipt["receipt"]["destination_epoch"], gate.epoch.unwrap().as_str());
    assert_eq!(receipt["receipt"]["destination_host_policy"], gate.fingerprint.unwrap().as_str());
    // Ordinary commit recovery; no uncertain COMMIT fault was injected.
    fixture.services.shutdown().await;
}

#[test]
fn renewal_openapi_and_receipt_projection_expose_only_passive_bounded_contracts() {
    let document: Value = serde_json::from_str(include_str!("../../openapi/package_policy.json")).unwrap();
    let endpoint = &document["paths"]["/admin/plugin-policy-renewals/{operation_id}"];
    assert_eq!(endpoint["put"]["x-max-request-body-bytes"], 1024);
    assert_eq!(document["components"]["schemas"]["PolicyRenewalRequest"]["additionalProperties"], false);
    assert_eq!(document["components"]["schemas"]["PolicyRenewalRequest"]["required"].as_array().unwrap().len(), 3);
    assert!(document["components"]["schemas"]["PolicyRenewalReceiptResult"]["properties"].get("gate").is_none());
    assert_eq!(routes().len(), 2);
}
