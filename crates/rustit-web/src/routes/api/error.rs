//! JSON errors: `{"message": "…"}`, plus `"errors": [{"field", "message"}]` for 422.
//!
//! Status codes: 401 unauthenticated (with `WWW-Authenticate: Bearer`), 403 for callers
//! who may see the resource but not do this, 404 for anything the caller may not know
//! exists (and unknown paths), 409 conflicts, 413 oversized bodies, 422 validation and
//! 429 with `Retry-After`.

use axum::{
    Json,
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rustit_services::ServiceError;
use serde::Serialize;

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
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Serialize)]
struct Body<'a> {
    message: &'a str,
    #[serde(skip_serializing_if = "<[FieldError]>::is_empty")]
    errors: &'a [FieldError],
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into(), errors: Vec::new(), retry_after: None }
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
            ServiceError::RegistrationClosed => Self::forbidden(error.to_string()),
            ServiceError::InvalidCredentials => Self::unauthorized("Bad credentials"),
            ServiceError::RateLimited { retry_after_seconds } => Self::rate_limited(retry_after_seconds),
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
        let mut response = (self.status, Json(Body { message: &self.message, errors: &self.errors })).into_response();
        let headers = response.headers_mut();
        if self.status == StatusCode::UNAUTHORIZED {
            headers.insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer realm=\"Rustit\""));
        }
        if let Some(seconds) = self.retry_after {
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
    async fn errors_are_json_with_the_right_status() {
        let (status, json, headers) = body(ApiError::service(ServiceError::Invalid("name is too long".to_owned()), "name")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json,
            serde_json::json!({ "message": "Validation Failed", "errors": [{ "field": "name", "message": "name is too long" }] })
        );
        assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/json");

        let (status, json, _) = body(ServiceError::NotFound.into()).await;
        assert_eq!((status, json), (StatusCode::NOT_FOUND, serde_json::json!({ "message": "Not Found" })));

        let (status, _, headers) = body(ApiError::unauthorized("Bad credentials")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(headers.get(header::WWW_AUTHENTICATE).unwrap().to_str().unwrap().starts_with("Bearer"));

        let (status, _, headers) = body(ServiceError::RateLimited { retry_after_seconds: 7 }.into()).await;
        assert_eq!((status, headers.get(header::RETRY_AFTER).unwrap().to_str().unwrap()), (StatusCode::TOO_MANY_REQUESTS, "7"));

        let (status, json, _) = body(ServiceError::Conflict("repository name").into()).await;
        assert_eq!((status, json["message"].as_str()), (StatusCode::CONFLICT, Some("repository name is already taken")));
        let (status, _, _) = body(ServiceError::Forbidden.into()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}
