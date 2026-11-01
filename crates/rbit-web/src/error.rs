//! Handler errors.
//!
//! A handler returns [`WebError`]; its response carries an [`ErrorInfo`] extension and
//! the [`crate::middleware::error_pages`] layer renders the full, branded error page
//! (with the signed-in user's navigation) around it.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rbit_services::ServiceError;

#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("Operation admission is unavailable.")]
    AdmissionUnavailable,
    #[error("Operation outcome is unconfirmed. Do not retry automatically.")]
    OutcomeUnavailable,
    #[error("page not found")]
    NotFound,
    #[error("you do not have permission to view this page")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
    #[error("too many requests; try again in {retry_after_seconds} seconds")]
    TooManyRequests { retry_after_seconds: u64 },
    #[error("internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Attached to error responses so the error-page middleware knows what to render.
#[derive(Debug, Clone)]
pub struct ErrorInfo {
    pub status: StatusCode,
    pub title: &'static str,
    pub message: String,
}

impl ErrorInfo {
    pub fn localized_title(&self) -> std::borrow::Cow<'static, str> {
        let key = match self.title {
            "Service unavailable" => "error-title-service-unavailable",
            "Page not found" => "error-title-page-not-found",
            "Access denied" => "error-title-access-denied",
            "Bad request" => "error-title-bad-request",
            "Slow down" => "error-title-slow-down",
            "Something went wrong" => "error-title-something-went-wrong",
            _ => return std::borrow::Cow::Borrowed(self.title),
        };
        crate::i18n::text("core", key)
    }
    pub fn localized_message(&self) -> std::borrow::Cow<'_, str> {
        let key = match self.message.as_str() {
            "The page you were looking for doesn't exist, or you don't have access to it." => "error-not-found",
            "Operation admission is unavailable." => "error-admission-unavailable",
            "Operation outcome is unconfirmed. Do not retry automatically." => "error-outcome-unavailable",
            "An unexpected error occurred. It has been logged." => "error-internal",
            _ => return std::borrow::Cow::Borrowed(&self.message),
        };
        crate::i18n::text("core", key)
    }
}

impl WebError {
    pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(error))
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::AdmissionUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::OutcomeUnavailable | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::AdmissionUnavailable => "Service unavailable",
            Self::NotFound => "Page not found",
            Self::Forbidden => "Access denied",
            Self::BadRequest(_) => "Bad request",
            Self::TooManyRequests { .. } => "Slow down",
            Self::OutcomeUnavailable | Self::Internal(_) => "Something went wrong",
        }
    }
}

impl From<ServiceError> for WebError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::AdmissionUnavailable { .. } => Self::AdmissionUnavailable,
            ServiceError::OutcomeUnavailable { .. } => Self::OutcomeUnavailable,
            ServiceError::NotFound => Self::NotFound,
            ServiceError::Forbidden => Self::Forbidden,
            ServiceError::RateLimited { retry_after_seconds } => Self::TooManyRequests { retry_after_seconds },
            ServiceError::Internal(source) => Self::Internal(source),
            other => Self::BadRequest(other.to_string()),
        }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let message = match &self {
            Self::Internal(source) => {
                tracing::error!(error = %source, "request failed");
                "An unexpected error occurred. It has been logged.".to_owned()
            }
            Self::NotFound => "The page you were looking for doesn't exist, or you don't have access to it.".to_owned(),
            other => other.to_string(),
        };
        let retry_after = match &self {
            Self::TooManyRequests { retry_after_seconds } => Some(*retry_after_seconds),
            _ => None,
        };
        let info = ErrorInfo { status: self.status(), title: self.title(), message };
        let mut response = (info.status, info.message.clone()).into_response();
        if let Some(seconds) = retry_after {
            response.headers_mut().insert(axum::http::header::RETRY_AFTER, axum::http::HeaderValue::from(seconds));
        }
        response.extensions_mut().insert(info);
        response
    }
}

pub type WebResult<T> = Result<T, WebError>;

#[cfg(test)]
mod typed_error_tests {
    use super::{ErrorInfo, WebError};
    use axum::{
        http::{StatusCode, header},
        response::IntoResponse,
    };
    use rbit_services::ServiceError;

    #[test]
    fn typed_service_errors_preserve_page_status_without_retry_headers() {
        for (error, status, message) in [
            (
                ServiceError::AdmissionUnavailable { scope: "private context", reason: rbit_storage::AdmissionReason::Stopped },
                StatusCode::SERVICE_UNAVAILABLE,
                "Operation admission is unavailable.",
            ),
            (
                ServiceError::OutcomeUnavailable { scope: "private context" },
                StatusCode::INTERNAL_SERVER_ERROR,
                "Operation outcome is unconfirmed. Do not retry automatically.",
            ),
            (
                ServiceError::internal(rbit_storage::StorageError::Invariant("private context")),
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected error occurred. It has been logged.",
            ),
        ] {
            let response = WebError::from(error).into_response();
            assert_eq!(response.status(), status);
            assert!(!response.headers().contains_key(header::RETRY_AFTER));
            let info = response.extensions().get::<ErrorInfo>().unwrap();
            assert_eq!(info.message, message);
            assert!(!info.message.contains("private context"));
            assert!(!info.message.contains("already taken"));
        }
    }
}
