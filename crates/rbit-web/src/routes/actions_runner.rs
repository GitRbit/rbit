//! The runner endpoint: Forgejo Runner and Gitea `act_runner` speak Connect (binary
//! protobuf over `POST /api/actions/{service}/{method}`) to register, declare their labels,
//! fetch tasks and report state and logs.
//!
//! [`routes`] is merged into the application router only when `Services::actions` exists;
//! with Actions disabled these paths fall through to the API's JSON 404. It is merged outside
//! the page router: no session, no CSRF check, no HTML error pages. [`PATHS`] lists every
//! method this endpoint serves, for the disabled-is-disabled tests.
//!
//! [`RunnerCaller`] authenticates a runner call: `Authorization: Bearer rbrn_…`, checked
//! against the runner cache only (one digest, one map lookup, no SQLite). Only failed
//! authentication spends the client's `RunnerAuth` budget.
//!
//! The Actions foundation (workstream F) owns the route list; the server side of the runner
//! protocol (stream S) owns the handlers and the extractor.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::FromRequestParts,
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use rbit_services::{
    ServiceError, ThrottleScope,
    actions::runners::{CachedRunner, RunnerError},
};
use rbit_workflow::protocol::ErrorCode;

use crate::{AppState, client_ip::ClientIp};

/// Where the Connect services are mounted.
pub const PREFIX: &str = "/api/actions";

/// Every RPC of the endpoint, as a path below `/`.
pub const PATHS: &[&str] = &[
    "/api/actions/ping.v1.PingService/Ping",
    "/api/actions/runner.v1.RunnerService/Register",
    "/api/actions/runner.v1.RunnerService/Declare",
    "/api/actions/runner.v1.RunnerService/FetchTask",
    "/api/actions/runner.v1.RunnerService/UpdateTask",
    "/api/actions/runner.v1.RunnerService/UpdateLog",
];

/// The runner endpoint. Merged only when Actions is enabled (see `routes::router`).
pub fn routes() -> Router<AppState> {
    Router::new()
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

/// The runner making a call, authenticated against the runner cache.
#[derive(Debug, Clone)]
pub struct RunnerCaller(pub Arc<CachedRunner>);

impl FromRequestParts<AppState> for RunnerCaller {
    type Rejection = Refusal;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let Some(actions) = &state.services.actions else {
            return Err(RunnerError::not_found("not found").into());
        };
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
        if runner.disabled {
            return Err(RunnerError::forbidden("this runner is disabled; an administrator can enable it again").into());
        }
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
    use super::*;

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
}
