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

/// One public message for exhausted and closed password work, without account details.
pub(crate) const UNAVAILABLE_MESSAGE: &str = "The service is temporarily unavailable. Try again shortly.";

#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("page not found")]
    NotFound,
    #[error("you do not have permission to view this page")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
    #[error("too many requests; try again in {retry_after_seconds} seconds")]
    TooManyRequests { retry_after_seconds: u64 },
    #[error("service temporarily unavailable")]
    Unavailable { retry_after_seconds: u64 },
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

impl WebError {
    pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(error))
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::NotFound => "Page not found",
            Self::Forbidden => "Access denied",
            Self::BadRequest(_) => "Bad request",
            Self::TooManyRequests { .. } => "Slow down",
            Self::Unavailable { .. } => "Temporarily unavailable",
            Self::Internal(_) => "Something went wrong",
        }
    }
}

impl From<ServiceError> for WebError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound => Self::NotFound,
            ServiceError::Forbidden => Self::Forbidden,
            ServiceError::RateLimited { retry_after_seconds } => Self::TooManyRequests { retry_after_seconds },
            ServiceError::Unavailable { retry_after_seconds } => Self::Unavailable { retry_after_seconds },
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
            Self::Unavailable { .. } => UNAVAILABLE_MESSAGE.to_owned(),
            other => other.to_string(),
        };
        let retry_after = match &self {
            Self::TooManyRequests { retry_after_seconds } => Some(*retry_after_seconds),
            _ => None,
        };
        let info = ErrorInfo { status: self.status(), title: self.title(), message };
        let mut response = (info.status, info.message.clone()).into_response();
        if let Self::Unavailable { retry_after_seconds } = self {
            response = unavailable_response(response, retry_after_seconds);
        }
        if let Some(seconds) = retry_after {
            response.headers_mut().insert(axum::http::header::RETRY_AFTER, axum::http::HeaderValue::from(seconds));
        }
        response.extensions_mut().insert(info);
        response
    }
}

pub type WebResult<T> = Result<T, WebError>;

/// Keeps protocol-specific bodies while applying the common overload contract.
pub(crate) fn unavailable_response(body: impl IntoResponse, retry_after_seconds: u64) -> Response {
    let mut response = (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
    response.headers_mut().insert(axum::http::header::RETRY_AFTER, axum::http::HeaderValue::from(retry_after_seconds));
    response.headers_mut().insert(axum::http::header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-store"));
    response.headers_mut().remove(axum::http::header::WWW_AUTHENTICATE);
    response.headers_mut().remove("lfs-authenticate");
    response
}

#[cfg(test)]
mod tests;
