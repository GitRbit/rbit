//! JSON errors: `{"message": "…", "code": "…"}`, plus `"errors": [{"field", "message"}]` for
//! 422. `code` is rbit's own, a stable word clients can branch on (see [`ApiError::code`]);
//! `message` is for people and may change.
//!
//! Status codes: 401 unauthenticated (with `WWW-Authenticate: Bearer`), 403 for callers
//! who may see the resource but not do this, 404 for anything the caller may not know
//! exists (and unknown paths), 409 conflicts, 413 oversized bodies, 422 validation and
//! 429 with `Retry-After`, or generic retryable 503 without a credential challenge.
//! Old names of renamed or transferred repositories answer like
//! GitHub: `301 Moved Permanently` for reads and `307 Temporary Redirect` for everything
//! else, with `Location` and a `url` field naming the same endpoint under the new name.

use axum::{
    Json,
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_services::ServiceError;
use serde::Serialize;

use crate::error::{UNAVAILABLE_MESSAGE, unavailable_response};

/// One invalid field of a 422.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    pub errors: Vec<FieldError>,
    pub retry_after: Option<u64>,
    /// Where a moved resource lives now (`Location` and the body's `url`).
    pub location: Option<String>,
    /// A more precise `code` than the status implies (`head_changed` for a 409, …).
    pub specific: Option<&'static str>,
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Serialize)]
struct Body<'a> {
    message: &'a str,
    code: &'a str,
    #[serde(skip_serializing_if = "<[FieldError]>::is_empty")]
    errors: &'a [FieldError],
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<&'a str>,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into(), errors: Vec::new(), retry_after: None, location: None, specific: None }
    }

    /// This error with `code` instead of the one its status implies.
    #[must_use]
    pub fn with_code(self, code: &'static str) -> Self {
        Self { specific: Some(code), ..self }
    }

    /// The stable code of the body: `not_found`, `unauthenticated`, `forbidden`,
    /// `conflict` (or a precise kind of it, such as `head_changed`), `invalid`,
    /// `rate_limited`, `unavailable`, `moved` or `internal`.
    pub fn code(&self) -> &'static str {
        self.specific.unwrap_or(match self.status.as_u16() {
            401 => "unauthenticated",
            403 => "forbidden",
            404 | 410 => "not_found",
            405 => "blocked",
            409 | 412 => "conflict",
            429 => "rate_limited",
            301 | 307 | 308 => "moved",
            400..=499 => "invalid",
            503 => "unavailable",
            _ => "internal",
        })
    }

    /// The resource is at `location` now: `301` for `GET` and `HEAD`, `307` (which keeps
    /// the method and body) for everything else, as on GitHub.
    pub fn moved(method: &Method, location: String) -> Self {
        let status =
            if matches!(*method, Method::GET | Method::HEAD) { StatusCode::MOVED_PERMANENTLY } else { StatusCode::TEMPORARY_REDIRECT };
        Self { location: Some(location), ..Self::new(status, status.canonical_reason().unwrap_or("Moved")) }
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not Found")
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    /// A 422 naming the offending field.
    pub fn invalid(field: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            errors: vec![FieldError { field: field.to_owned(), message: message.clone() }],
            message: "Validation Failed".to_owned(),
            retry_after: None,
            location: None,
            specific: None,
        }
    }

    pub fn rate_limited(retry_after_seconds: u64) -> Self {
        Self { retry_after: Some(retry_after_seconds), ..Self::new(StatusCode::TOO_MANY_REQUESTS, "API rate limit exceeded; retry later") }
    }

    /// Maps a service error, attributing validation failures to `field`.
    pub fn service(error: ServiceError, field: &str) -> Self {
        match error {
            ServiceError::Invalid(message) => Self::invalid(field, message),
            ServiceError::Conflict(what) => Self::new(StatusCode::CONFLICT, format!("{what} is already taken")),
            ServiceError::NotFound => Self::not_found(),
            ServiceError::Forbidden => Self::forbidden("You do not have permission to do that"),
            // GitHub's wording, which clients may match on.
            ServiceError::Archived => Self::forbidden("Repository was archived so is read-only."),
            ServiceError::RegistrationClosed => Self::forbidden(error.to_string()),
            ServiceError::InvalidCredentials => Self::unauthorized("Bad credentials"),
            ServiceError::RateLimited { retry_after_seconds } => Self::rate_limited(retry_after_seconds),
            ServiceError::Unavailable { retry_after_seconds } => {
                Self { retry_after: Some(retry_after_seconds), ..Self::new(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE_MESSAGE) }
            }
            ServiceError::Internal(source) => {
                tracing::error!(error = %source, "API request failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "An unexpected error occurred. It has been logged.")
            }
        }
    }
}

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        Self::service(error, "body")
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        let status = rejection.status();
        match status {
            StatusCode::PAYLOAD_TOO_LARGE => Self::new(status, "Request body is too large (at most 1 MiB)"),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Self::new(status, "Send the body as application/json"),
            StatusCode::UNPROCESSABLE_ENTITY => Self::invalid("body", rejection.body_text()),
            _ => Self::new(StatusCode::BAD_REQUEST, format!("Problems parsing JSON: {}", rejection.body_text())),
        }
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::invalid("query", rejection.body_text())
    }
}

impl From<PathRejection> for ApiError {
    fn from(_: PathRejection) -> Self {
        Self::not_found()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Body { message: &self.message, code: self.code(), errors: &self.errors, url: self.location.as_deref() };
        let mut response = (self.status, Json(body)).into_response();
        let headers = response.headers_mut();
        if let Some(location) = self.location.as_deref().and_then(|location| HeaderValue::from_str(location).ok()) {
            headers.insert(header::LOCATION, location);
        }
        if self.status == StatusCode::UNAUTHORIZED {
            headers.insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer realm=\"rbit\""));
        }
        if let Some(seconds) = self.retry_after {
            if self.status == StatusCode::SERVICE_UNAVAILABLE {
                return unavailable_response(response, seconds);
            }
            headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body(error: ApiError) -> (StatusCode, serde_json::Value, axum::http::HeaderMap) {
        let response = error.into_response();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap(), headers)
    }

    #[tokio::test]
    async fn unavailable_keeps_json_without_validation_details_or_bearer_challenge() {
        let (status, json, headers) = body(ServiceError::Unavailable { retry_after_seconds: 5 }.into()).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(headers[header::RETRY_AFTER], "5");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert!(!headers.contains_key(header::WWW_AUTHENTICATE));
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");
        assert_eq!(json, serde_json::json!({ "message": UNAVAILABLE_MESSAGE, "code": "unavailable" }));
    }

    #[tokio::test]
    async fn moves_redirect_and_keep_the_method_like_github() {
        let (status, json, headers) = body(ApiError::moved(&Method::GET, "https://git.example.com/api/v1/repos/bob/demo".into())).await;
        assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
        assert_eq!(headers[header::LOCATION], "https://git.example.com/api/v1/repos/bob/demo");
        assert_eq!(
            json,
            serde_json::json!({ "message": "Moved Permanently", "code": "moved", "url": "https://git.example.com/api/v1/repos/bob/demo" })
        );
        assert_eq!(ApiError::moved(&Method::HEAD, "/x".into()).status, StatusCode::MOVED_PERMANENTLY);
        let (status, json, _) = body(ApiError::moved(&Method::PATCH, "/x".into())).await;
        assert_eq!((status, json["message"].as_str()), (StatusCode::TEMPORARY_REDIRECT, Some("Temporary Redirect")));
    }

    #[tokio::test]
    async fn errors_are_json_with_the_right_status() {
        let (status, json, headers) = body(ApiError::service(ServiceError::Invalid("name is too long".to_owned()), "name")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json,
            serde_json::json!({
                "message": "Validation Failed",
                "code": "invalid",
                "errors": [{ "field": "name", "message": "name is too long" }]
            })
        );
        assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/json");

        let (status, json, _) = body(ServiceError::NotFound.into()).await;
        assert_eq!((status, json), (StatusCode::NOT_FOUND, serde_json::json!({ "message": "Not Found", "code": "not_found" })));

        let (status, _, headers) = body(ApiError::unauthorized("Bad credentials")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(headers.get(header::WWW_AUTHENTICATE).unwrap().to_str().unwrap().starts_with("Bearer"));

        let (status, _, headers) = body(ServiceError::RateLimited { retry_after_seconds: 7 }.into()).await;
        assert_eq!((status, headers.get(header::RETRY_AFTER).unwrap().to_str().unwrap()), (StatusCode::TOO_MANY_REQUESTS, "7"));

        let (status, json, _) = body(ServiceError::Conflict("repository name").into()).await;
        assert_eq!((status, json["message"].as_str()), (StatusCode::CONFLICT, Some("repository name is already taken")));
        let (status, _, _) = body(ServiceError::Forbidden.into()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, json, _) = body(ServiceError::Archived.into()).await;
        assert_eq!((status, json["message"].as_str()), (StatusCode::FORBIDDEN, Some("Repository was archived so is read-only.")));
    }
    #[tokio::test]
    async fn every_error_carries_a_stable_code() {
        for (error, code) in [
            (ApiError::unauthorized("Bad credentials"), "unauthenticated"),
            (ServiceError::Forbidden.into(), "forbidden"),
            (ServiceError::Conflict("name").into(), "conflict"),
            (ApiError::new(StatusCode::CONFLICT, "moved").with_code("head_changed"), "head_changed"),
            (ApiError::new(StatusCode::METHOD_NOT_ALLOWED, "required check missing"), "blocked"),
            (ServiceError::RateLimited { retry_after_seconds: 1 }.into(), "rate_limited"),
            (ApiError::new(StatusCode::BAD_REQUEST, "bad"), "invalid"),
            (ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "oops"), "internal"),
        ] {
            assert_eq!(body(error).await.1["code"], code);
        }
    }
}
