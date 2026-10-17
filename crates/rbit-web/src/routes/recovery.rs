//! Password reset and email verification: `/password/forgot`, `/password/reset/{token}`, `/email/verify/{token}`.
//!
//! Milestone 1 workstream E owns this file.
//!
//! Pages that carry a token in their URL are sent with `Cache-Control: no-store` and
//! `Referrer-Policy: no-referrer`, so the secret neither lingers in caches nor leaks to
//! other sites through links on the page.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderName, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_services::ServiceError;
use serde::Deserialize;

use crate::{
    AppState,
    client_ip::ClientIp,
    error::WebResult,
    views::{
        components::FormError,
        layout::Page,
        recovery::{ForgotPasswordPage, LinkProblem, ResetCompletePage, ResetPasswordPage, VerifyEmailPage},
    },
};

const RESET_COMPLETE_URL: &str = "/password/reset/complete";

/// Headers for pages whose URL contains a secret.
const SECRET_URL_HEADERS: [(HeaderName, &str); 2] = [(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer")];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/password/forgot", get(forgot_form).post(forgot))
        .route(RESET_COMPLETE_URL, get(reset_complete))
        .route("/password/reset/{token}", get(reset_form).post(reset))
        .route("/email/verify/{token}", get(verify))
}

// ------------------------------------------------------------------ forgot password

#[derive(Debug, Deserialize)]
pub struct SentQuery {
    sent: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ForgotForm {
    #[serde(default)]
    email: String,
}

/// `GET /password/forgot`
pub async fn forgot_form(State(state): State<AppState>, page: Page, Query(query): Query<SentQuery>) -> ForgotPasswordPage {
    ForgotPasswordPage::new(page, state.services.recovery.email_available()).sent(query.sent.is_some())
}

/// `POST /password/forgot`: always ends on the same "check your email" notice, whether
/// or not an account uses the address.
pub async fn forgot(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    page: Page,
    Form(form): Form<ForgotForm>,
) -> WebResult<Response> {
    let recovery = &state.services.recovery;
    let error = match recovery.request_password_reset(&form.email, &ip.to_string()).await {
        Ok(()) => return Ok(Redirect::to("/password/forgot?sent=1").into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::RateLimited { .. })) => error,
        Err(other) => return Err(other.into()),
    };
    let page =
        ForgotPasswordPage::new(page, recovery.email_available()).with_error(FormError::from_message(&error.to_string()), form.email);
    Ok(form_error_response(&error, page))
}

/// A form re-rendered after `error`: 429 with `Retry-After` when rate limited, 422 otherwise.
pub(crate) fn form_error_response(error: &ServiceError, page: impl IntoResponse) -> Response {
    match error {
        ServiceError::RateLimited { retry_after_seconds } => {
            (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, retry_after_seconds.to_string())], page).into_response()
        }
        _ => (StatusCode::UNPROCESSABLE_ENTITY, page).into_response(),
    }
}

// ------------------------------------------------------------------ reset password

#[derive(Debug, Deserialize)]
pub struct ResetForm {
    #[serde(default)]
    new_password: String,
    #[serde(default)]
    confirm_password: String,
}

/// `GET /password/reset/{token}`: the new-password form, if the link is still good.
pub async fn reset_form(State(state): State<AppState>, page: Page, Path(token): Path<String>) -> WebResult<Response> {
    let response = match state.services.recovery.reset_target(&token).await {
        Ok(username) => {
            (StatusCode::OK, ResetPasswordPage::form(page, &token, username, state.config.security.min_password_length)).into_response()
        }
        Err(error) => problem_page(&state, page, error)?,
    };
    Ok((SECRET_URL_HEADERS, response).into_response())
}

/// `POST /password/reset/{token}`
pub async fn reset(
    State(state): State<AppState>,
    page: Page,
    Path(token): Path<String>,
    Form(form): Form<ResetForm>,
) -> WebResult<Response> {
    let recovery = &state.services.recovery;
    let result = if form.new_password == form.confirm_password {
        recovery.reset_password(&token, form.new_password).await
    } else {
        Err(ServiceError::Invalid("the new passwords do not match".to_owned()))
    };
    let response = match result {
        Ok(()) => Redirect::to(RESET_COMPLETE_URL).into_response(),
        Err(ServiceError::Invalid(message)) => {
            // Re-check the link: it may have been used or expired meanwhile.
            match recovery.reset_target(&token).await {
                Ok(username) => {
                    let page = ResetPasswordPage::form(page, &token, username, state.config.security.min_password_length)
                        .with_error(FormError::from_message(&message));
                    (StatusCode::UNPROCESSABLE_ENTITY, page).into_response()
                }
                Err(error) => problem_page(&state, page, error)?,
            }
        }
        Err(error) => problem_page(&state, page, error)?,
    };
    Ok((SECRET_URL_HEADERS, response).into_response())
}

/// Renders why a reset link can't be used, or passes unexpected errors on.
fn problem_page(state: &AppState, page: Page, error: ServiceError) -> WebResult<Response> {
    let (status, problem) = match error {
        ServiceError::NotFound => (StatusCode::NOT_FOUND, LinkProblem::Invalid),
        ServiceError::Forbidden => (StatusCode::FORBIDDEN, LinkProblem::Suspended),
        other => return Err(other.into()),
    };
    Ok((status, ResetPasswordPage::problem(page, problem, state.services.recovery.email_available())).into_response())
}

/// `GET /password/reset/complete`
pub async fn reset_complete(page: Page) -> ResetCompletePage {
    ResetCompletePage::new(page)
}

// ------------------------------------------------------------------ verification

/// `GET /email/verify/{token}`: confirms the address. Following the link is the whole
/// action, so mail clients that open links in advance can only ever confirm it.
pub async fn verify(State(state): State<AppState>, page: Page, Path(token): Path<String>) -> WebResult<Response> {
    let response = match state.services.recovery.verify_email(&token).await {
        Ok(verified) => (StatusCode::OK, VerifyEmailPage::verified(page, verified.address.to_string())).into_response(),
        Err(ServiceError::NotFound) => (StatusCode::NOT_FOUND, VerifyEmailPage::invalid(page)).into_response(),
        Err(other) => return Err(other.into()),
    };
    Ok((SECRET_URL_HEADERS, response).into_response())
}
