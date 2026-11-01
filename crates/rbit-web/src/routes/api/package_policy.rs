//! Bounded administrator policy metadata. Approval/selection never activates code.
use super::{
    ApiMethod::{Get, Post, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    route,
};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{TokenScope, User};
use rbit_plugin_api::{
    InstalledPackageRef, MAX_POLICY_REQUEST_BYTES, PackageReviewIntent, PackageReviewRequest, PackageSelectionClearRequest,
    PackageSelectionRequest, PluginId,
};
use rbit_services::{
    PackageOperationId, PackagePolicyCommand, PackagePolicyGate, PackagePolicyGateState, PackagePolicyOutcome, PackagePolicyReceipt,
    PackagePolicyReview, PackagePolicySealReason, PackagePolicySelection, ServiceError, webhooks::payloads,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{pin::Pin, time::Duration};
use tokio::time::Instant;

mod renewal;

const ROOT: &str = "/admin/plugin-policy";
const OPERATIONS: &str = "/admin/plugin-policy-operations/{operation_id}";
const RESPONSE_LIMIT: usize = 64 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(25);
const BODY_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn routes() -> Vec<ApiRoute> {
    let mut routes = vec![
        route(Get, ROOT, gate),
        route(Post, "/admin/plugin-policy/prepare", prepare),
        route(Get, "/admin/plugin-policy/candidates/{plugin_id}", inspect),
        route(Get, "/admin/plugin-policy/packages/{plugin_id}/versions/{full_version}/archives/{digest}", proposal),
        route(Put, OPERATIONS, apply),
        route(Get, OPERATIONS, operation),
    ];
    routes.extend(renewal::routes());
    routes
}
fn administrator(caller: &Caller) -> ApiResult<&User> {
    let actor = caller.require(TokenScope::Admin)?;
    if !actor.is_admin {
        return Err(ApiError::forbidden("Active site administrator required"));
    }
    Ok(actor)
}
fn fault(error: &ServiceError) -> ApiError {
    match error {
        ServiceError::Forbidden => ApiError::forbidden("Active site administrator required"),
        ServiceError::Conflict(
            "server shutdown"
            | "package policy operation admission"
            | "package policy coordinator unavailable"
            | "package policy maintenance admission",
        ) => ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Package policy administration unavailable"),
        ServiceError::Conflict("empty host capability catalog" | "package review declaration mismatch") => {
            ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported passive package policy declaration")
        }
        ServiceError::Conflict(_) => {
            ApiError::new(StatusCode::CONFLICT, "Package policy identity, revision or state conflicts; inspect current policy")
        }
        ServiceError::Invalid(_) => ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid passive package policy request"),
        _ => {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Package policy result uncertain; inspect operation and gate before retrying")
        }
    }
}
fn url(state: &AppState, path: &str) -> String {
    format!("{}/api/v1{path}", state.config.server.base_url().trim_end_matches('/'))
}
fn located(mut error: ApiError, location: &str) -> ApiError {
    error.location = Some(location.to_owned());
    error
}
fn expired() -> ApiError {
    ApiError::new(StatusCode::REQUEST_TIMEOUT, "Package policy deadline expired; inspect operation and gate")
}
fn operation_id(raw: &str) -> ApiResult<PackageOperationId> {
    PackageOperationId::new(raw).map_err(|_| ApiError::invalid("operation_id", "Expected 1-64 lowercase ASCII letters, digits or hyphens"))
}
fn plugin_id(raw: &str) -> ApiResult<PluginId> {
    raw.parse().map_err(|_| ApiError::invalid("plugin_id", "Invalid bounded plugin identity"))
}
fn answer(status: StatusCode, value: &Value, location: Option<&str>) -> ApiResult<Response> {
    let bytes = serde_json::to_vec(value).map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Policy response unavailable"))?;
    if bytes.len() > RESPONSE_LIMIT {
        return Err(ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Policy response exceeds its bounded contract"));
    }
    let mut response = (status, [(header::CONTENT_TYPE, "application/json")], bytes).into_response();
    if let Some(location) = location {
        response.headers_mut().insert(
            header::LOCATION,
            HeaderValue::from_str(location)
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Invalid configured public URL"))?,
        );
    }
    Ok(response)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepare {
    schema_version: u32,
}
#[derive(Deserialize)]
#[serde(tag = "command", content = "request", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Review(PackageReviewRequest),
    Select(PackageSelectionRequest),
    Clear(PackageSelectionClearRequest),
}

async fn gate(State(state): State<AppState>, caller: Caller) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let value = state.services.package_policy.gate(actor).await.map_err(|e| fault(&e))?;
    answer(StatusCode::OK, &json!({"schema_version":1,"execution":"unavailable","gate":gate_value(&value)}), None)
}
async fn prepare(State(state): State<AppState>, caller: Caller, headers: HeaderMap, body: Body) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let location = url(&state, ROOT);
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    // A raw Body extractor does not poll. Reload live authority before any frame.
    tokio::time::timeout_at(deadline, state.services.package_policy.gate(actor))
        .await
        .map_err(|_| located(expired(), &location))?
        .map_err(|e| located(fault(&e), &location))?;
    let input: Prepare = read_json(body, &headers, deadline).await.map_err(|e| located(e, &location))?;
    if input.schema_version != 1 {
        return Err(located(ApiError::invalid("schema_version", "Unsupported policy schema"), &location));
    }
    let value = tokio::time::timeout_at(deadline, state.services.package_policy.prepare(actor))
        .await
        .map_err(|_| located(expired(), &location))?
        .map_err(|e| located(fault(&e), &location))?;
    answer(
        StatusCode::OK,
        &json!({"schema_version":1,"outcome":"prepared","execution":"unavailable","gate":gate_value(&value),"url":location}),
        Some(&location),
    )
}
async fn inspect(State(state): State<AppState>, caller: Caller, Path(raw): Path<String>) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let id = plugin_id(&raw)?;
    let policy = state.services.package_policy.inspect(actor, &id).await.map_err(|e| fault(&e))?.policy;
    answer(
        StatusCode::OK,
        &json!({"schema_version":1,"execution":"unavailable","plugin_id":id.to_string(),"gate":gate_value(&policy.gate),"selection":policy.selection.as_ref().map(selection_value),"current_review":policy.review.as_ref().map(review_value)}),
        None,
    )
}
async fn proposal(
    State(state): State<AppState>,
    caller: Caller,
    Path((id, version, digest)): Path<(String, String, String)>,
) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    // SDK deserialization enforces full canonical SemVer, exact hash and bounds.
    let reference: InstalledPackageRef = serde_json::from_value(json!({"plugin_id":id,"version":version,"archive_digest":digest}))
        .map_err(|_| ApiError::invalid("package", "Invalid exact installed package identity"))?;
    let policy = state.services.package_policy.inspect_package(actor, &reference).await.map_err(|e| fault(&e))?.policy;
    let declared = &policy.declarations;
    answer(
        StatusCode::OK,
        &json!({"schema_version":1,"execution":"unavailable","package":policy.package,"gate":gate_value(&policy.gate),"declarations":{"kind":declared.kind,"host_api":declared.host_api.to_string(),"capabilities":declared.capabilities,"budgets":declared.budgets},"recognized_capabilities":[],"current_review":policy.review.as_ref().map(review_value)}),
        None,
    )
}
async fn apply(
    State(state): State<AppState>,
    caller: Caller,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let id = operation_id(&raw)?;
    let location = url(&state, &format!("/admin/plugin-policy-operations/{}", id.as_str()));
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    tokio::time::timeout_at(deadline, state.services.package_policy.gate(actor))
        .await
        .map_err(|_| located(expired(), &location))?
        .map_err(|e| located(fault(&e), &location))?;
    let input: Command = read_json(body, &headers, deadline).await.map_err(|e| located(e, &location))?;
    let operation = async {
        match input {
            Command::Review(request) => state.services.package_policy.review(actor, id, request).await,
            Command::Select(request) => state.services.package_policy.select(actor, id, request).await,
            Command::Clear(request) => state.services.package_policy.clear(actor, id, request).await,
        }
    };
    let result = tokio::time::timeout_at(deadline, operation)
        .await
        .map_err(|_| located(expired(), &location))?
        .map_err(|e| located(fault(&e), &location))?;
    let (status, outcome, receipt, gate) = match result {
        PackagePolicyOutcome::Applied { receipt, gate } => (StatusCode::OK, "applied", Some(receipt), gate),
        PackagePolicyOutcome::HistoricalReceipt { receipt, gate } => (StatusCode::OK, "historical_receipt", Some(receipt), gate),
        PackagePolicyOutcome::GateSealed { gate } => (StatusCode::CONFLICT, "gate_sealed", None, gate),
        PackagePolicyOutcome::CapacitySealed { gate } => (StatusCode::CONFLICT, "capacity_sealed", None, gate),
    };
    answer(
        status,
        &json!({"schema_version":1,"operation_id":raw,"url":location,"outcome":outcome,"execution":"unavailable","receipt":receipt.as_ref().map(|r| receipt_value(r,outcome=="historical_receipt")),"gate":gate_value(&gate),"gate_url":url(&state,ROOT)}),
        Some(&location),
    )
}
async fn operation(State(state): State<AppState>, caller: Caller, Path(raw): Path<String>) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let id = operation_id(&raw)?;
    let location = url(&state, &format!("/admin/plugin-policy-operations/{}", id.as_str()));
    let receipt =
        state.services.package_policy.operation(actor, &id).await.map_err(|e| located(fault(&e), &location))?.ok_or_else(|| {
            located(ApiError::new(StatusCode::NOT_FOUND, "No policy receipt; inspect the gate for a committed seal"), &location)
        })?;
    // Receipt-only inspection: do not claim a separate gate read is one snapshot.
    answer(
        StatusCode::OK,
        &json!({"schema_version":1,"operation_id":raw,"execution":"unavailable","receipt":receipt_value(&receipt,true),"gate_url":url(&state,ROOT)}),
        None,
    )
}

fn check_headers(headers: &HeaderMap, limit: usize) -> ApiResult<()> {
    if headers.get_all(header::CONTENT_TYPE).iter().count() != 1 || headers[header::CONTENT_TYPE].as_bytes() != b"application/json" {
        return Err(ApiError::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Expected application/json"));
    }
    if headers.get_all(header::CONTENT_ENCODING).iter().count() > 1
        || headers.get(header::CONTENT_ENCODING).is_some_and(|v| v.as_bytes() != b"identity")
    {
        return Err(ApiError::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Only identity encoding is supported"));
    }
    if headers.get_all(header::CONTENT_LENGTH).iter().count() > 1 {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "Invalid Content-Length"));
    }
    if let Some(length) = headers.get(header::CONTENT_LENGTH) {
        let value = length
            .to_str()
            .ok()
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "Invalid Content-Length"))?;
        if value > limit as u64 {
            return Err(body_limit_error(limit));
        }
    }
    Ok(())
}
fn body_limit_error(limit: usize) -> ApiError {
    ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, format!("Policy request exceeds {limit} bytes"))
}
async fn read_json<T: DeserializeOwned>(body: Body, headers: &HeaderMap, deadline: Instant) -> ApiResult<T> {
    read_json_limited(body, headers, deadline, MAX_POLICY_REQUEST_BYTES).await
}
async fn read_json_limited<T: DeserializeOwned>(mut body: Body, headers: &HeaderMap, deadline: Instant, limit: usize) -> ApiResult<T> {
    use http_body::Body as _;
    check_headers(headers, limit)?;
    let deadline = deadline.min(Instant::now() + BODY_TIMEOUT);
    tokio::time::timeout_at(deadline, async move {
        let mut bytes = Vec::new();
        while let Some(frame) = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
            let frame = frame.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "Policy body could not be read"))?;
            if let Ok(data) = frame.into_data() {
                if data.len() > limit - bytes.len() {
                    return Err(body_limit_error(limit));
                }
                bytes.extend_from_slice(&data);
            }
            tokio::task::yield_now().await;
        }
        serde_json::from_slice(&bytes).map_err(|error| {
            ApiError::new(
                if error.is_data() { StatusCode::UNPROCESSABLE_ENTITY } else { StatusCode::BAD_REQUEST },
                "Invalid strict policy JSON request",
            )
        })
    })
    .await
    .map_err(|_| expired())?
}
fn intent(value: PackageReviewIntent) -> &'static str {
    match value {
        PackageReviewIntent::Deny {} => "deny",
        PackageReviewIntent::DataOnly {} => "data_only",
        PackageReviewIntent::NativeFullServiceUserTrust {} => "native_full_service_user_trust",
        PackageReviewIntent::WasmSandboxRequired {} => "wasm_sandbox_required",
    }
}
fn review_value(review: &PackagePolicyReview) -> Value {
    json!({"package":review.request.package,"revision":review.revision,"intent":intent(review.request.intent),"grants":[],"expected_host_policy":review.request.expected_host_policy,"actor":review.actor.map(|a|a.0.to_string()),"created_at":payloads::timestamp(review.created_at)})
}
fn selection_value(value: &PackagePolicySelection) -> Value {
    json!({"plugin_id":value.plugin_id.to_string(),"revision":value.revision,"package":value.package,"review_revision":value.review_revision,"actor":value.actor.map(|a|a.0.to_string()),"updated_at":payloads::timestamp(value.updated_at)})
}
fn receipt_value(value: &PackagePolicyReceipt, historical: bool) -> Value {
    let (kind, plugin, package) = match &value.command {
        PackagePolicyCommand::Review(r) => ("review", r.package.plugin_id(), Some(&r.package)),
        PackagePolicyCommand::Select(r) => ("select", r.package.plugin_id(), Some(&r.package)),
        PackagePolicyCommand::Clear(r) => ("clear", &r.plugin_id, None),
    };
    let summary = match &value.command {
        PackagePolicyCommand::Review(r) => json!({"intent":intent(r.intent),"expected_review_revision":null}),
        PackagePolicyCommand::Select(r) => json!({"intent":null,"expected_review_revision":r.expected_review_revision}),
        PackagePolicyCommand::Clear(_) => json!({"intent":null,"expected_review_revision":null}),
    };
    json!({"operation_id":value.id.as_str(),"kind":kind,"plugin_id":plugin.to_string(),"package":package,"summary":summary,"result_revision":value.result_revision,"epoch":value.epoch,"host_policy":value.fingerprint,"actor":value.actor.map(|a|a.0.to_string()),"created_at":payloads::timestamp(value.created_at),"historical":historical})
}
fn gate_value(value: &PackagePolicyGate) -> Value {
    let state = match value.state {
        PackagePolicyGateState::Uninitialized => "uninitialized",
        PackagePolicyGateState::Open => "open",
        PackagePolicyGateState::Sealed => "sealed",
    };
    let seal=value.seal.as_ref().map(|s|{let reason=match s.reason{PackagePolicySealReason::HistoryCapacity=>"history_capacity",PackagePolicySealReason::ReceiptCapacity=>"receipt_capacity",PackagePolicySealReason::ReviewHeadCapacity=>"review_head_capacity",PackagePolicySealReason::SelectionHeadCapacity=>"selection_head_capacity",PackagePolicySealReason::RevisionExhausted=>"revision_exhausted"};json!({"reason":reason,"operation_id":s.operation.as_str(),"actor":s.actor.map(|a|a.0.to_string()),"sealed_at":payloads::timestamp(s.at)})});
    json!({"state":state,"epoch":value.epoch,"host_policy":value.fingerprint,"seal":seal})
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Fixture;
    use super::*;
    use axum::http::Request;
    use rbit_core::{Timestamp, TokenScope};
    use rbit_plugin_api::{ExpectedPolicyRevision, HostPolicyFingerprint, PackageManifest, PolicyGrantRequests, PolicyRevision};
    use rbit_services::{PackageInstallationDescriptor, Services, SignUp};
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::Poll,
    };
    use tower::ServiceExt as _;

    pub(super) async fn fixture() -> (Fixture, User, String) {
        let fixture = Fixture::new(|config| {
            config.actions.enabled = false;
            config.i18n.enabled = false;
            config.updates.check_for_updates = false;
        })
        .await;
        let admin = fixture
            .services
            .accounts
            .create_user(
                SignUp {
                    username: "policy-admin".into(),
                    email: "policy-admin@example.test".into(),
                    password: "correct horse battery".into(),
                },
                Some(true),
            )
            .await
            .unwrap();
        assert!(admin.is_admin);
        let token = fixture.token(&admin, TokenScope::Admin).await;
        (fixture, admin, token)
    }
    pub(super) fn stalled(polls: Arc<AtomicUsize>) -> Body {
        Body::from_stream(futures_util::stream::poll_fn(move |_| {
            polls.fetch_add(1, Ordering::SeqCst);
            Poll::<Option<Result<axum::body::Bytes, std::io::Error>>>::Pending
        }))
    }
    async fn send_stalled(fixture: &Fixture, credential: Option<&str>) -> (StatusCode, usize) {
        let polls = Arc::new(AtomicUsize::new(0));
        let mut request = Request::builder()
            .method("PUT")
            .uri("/api/v1/admin/plugin-policy-operations/unpolled")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = credential {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = tokio::time::timeout(Duration::from_secs(5), fixture.oneshot(request.body(stalled(Arc::clone(&polls))).unwrap()))
            .await
            .unwrap();
        (response.status(), polls.load(Ordering::SeqCst))
    }
    #[tokio::test]
    async fn scope_live_role_and_task_exclusion_precede_body_polling() {
        let (fixture, admin, admin_token) = fixture().await;
        let reader = fixture.user("ordinary-user").await;
        let nonadmin = fixture.token(&reader, TokenScope::Admin).await;
        let read = fixture.token(&admin, TokenScope::Read).await;
        let write = fixture.token(&admin, TokenScope::Write).await;
        for (token, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some(read.as_str()), StatusCode::FORBIDDEN),
            (Some(write.as_str()), StatusCode::FORBIDDEN),
            (Some(nonadmin.as_str()), StatusCode::FORBIDDEN),
        ] {
            assert_eq!(send_stalled(&fixture, token).await, (expected, 0));
        }
        let other = fixture
            .services
            .accounts
            .create_user(
                SignUp {
                    username: "other-admin".into(),
                    email: "other-admin@example.test".into(),
                    password: "correct horse battery".into(),
                },
                Some(true),
            )
            .await
            .unwrap();
        assert!(other.is_admin);
        assert_eq!(
            fixture.services.context().database.admin().set_admin(admin.id(), false).await.unwrap(),
            rbit_storage::sqlite::admin::Moderation::Applied
        );
        assert_eq!(send_stalled(&fixture, Some(&admin_token)).await, (StatusCode::FORBIDDEN, 0));
        assert!(super::super::auth::task_route(Put, OPERATIONS).is_none());
        assert!(super::super::auth::task_route(Post, "/admin/plugin-policy/prepare").is_none());
        fixture.services.shutdown().await;
    }
    #[tokio::test]
    async fn bounded_json_reads_exact_cap_chunked_overflow_and_stalled_deadline() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let mut exact = br#"{"schema_version":1}"#.to_vec();
        exact.resize(MAX_POLICY_REQUEST_BYTES, b' ');
        let parsed: Prepare = read_json(Body::from(exact.clone()), &headers, Instant::now() + COMMAND_TIMEOUT).await.unwrap();
        assert_eq!(parsed.schema_version, 1);
        // Unknown-length, multiple frames; whitespace is valid JSON framing until
        // the actual byte cap, so syntax rejection cannot stand in for quota.
        let chunks = vec![Ok::<_, std::io::Error>(axum::body::Bytes::from(exact)), Ok(axum::body::Bytes::from_static(b" "))];
        assert_eq!(
            read_json::<Prepare>(Body::from_stream(futures_util::stream::iter(chunks)), &headers, Instant::now() + COMMAND_TIMEOUT)
                .await
                .unwrap_err()
                .status,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        let polls = Arc::new(AtomicUsize::new(0));
        assert_eq!(
            read_json::<Prepare>(stalled(polls), &headers, Instant::now() + Duration::from_millis(10)).await.unwrap_err().status,
            StatusCode::REQUEST_TIMEOUT
        );
        assert_eq!(
            read_json::<Prepare>(Body::from(r#"{"schema_version":1,"enable":true}"#), &headers, Instant::now() + COMMAND_TIMEOUT)
                .await
                .unwrap_err()
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let malformed = read_json::<Prepare>(Body::from("{"), &headers, Instant::now() + COMMAND_TIMEOUT).await.unwrap_err();
        assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn infinite_ready_empty_frames_yield_and_obey_deadline() {
        use std::future::Future as _;
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let polls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&polls);
        let body = Body::from_stream(futures_util::stream::poll_fn(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(Some(Ok::<_, std::io::Error>(axum::body::Bytes::new())))
        }));
        let mut read = Box::pin(read_json::<Prepare>(body, &headers, Instant::now() + Duration::from_millis(20)));
        // A single manual poll must relinquish execution after exactly one
        // always-ready frame. Otherwise even the timeout cannot be polled.
        std::future::poll_fn(|cx| {
            assert!(read.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        let error = tokio::time::timeout(Duration::from_secs(1), read).await.unwrap().unwrap_err();
        assert_eq!(error.status, StatusCode::REQUEST_TIMEOUT);
    }
    pub(super) async fn seed_native(fixture: &Fixture, admin: &User) -> InstalledPackageRef {
        let manifest:PackageManifest=serde_json::from_value(json!({"schema_version":1,"plugin":{"id":"example.independent","version":"1.0.0+outside.server","host_api":"^1","capabilities":[]},"kind":{"type":"native","target":"x86_64-unknown-linux-gnu","entrypoint":"greeter"},"files":[{"path":"greeter","size":1,"sha256":"a".repeat(64)}],"budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}})).unwrap();
        // Trusted installation assertion fixture, not proof of archive materialization.
        let descriptor = PackageInstallationDescriptor::new(manifest, "b".repeat(64).parse().unwrap(), 4096).unwrap();
        let id = PackageOperationId::new("external-install").unwrap();
        let store = fixture.services.context().database.plugin_installations();
        store.begin(&id, &descriptor, Some(admin.id()), Timestamp::now()).await.unwrap();
        store.complete(&id, 0, Timestamp::now()).await.unwrap();
        InstalledPackageRef::new(
            descriptor.manifest().plugin.id.clone(),
            descriptor.manifest().plugin.version.clone(),
            descriptor.digest().clone(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn native_review_select_deny_historical_replay_and_restart_remain_passive() {
        let (fixture, admin, token) = fixture().await;
        let package = seed_native(&fixture, &admin).await;
        let (_, virgin) = fixture.send("GET", "/api/v1/admin/plugin-policy", Some(&token), None).await;
        assert_eq!(virgin["gate"]["state"], "uninitialized");
        let (response, absent) = fixture.send("GET", "/api/v1/admin/plugin-policy/candidates/example.unselected", Some(&token), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(absent["plugin_id"], "example.unselected");
        assert!(absent["selection"].is_null());
        let (response, prepared) =
            fixture.send("POST", "/api/v1/admin/plugin-policy/prepare", Some(&token), Some(json!({"schema_version":1}))).await;
        assert_eq!(response.status(), StatusCode::OK);
        let fp = HostPolicyFingerprint::new(prepared["gate"]["host_policy"].as_str().unwrap().parse().unwrap());
        let review = PackageReviewRequest::new(
            package.clone(),
            ExpectedPolicyRevision::Absent {},
            fp.clone(),
            PackageReviewIntent::NativeFullServiceUserTrust {},
            PolicyGrantRequests::new(Vec::new()).unwrap(),
        )
        .unwrap();
        let envelope = json!({"command":"review","request":review});
        let (response, applied) =
            fixture.send("PUT", "/api/v1/admin/plugin-policy-operations/native-review", Some(&token), Some(envelope.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(applied["execution"], "unavailable");
        assert_eq!(applied["receipt"]["result_revision"], "1");
        assert_eq!(applied["receipt"]["summary"]["intent"], "native_full_service_user_trust");
        assert!(applied["receipt"]["summary"]["expected_review_revision"].is_null());
        assert!(applied["receipt"].get("request").is_none());
        let select =
            PackageSelectionRequest::new(package.clone(), ExpectedPolicyRevision::Absent {}, PolicyRevision::new(1).unwrap(), fp.clone());
        let (response, selected) = fixture
            .send(
                "PUT",
                "/api/v1/admin/plugin-policy-operations/native-select",
                Some(&token),
                Some(json!({"command":"select","request":select})),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(selected["receipt"]["summary"]["intent"].is_null());
        assert_eq!(selected["receipt"]["summary"]["expected_review_revision"], "1");
        let deny = PackageReviewRequest::new(
            package.clone(),
            ExpectedPolicyRevision::Exact { revision: PolicyRevision::new(1).unwrap() },
            fp.clone(),
            PackageReviewIntent::Deny {},
            PolicyGrantRequests::new(Vec::new()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            fixture
                .send(
                    "PUT",
                    "/api/v1/admin/plugin-policy-operations/native-deny",
                    Some(&token),
                    Some(json!({"command":"review","request":deny}))
                )
                .await
                .0
                .status(),
            StatusCode::OK
        );
        let (_, historical) =
            fixture.send("PUT", "/api/v1/admin/plugin-policy-operations/native-review", Some(&token), Some(envelope)).await;
        assert_eq!(historical["outcome"], "historical_receipt");
        assert_eq!(historical["receipt"]["historical"], true);
        assert_eq!(historical["receipt"]["summary"]["intent"], "native_full_service_user_trust");
        let (response, receipt) = fixture.send("GET", "/api/v1/admin/plugin-policy-operations/native-deny", Some(&token), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(receipt["receipt"]["summary"]["intent"], "deny");
        let stale = PackageSelectionRequest::new(
            package.clone(),
            ExpectedPolicyRevision::Exact { revision: PolicyRevision::new(1).unwrap() },
            PolicyRevision::new(1).unwrap(),
            fp.clone(),
        );
        assert_eq!(
            fixture
                .send(
                    "PUT",
                    "/api/v1/admin/plugin-policy-operations/stale-selection",
                    Some(&token),
                    Some(json!({"command":"select","request":stale}))
                )
                .await
                .0
                .status(),
            StatusCode::CONFLICT
        );
        assert_restarted_policy_and_clear(&fixture, &token, &package, fp).await;
    }

    async fn assert_restarted_policy_and_clear(fixture: &Fixture, token: &str, package: &InstalledPackageRef, fp: HostPolicyFingerprint) {
        let context = fixture.services.context().clone();
        fixture.services.shutdown().await;
        // Fresh composition over the same durable DB, never a worker restart.
        let restarted = Services::new(Arc::clone(&context.config), &context.database, &context.kv).await.unwrap();
        let router = crate::router(crate::AppState::new(Arc::clone(&context.config), restarted.clone()));
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/plugin-policy/candidates/example.independent")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), RESPONSE_LIMIT).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["plugin_id"], "example.independent");
        assert_eq!(value["current_review"]["intent"], "deny");
        assert_eq!(value["execution"], "unavailable");
        assert_eq!(value["selection"]["review_revision"], "1");
        assert_eq!(value["current_review"]["revision"], "2");
        // Clear is selection-only: retain the revision tombstone while the exact
        // installed archive's deny decision remains independently inspectable.
        let clear = PackageSelectionClearRequest::new(
            package.plugin_id().clone(),
            ExpectedPolicyRevision::Exact { revision: PolicyRevision::new(1).unwrap() },
            fp,
        );
        for (key, expected) in [("native-clear", StatusCode::OK), ("stale-clear", StatusCode::CONFLICT)] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/api/v1/admin/plugin-policy-operations/{key}"))
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(serde_json::to_vec(&json!({"command":"clear","request":clear})).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::OK {
                let bytes = axum::body::to_bytes(response.into_body(), RESPONSE_LIMIT).await.unwrap();
                let cleared: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(cleared["outcome"], "applied");
                assert_eq!(cleared["execution"], "unavailable");
                assert_eq!(cleared["receipt"]["kind"], "clear");
                assert_eq!(cleared["receipt"]["plugin_id"], "example.independent");
                assert_eq!(cleared["receipt"]["result_revision"], "2");
                assert!(cleared["receipt"]["package"].is_null());
                assert_eq!(cleared["receipt"]["summary"], json!({"intent":null,"expected_review_revision":null}));
            }
        }
        let exact_path = format!(
            "/api/v1/admin/plugin-policy/packages/example.independent/versions/1.0.0%2Boutside.server/archives/{}",
            package.archive_digest().as_str()
        );
        for path in ["/api/v1/admin/plugin-policy/candidates/example.independent", exact_path.as_str()] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(path).header(header::AUTHORIZATION, format!("Bearer {token}")).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), RESPONSE_LIMIT).await.unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["execution"], "unavailable");
            if path == exact_path {
                assert_eq!(value["current_review"]["intent"], "deny");
                assert_eq!(value["current_review"]["revision"], "2");
            } else {
                assert_eq!(value["plugin_id"], "example.independent");
                assert_eq!(value["selection"]["revision"], "2");
                assert!(value["selection"]["package"].is_null());
                assert!(value["selection"]["review_revision"].is_null());
                assert!(value["current_review"].is_null());
            }
        }
        assert_eq!(context.database.plugin_settings().desired_enabled(package.plugin_id().as_str()).await.unwrap(), None);
        assert!(!context.config.storage.data_dir.join("plugins").exists());
        restarted.shutdown().await;
    }
    #[tokio::test(flavor = "current_thread")]
    async fn dropped_router_waiter_after_owned_admission_recovers_committed_receipt() {
        use futures_util::StreamExt as _;
        use sqlx::{ConnectOptions as _, Connection as _};
        use std::{future::Future as _, sync::atomic::AtomicBool};
        let (fixture, admin, token) = fixture().await;
        let package = seed_native(&fixture, &admin).await;
        let gate = fixture.services.package_policy.prepare(&admin).await.unwrap();
        let review = PackageReviewRequest::new(
            package,
            ExpectedPolicyRevision::Absent {},
            gate.fingerprint.unwrap(),
            PackageReviewIntent::NativeFullServiceUserTrust {},
            PolicyGrantRequests::new(Vec::new()).unwrap(),
        )
        .unwrap();
        let bytes = serde_json::to_vec(&json!({"command":"review","request":review})).unwrap();
        let eof = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&eof);
        let mut blocker = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(fixture.services.context().config.storage.sqlite_path())
            .connect()
            .await
            .unwrap();
        let (writer_sender, writer_receiver) = tokio::sync::oneshot::channel();
        let data = futures_util::stream::once(async move {
            // Caller authentication records PAT use with a write. Acquire only
            // when the handler polls its body, after that write and live-auth
            // preflight, so the barrier blocks the policy owner rather than auth.
            sqlx::query("BEGIN IMMEDIATE").execute(&mut blocker).await.unwrap();
            writer_sender.send(blocker).unwrap();
            Ok::<_, std::io::Error>(axum::body::Bytes::from(bytes))
        });
        let eof_frame = futures_util::stream::poll_fn(move |_| {
            observed.store(true, Ordering::SeqCst);
            Poll::Ready(None)
        });
        let body = Body::from_stream(data.chain(eof_frame));
        let path = "/api/v1/admin/plugin-policy-operations/abandoned-http-review";
        let request = Request::builder()
            .method("PUT")
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .unwrap();
        // The body acquires an independent writer before service admission,
        // preventing the owned transaction from finishing before waiter loss.
        let mut response = Box::pin(fixture.oneshot(request));
        tokio::time::timeout(
            Duration::from_secs(3),
            std::future::poll_fn(|cx| {
                assert!(response.as_mut().poll(cx).is_pending(), "writer barrier must prevent a completed mutation");
                if eof.load(Ordering::SeqCst) { Poll::Ready(()) } else { Poll::Pending }
            }),
        )
        .await
        .unwrap();
        // On this current-thread runtime, the poll that observes EOF continues
        // synchronously through strict JSON validation and service admission/spawn
        // to its oneshot receiver. No intervening await can return Pending there.
        // Thus this is a dropped admitted waiter, not a cancelled body read.
        drop(response);
        let mut blocker = writer_receiver.await.unwrap();
        sqlx::query("COMMIT").execute(&mut blocker).await.unwrap();
        blocker.close().await.unwrap();
        // A first read can win the connection before the newly spawned owner;
        // permit only a transient 404, bounded by the same recovery deadline.
        let recovered = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let (response, value) = fixture.send("GET", path, Some(&token), None).await;
                if response.status() == StatusCode::OK {
                    break value;
                }
                assert_eq!(response.status(), StatusCode::NOT_FOUND);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(recovered["receipt"]["operation_id"], "abandoned-http-review");
        assert_eq!(recovered["receipt"]["result_revision"], "1");
        assert_eq!(recovered["receipt"]["summary"]["intent"], "native_full_service_user_trust");
        assert_eq!(recovered["receipt"]["historical"], true);
        assert_eq!(recovered["execution"], "unavailable");
        // This is ordinary committed-receipt recovery, not injected uncertain COMMIT.
        fixture.services.shutdown().await;
    }
    #[test]
    fn policy_openapi_is_strict_and_covers_only_named_administration() {
        let document: Value = serde_json::from_str(include_str!("openapi/package_policy.json")).unwrap();
        assert_eq!(document["paths"].as_object().unwrap().len(), 6);
        assert_eq!(document["components"]["schemas"]["PolicyReviewRequest"]["properties"]["grants"]["maxItems"], 0);
        assert_eq!(document["components"]["schemas"]["PolicyExactPackage"]["additionalProperties"], false);
        for path in document["paths"].as_object().unwrap().keys() {
            assert!(!path.contains("activate"));
        }
        assert_eq!(routes().len(), 8);
    }
}
