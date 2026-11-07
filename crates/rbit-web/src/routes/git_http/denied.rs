//! Git authentication refusals keep overload separate from credential challenges.

use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_services::mirrors;

use crate::error::{UNAVAILABLE_MESSAGE, unavailable_response};

/// Why a Git request was refused.
#[derive(Debug)]
pub(crate) enum Denied {
    /// Ask the client for (better) credentials.
    Challenge,
    Forbidden,
    /// Correct credentials of a suspended user.
    Suspended,
    /// The account password of a user with two-factor authentication.
    TokenRequired,
    /// Too many attempts from this client or against this login.
    RateLimited {
        retry_after_seconds: u64,
    },
    /// Capacity is unavailable; do not prompt the client for another credential.
    Unavailable {
        retry_after_seconds: u64,
    },
    /// A push to a pull mirror.
    Mirror,
    NotFound,
    Internal,
}

/// Shown by Git when an account with two-factor authentication offers its password.
pub(super) const TOKEN_REQUIRED: &str = "two-factor authentication is enabled for this account, so its password cannot be used \
                              with Git. Create a personal access token under Settings > Access tokens and use it as the \
                              password.";

impl IntoResponse for Denied {
    fn into_response(self) -> Response {
        match self {
            Self::Challenge => (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"rbit\", charset=\"UTF-8\"")],
                "authentication required",
            )
                .into_response(),
            Self::Forbidden => (StatusCode::FORBIDDEN, "you do not have permission to push to this repository").into_response(),
            Self::Suspended => (StatusCode::FORBIDDEN, "this account has been suspended").into_response(),
            Self::TokenRequired => (StatusCode::FORBIDDEN, TOKEN_REQUIRED).into_response(),
            Self::RateLimited { retry_after_seconds } => (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry_after_seconds.to_string())],
                format!("too many authentication attempts; try again in {retry_after_seconds} seconds"),
            )
                .into_response(),
            Self::Unavailable { retry_after_seconds } => unavailable_response(UNAVAILABLE_MESSAGE, retry_after_seconds),
            Self::Mirror => (StatusCode::FORBIDDEN, mirrors::PUSH_REFUSED).into_response(),
            Self::NotFound => (StatusCode::NOT_FOUND, "repository not found").into_response(),
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unavailable_does_not_ask_git_to_retry_with_another_password() {
        let response = Denied::Unavailable { retry_after_seconds: 3 }.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::RETRY_AFTER], "3");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
        let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(body.as_ref(), UNAVAILABLE_MESSAGE.as_bytes());
        let challenge = Denied::Challenge.into_response();
        assert_eq!(challenge.status(), StatusCode::UNAUTHORIZED);
        assert!(challenge.headers().contains_key(header::WWW_AUTHENTICATE));
    }
}
