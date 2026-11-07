//! LFS error responses retain their JSON media type and retry/privacy headers.

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_services::{ServiceError, mirrors};
use serde_json::json;

use super::json_response;
use crate::{
    error::{UNAVAILABLE_MESSAGE, unavailable_response},
    routes::git_http::Denied,
};

// ------------------------------------------------------------------ errors

/// A refused LFS request, answered as `{"message": …}` with the LFS media type.
#[derive(Debug)]
pub(crate) enum LfsError {
    /// `401`: ask for credentials.
    Challenge,
    Status(StatusCode, String),
    RateLimited {
        retry_after_seconds: u64,
    },
    Unavailable {
        retry_after_seconds: u64,
    },
}

impl LfsError {
    pub(super) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self::Status(status, message.into())
    }

    pub(super) fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found")
    }
}

impl IntoResponse for LfsError {
    fn into_response(self) -> Response {
        match self {
            Self::Challenge => {
                let mut response = json_response(StatusCode::UNAUTHORIZED, &json!({ "message": "Credentials needed" }));
                let challenge = HeaderValue::from_static("Basic realm=\"rbit\", charset=\"UTF-8\"");
                response.headers_mut().insert("lfs-authenticate", challenge.clone());
                response.headers_mut().insert(header::WWW_AUTHENTICATE, challenge);
                response
            }
            Self::Status(status, message) => json_response(status, &json!({ "message": message })),
            Self::RateLimited { retry_after_seconds } => {
                let message = format!("too many authentication attempts; try again in {retry_after_seconds} seconds");
                let mut response = json_response(StatusCode::TOO_MANY_REQUESTS, &json!({ "message": message }));
                response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(retry_after_seconds));
                response
            }
            Self::Unavailable { retry_after_seconds } => unavailable_response(
                json_response(StatusCode::SERVICE_UNAVAILABLE, &json!({ "message": UNAVAILABLE_MESSAGE })),
                retry_after_seconds,
            ),
        }
    }
}

impl From<Denied> for LfsError {
    fn from(denied: Denied) -> Self {
        match denied {
            Denied::Challenge => Self::Challenge,
            Denied::RateLimited { retry_after_seconds } => Self::RateLimited { retry_after_seconds },
            Denied::Unavailable { retry_after_seconds } => Self::Unavailable { retry_after_seconds },
            Denied::NotFound => Self::not_found(),
            Denied::Internal => Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
            Denied::Suspended => Self::new(StatusCode::FORBIDDEN, "this account has been suspended"),
            Denied::TokenRequired => Self::new(
                StatusCode::FORBIDDEN,
                "two-factor authentication is enabled for this account, so its password cannot be used with Git; \
                 use a personal access token as the password",
            ),
            Denied::Forbidden => Self::new(StatusCode::FORBIDDEN, "you do not have permission to push to this repository"),
            Denied::Mirror => Self::new(StatusCode::FORBIDDEN, mirrors::PUSH_REFUSED),
        }
    }
}

impl From<ServiceError> for LfsError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound => Self::not_found(),
            ServiceError::Forbidden | ServiceError::Archived => Self::new(StatusCode::FORBIDDEN, error.to_string()),
            ServiceError::Invalid(message) => Self::new(StatusCode::UNPROCESSABLE_ENTITY, message),
            ServiceError::Conflict(_) => Self::new(StatusCode::CONFLICT, error.to_string()),
            ServiceError::RateLimited { retry_after_seconds } => Self::RateLimited { retry_after_seconds },
            ServiceError::Unavailable { retry_after_seconds } => Self::Unavailable { retry_after_seconds },
            other => {
                tracing::error!(error = %other, "LFS request failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unavailable_preserves_lfs_json_for_service_and_git_authentication_failures() {
        for error in [
            LfsError::from(ServiceError::Unavailable { retry_after_seconds: 9 }),
            LfsError::from(Denied::Unavailable { retry_after_seconds: 9 }),
        ] {
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(response.headers()[header::CONTENT_TYPE], super::super::LFS_JSON);
            assert_eq!(response.headers()[header::RETRY_AFTER], "9");
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
            assert!(!response.headers().contains_key("lfs-authenticate"));
            let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap(), json!({ "message": UNAVAILABLE_MESSAGE }));
        }
    }
}
