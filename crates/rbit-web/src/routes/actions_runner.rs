//! The runner endpoint: protocol v1 of `rbit-runner` (`docs/design/actions.md` §12.6), JSON
//! over `POST /api/actions/runner/v1/…`.
//!
//! [`routes`] is merged into the application router only when `Services::actions` exists;
//! with Actions disabled these paths fall through to the API's JSON 404. It is merged outside
//! the page router — no session, no CSRF check, no HTML error pages, no 30 s page timeout —
//! with its own 30 s limit, which the long poll's at most 25 s fits in. [`PATHS`] lists every
//! path this endpoint serves, for the disabled-is-disabled tests.
//!
//! Every call:
//!
//! * carries `Rbit-Runner-Protocol` with a version this server speaks, or is answered `426`
//!   with the versions it does;
//! * has a body within its endpoint's cap (`Endpoint::body_cap`), counted after
//!   decompression: `Content-Encoding: gzip` is accepted when the body inflates at most a
//!   hundredfold (`MAX_GZIP_RATIO`), anything else is `413`;
//! * is answered, when refused, with `{"code", "message"}` and the code's status
//!   ([`Refusal`]).
//!
//! Every call counts in `rbit_actions_rpc_total{method, code}`: `method` names the call
//! (`poll`, `task_state`, `artifact_upload`, …), `code` is `ok` or the refusal's
//! [`ErrorCode`] (`unauthenticated`, `throttled`, …), both from fixed sets.
//!
//! [`RunnerCaller`] authenticates a runner call: `Authorization: Bearer rbrn_…`, checked
//! against the runner cache only (one digest, one map lookup, no SQLite). Only failed
//! authentication spends the client's `RunnerAuth` budget; registrations spend
//! `RunnerRegister`, and a wrong registration token costs extra.
//!
//! The Actions foundation (workstream F) owns the route list; the server side of the runner
//! protocol (stream S) owns the handlers and the extractor.

use std::{io::Read as _, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    body::Body,
    extract::{FromRequestParts, MatchedPath, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    middleware::{Next, from_fn},
    response::{IntoResponse, Response},
    routing::post,
};
use rbit_services::{
    ServiceError, ThrottleScope,
    actions::{
        artifacts::Upload,
        runners::{CachedRunner, RunnerError},
    },
};
use rbit_workflow::protocol::{
    ARTIFACTS_PATH, ArtifactInfo, ArtifactList, ArtifactQuery, BASE_PATH, CONTENT_SHA256_HEADER, DeclareRequest, Endpoint, ErrorBody,
    ErrorCode, IDEMPOTENCY_KEY_HEADER, LogsQuery, MAX_GZIP_RATIO, PROTOCOL_HEADER, PollRequest, PollResponse, RegisterRequest,
    SUPPORTED_PROTOCOL_VERSIONS, TaskStateRequest, UnregisterRequest,
};
use serde::de::DeserializeOwned;
use tower_http::timeout::TimeoutLayer;

use crate::{AppState, client_ip::ClientIp};

/// Where the protocol is mounted.
pub const PREFIX: &str = BASE_PATH;

/// Every path of the endpoint (`{id}` is a task id), in `Endpoint::ALL` order.
pub const PATHS: &[&str] = &[
    "/api/actions/runner/v1/register",
    "/api/actions/runner/v1/declare",
    "/api/actions/runner/v1/poll",
    "/api/actions/runner/v1/tasks/{id}/state",
    "/api/actions/runner/v1/tasks/{id}/logs",
    "/api/actions/runner/v1/unregister",
    // Actions: artifacts (A).
    "/api/actions/runner/v1/tasks/{id}/artifacts",
    "/api/actions/runner/v1/tasks/{id}/artifacts/{name}",
];

// Actions: artifacts (A).
/// Uploads and downloads stream up to `[actions] max_artifact_bytes`, so they get their
/// own limit instead of [`CALL_TIMEOUT`].
const ARTIFACT_TIMEOUT: Duration = Duration::from_hours(1);

/// The endpoint's own limit; outside it, the page and API timeouts do not apply.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// The runner endpoint. Merged only when Actions is enabled (see `routes::router`).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route(&path(Endpoint::Register), post(register))
        .route(&path(Endpoint::Declare), post(declare))
        .route(&path(Endpoint::Poll), post(poll))
        .route(&path(Endpoint::TaskState), post(task_state))
        .route(&path(Endpoint::TaskLogs), post(task_logs))
        .route(&path(Endpoint::Unregister), post(unregister))
        .layer(from_fn(require_protocol))
        .layer(TimeoutLayer::with_status_code(StatusCode::SERVICE_UNAVAILABLE, CALL_TIMEOUT))
        // Actions: metrics.
        .layer(from_fn(count_call))
        // Actions: artifacts (A).
        .merge(
            Router::new()
                .route(&format!("{BASE_PATH}/{ARTIFACTS_PATH}"), axum::routing::get(list_artifacts))
                .route(&format!("{BASE_PATH}/{ARTIFACTS_PATH}/{{name}}"), axum::routing::get(download_artifact).put(upload_artifact))
                .layer(from_fn(require_protocol))
                .layer(TimeoutLayer::with_status_code(StatusCode::SERVICE_UNAVAILABLE, ARTIFACT_TIMEOUT))
                .layer(from_fn(count_call)),
        )
}

// Actions: metrics.
/// Counts the call in `rbit_actions_rpc_total`, refusals and time-outs included.
async fn count_call(request: Request, next: Next) -> Response {
    let method = request.extensions().get::<MatchedPath>().map_or("unknown", |matched| call_name(matched.as_str(), request.method()));
    let response = next.run(request).await;
    metrics::counter!("rbit_actions_rpc_total", "method" => method, "code" => call_code(response.status())).increment(1);
    response
}

/// The `method` label of a call to `path` (a route template).
fn call_name(path: &str, method: &axum::http::Method) -> &'static str {
    let path = path.strip_prefix(BASE_PATH).and_then(|path| path.strip_prefix('/')).unwrap_or(path);
    if let Some(endpoint) = Endpoint::ALL.iter().find(|endpoint| endpoint.path() == path) {
        return match endpoint {
            Endpoint::Register => "register",
            Endpoint::Declare => "declare",
            Endpoint::Poll => "poll",
            Endpoint::TaskState => "task_state",
            Endpoint::TaskLogs => "task_logs",
            Endpoint::Unregister => "unregister",
        };
    }
    match (path == ARTIFACTS_PATH, *method == axum::http::Method::PUT) {
        (true, _) => "artifact_list",
        (false, true) => "artifact_upload",
        (false, false) => "artifact_download",
    }
}

/// The `code` label of a response: `ok`, or the [`ErrorCode`] its status stands for.
fn call_code(status: StatusCode) -> &'static str {
    if status.is_success() {
        return "ok";
    }
    match status.as_u16() {
        400 => "bad_request",
        401 => "unauthenticated",
        403 => "forbidden",
        404 => "not_found",
        409 => "conflict",
        413 => "too_large",
        426 => "unsupported_protocol",
        429 => "throttled",
        503 => "unavailable",
        _ => "other",
    }
}

fn path(endpoint: Endpoint) -> String {
    format!("{BASE_PATH}/{}", endpoint.path())
}

// ------------------------------------------------------------------ handlers

/// `POST register`: a registration token for a runner credential.
async fn register(State(state): State<AppState>, ClientIp(ip): ClientIp, headers: HeaderMap, body: Body) -> Result<Response, Refusal> {
    let actions = actions(&state)?;
    let address = ip.to_canonical().to_string();
    let throttle = &state.services.throttle;
    throttle.check_client(ThrottleScope::RunnerRegister, &address)?;
    let request: RegisterRequest = read_json(&headers, body, Endpoint::Register).await?;
    match actions.register_runner(&request).await {
        Ok(response) => Ok(Json(response).into_response()),
        Err(error) => {
            if error.code() == ErrorCode::Unauthenticated {
                throttle.record_client_failure(ThrottleScope::RunnerRegister, &address);
            }
            Err(error.into())
        }
    }
}

/// `POST declare`: labels and capabilities at the runner's start.
async fn declare(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Refusal> {
    let request: DeclareRequest = read_json(&headers, body, Endpoint::Declare).await?;
    let response = actions(&state)?.declare_runner(&runner, &request).await?;
    Ok(Json(response).into_response())
}

/// `POST poll`: one task, or `204` once the long poll has passed. The answer carries the
/// task's secrets and token, so nothing may keep it.
async fn poll(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Refusal> {
    let request: PollRequest = read_json(&headers, body, Endpoint::Poll).await?;
    let key = match headers.get(IDEMPOTENCY_KEY_HEADER) {
        Some(value) => Some(value.to_str().map_err(|_| RunnerError::bad_request("the Idempotency-Key is not ASCII"))?),
        None => None,
    };
    let Some(task) = actions(&state)?.poll(&runner, &request, key).await? else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(PollResponse { task })).into_response())
}

/// `POST tasks/{id}/state`: heartbeat, steps, outputs and the result; answers `{cancel}`.
async fn task_state(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Refusal> {
    let task = task_id(&id)?;
    let request: TaskStateRequest = read_json(&headers, body, Endpoint::TaskState).await?;
    let response = actions(&state)?.task_state(&runner, task, &request).await?;
    Ok(Json(response).into_response())
}

/// `POST tasks/{id}/logs?from=<line>`: `text/plain` lines from line `from`; answers the
/// next line, or `409` with it on a gap.
async fn task_logs(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path(id): Path<String>,
    query: Result<Query<LogsQuery>, axum::extract::rejection::QueryRejection>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Refusal> {
    let task = task_id(&id)?;
    let Ok(Query(query)) = query else {
        return Err(RunnerError::bad_request("the query must be `from=<line>`").into());
    };
    let body = read_body(&headers, body, Endpoint::TaskLogs).await?;
    let response = actions(&state)?.append_log(&runner, task, query.from, &body).await?;
    Ok(Json(response).into_response())
}

// Actions: artifacts (A).
fn artifact_info(artifact: &rbit_core::actions::Artifact) -> ArtifactInfo {
    let sha256 = artifact.sha256.iter().fold(String::with_capacity(64), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    ArtifactInfo { name: artifact.name.clone(), size: artifact.size, sha256 }
}

/// `PUT tasks/{id}/artifacts/{name}`: the tar.gz body, streamed; answers `201` with it.
async fn upload_artifact(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path((id, name)): Path<(String, String)>,
    query: Result<Query<ArtifactQuery>, axum::extract::rejection::QueryRejection>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Refusal> {
    let task = task_id(&id)?;
    let Ok(Query(query)) = query else {
        return Err(RunnerError::bad_request("the query is `retention_days=<days>&overwrite=<bool>`").into());
    };
    let digest = headers
        .get(CONTENT_SHA256_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| RunnerError::bad_request(format!("an upload needs the {CONTENT_SHA256_HEADER} header")))?;
    let upload = Upload { name: &name, sha256: digest, retention_days: query.retention_days, overwrite: query.overwrite };
    let reader = tokio_util::io::StreamReader::new(futures_util::TryStreamExt::map_err(body.into_data_stream(), |error| {
        std::io::Error::other(error.to_string())
    }));
    let artifact = actions(&state)?.upload_artifact(&runner, task, &upload, reader).await?;
    Ok((StatusCode::CREATED, Json(artifact_info(&artifact))).into_response())
}

/// `GET tasks/{id}/artifacts`: the run's artifacts so far.
async fn list_artifacts(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path(id): Path<String>,
) -> Result<Response, Refusal> {
    let artifacts = actions(&state)?.task_artifacts(&runner, task_id(&id)?).await?;
    Ok(Json(ArtifactList { artifacts: artifacts.iter().map(artifact_info).collect() }).into_response())
}

/// `GET tasks/{id}/artifacts/{name}`: one artifact's tar.gz, with its digest.
async fn download_artifact(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    Path((id, name)): Path<(String, String)>,
) -> Result<Response, Refusal> {
    let (artifact, file) = actions(&state)?.open_task_artifact(&runner, task_id(&id)?, &name).await?;
    let info = artifact_info(&artifact);
    let mut response = Body::from_stream(tokio_util::io::ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/gzip"));
    if let Ok(value) = HeaderValue::from_str(&info.sha256) {
        headers.insert(CONTENT_SHA256_HEADER, value);
    }
    if let Ok(value) = HeaderValue::from_str(&artifact.size.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    }
    Ok(response)
}

/// A task id from the path; anything else names no task.
fn task_id(id: &str) -> Result<rbit_core::actions::TaskId, Refusal> {
    id.parse()
        .ok()
        .filter(|id| *id > 0)
        .map(rbit_core::actions::TaskId)
        .ok_or_else(|| RunnerError::not_found("there is no such task").into())
}

/// `POST unregister`: the runner removes itself.
async fn unregister(
    State(state): State<AppState>,
    RunnerCaller(runner): RunnerCaller,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Refusal> {
    let bytes = read_body(&headers, body, Endpoint::Unregister).await?;
    if !bytes.is_empty() {
        parse_json::<UnregisterRequest>(&bytes)?;
    }
    actions(&state)?.unregister_runner(&runner).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn actions(state: &AppState) -> Result<&rbit_services::ActionsService, Refusal> {
    state.services.actions.as_ref().ok_or_else(|| RunnerError::not_found("not found").into())
}

// ------------------------------------------------------------------ the envelope

/// Refuses calls without a protocol version this server speaks: `426` with the versions.
async fn require_protocol(request: Request, next: Next) -> Response {
    let version = request.headers().get(PROTOCOL_HEADER).and_then(|value| value.to_str().ok()).and_then(|value| value.trim().parse().ok());
    if version.is_some_and(|version: u32| SUPPORTED_PROTOCOL_VERSIONS.contains(&version)) {
        next.run(request).await
    } else {
        (StatusCode::UPGRADE_REQUIRED, Json(ErrorBody::unsupported_protocol())).into_response()
    }
}

/// The request body of `endpoint`, decompressed, within its cap.
pub(crate) async fn read_body(headers: &HeaderMap, body: Body, endpoint: Endpoint) -> Result<Vec<u8>, Refusal> {
    let cap = endpoint.body_cap();
    let too_large = || RunnerError::too_large(format!("the body of this call may be at most {cap} bytes"));
    let raw = axum::body::to_bytes(body, cap).await.map_err(|_| too_large())?;
    let encoding = headers.get(header::CONTENT_ENCODING).and_then(|value| value.to_str().ok()).map_or("identity", str::trim);
    if encoding.eq_ignore_ascii_case("identity") || encoding.is_empty() {
        return Ok(raw.to_vec());
    }
    if !encoding.eq_ignore_ascii_case("gzip") {
        return Err(RunnerError::bad_request(format!("Content-Encoding `{encoding}` is not supported; use gzip or none")).into());
    }
    let ratio_cap = u64::try_from(raw.len()).unwrap_or(u64::MAX).saturating_mul(MAX_GZIP_RATIO);
    let limit = u64::try_from(cap).unwrap_or(u64::MAX).min(ratio_cap);
    // Small and bounded (at most the cap), so it is inflated in place.
    let mut inflated = Vec::new();
    flate2::read::GzDecoder::new(&raw[..])
        .take(limit.saturating_add(1))
        .read_to_end(&mut inflated)
        .map_err(|_| RunnerError::bad_request("the gzip body is malformed"))?;
    if u64::try_from(inflated.len()).unwrap_or(u64::MAX) > limit {
        return Err(RunnerError::too_large(format!(
            "the body may be at most {cap} bytes after decompression, and inflate at most {MAX_GZIP_RATIO} times"
        ))
        .into());
    }
    Ok(inflated)
}

async fn read_json<T: DeserializeOwned>(headers: &HeaderMap, body: Body, endpoint: Endpoint) -> Result<T, Refusal> {
    let bytes = read_body(headers, body, endpoint).await?;
    parse_json(&bytes)
}

fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Refusal> {
    // The message names the field and position, never echoes the value (it may be a secret).
    serde_json::from_slice(bytes).map_err(|error| {
        RunnerError::bad_request(format!("the body is not a valid message (line {}, column {})", error.line(), error.column())).into()
    })
}

/// A protocol error as sent: the [`ErrorCode`]'s status, `{"code", "message", …}` and, when
/// throttled, `Retry-After`.
#[derive(Debug)]
pub struct Refusal(pub RunnerError);

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.code().status()).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
        let mut response = (status, Json(&self.0.body)).into_response();
        if let Some(seconds) = self.0.retry_after_seconds {
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        if self.0.code() == ErrorCode::Unauthenticated {
            response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer realm=\"rbit-runner\""));
        }
        response
    }
}

impl From<RunnerError> for Refusal {
    fn from(error: RunnerError) -> Self {
        Self(error)
    }
}

impl From<ServiceError> for Refusal {
    fn from(error: ServiceError) -> Self {
        Self(error.into())
    }
}

/// The runner making a call, authenticated against the runner cache. Disabled runners are
/// let through: they may still report on their tasks, only taking new ones is refused.
#[derive(Debug, Clone)]
pub struct RunnerCaller(pub Arc<CachedRunner>);

impl FromRequestParts<AppState> for RunnerCaller {
    type Rejection = Refusal;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let actions = actions(state)?;
        let ClientIp(ip) = ClientIp::resolve(&parts.extensions, &parts.headers, &state.config.security);
        let address = ip.to_canonical().to_string();
        let throttle = &state.services.throttle;
        throttle.check_client_failures(ThrottleScope::RunnerAuth, &address)?;
        let credential = parts.headers.get(header::AUTHORIZATION).and_then(bearer);
        let runner = match credential {
            Some(credential) => actions.authenticate_runner(credential).await?,
            None => None,
        };
        let Some(runner) = runner else {
            throttle.record_client_failure(ThrottleScope::RunnerAuth, &address);
            return Err(RunnerError::unauthenticated().into());
        };
        Ok(Self(runner))
    }
}

/// The credential of `Authorization: Bearer <credential>`.
fn bearer(value: &HeaderValue) -> Option<&str> {
    let (scheme, credential) = value.to_str().ok()?.trim().split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| credential.trim()).filter(|credential| !credential.is_empty())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    #[test]
    fn paths_are_the_protocols() {
        let mut expected: Vec<String> = Endpoint::ALL.iter().map(|endpoint| path(*endpoint)).collect();
        // Actions: artifacts (A).
        expected.extend([format!("{BASE_PATH}/{ARTIFACTS_PATH}"), format!("{BASE_PATH}/{ARTIFACTS_PATH}/{{name}}")]);
        assert_eq!(PATHS, expected);
    }

    #[test]
    fn reads_bearer_credentials_only() {
        let value = |text: &'static str| bearer(&HeaderValue::from_static(text)).map(str::to_owned);
        assert_eq!(value("Bearer rbrn_abc").as_deref(), Some("rbrn_abc"));
        assert_eq!(value("bearer  rbrn_abc ").as_deref(), Some("rbrn_abc"));
        assert_eq!(value("Basic cmJybl9hYmM="), None);
        assert_eq!(value("Bearer "), None);
    }

    #[tokio::test]
    async fn refusals_carry_their_status_and_headers() {
        let response = Refusal(RunnerError::throttled(9)).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "9");
        let response = Refusal(RunnerError::unauthenticated()).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], "unauthenticated");
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    async fn read(encoding: Option<&'static str>, body: Vec<u8>, endpoint: Endpoint) -> Result<Vec<u8>, ErrorCode> {
        let mut headers = HeaderMap::new();
        if let Some(encoding) = encoding {
            headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }
        read_body(&headers, Body::from(body), endpoint).await.map_err(|refusal| refusal.0.code())
    }

    #[tokio::test]
    async fn bodies_are_capped_before_and_after_decompression() {
        let cap = Endpoint::Declare.body_cap();
        assert_eq!(read(None, b"{}".to_vec(), Endpoint::Declare).await.unwrap(), b"{}");
        assert_eq!(read(None, vec![b' '; cap + 1], Endpoint::Declare).await, Err(ErrorCode::TooLarge));
        let text = br#"{"name":"build-1"}"#.repeat(10);
        assert_eq!(read(Some("gzip"), gzip(&text), Endpoint::Declare).await.unwrap(), text);
        // Within the cap, but inflating far more than a hundredfold.
        let bomb = gzip(&vec![b' '; cap]);
        assert!(bomb.len() * 100 < cap, "{} compressed bytes", bomb.len());
        assert_eq!(read(Some("gzip"), bomb, Endpoint::Declare).await, Err(ErrorCode::TooLarge));
        // Within the ratio, but past the cap once inflated.
        let mut seed = 7u32;
        let hex: Vec<u8> = (0..cap + 10)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                b"0123456789abcdef"[(seed >> 16) as usize % 16]
            })
            .collect();
        let big = gzip(&hex);
        assert!(big.len() < cap);
        assert_eq!(read(Some("gzip"), big, Endpoint::Declare).await, Err(ErrorCode::TooLarge));
        assert_eq!(read(Some("gzip"), b"not gzip".to_vec(), Endpoint::Declare).await, Err(ErrorCode::BadRequest));
        assert_eq!(read(Some("br"), b"{}".to_vec(), Endpoint::Declare).await, Err(ErrorCode::BadRequest));
    }
}
