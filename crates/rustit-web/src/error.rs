//! Handler errors.
//!
//! A handler returns [`WebError`]; its response carries an [`ErrorInfo`] extension and
//! the [`crate::middleware::error_pages`] layer renders the full, branded error page
//! (with the signed-in user's navigation) around it.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rustit_services::ServiceError;

#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("page not found")]
    NotFound,
    #[error("you do not have permission to view this page")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
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
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::NotFound => "Page not found",
            Self::Forbidden => "Access denied",
            Self::BadRequest(_) => "Bad request",
            Self::Internal(_) => "Something went wrong",
        }
    }
}

impl From<ServiceError> for WebError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound => Self::NotFound,
            ServiceError::Forbidden => Self::Forbidden,
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
        let info = ErrorInfo { status: self.status(), title: self.title(), message };
        let mut response = (info.status, info.message.clone()).into_response();
        response.extensions_mut().insert(info);
        response
    }
}

pub type WebResult<T> = Result<T, WebError>;
