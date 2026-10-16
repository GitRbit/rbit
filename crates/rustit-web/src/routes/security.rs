//! Account security: `/settings/security` (two-factor auth, active sessions) and the sign-in second step.
//!
//! Milestone 1 workstream S owns this file.

use axum::{
    Form, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use rustit_auth::SessionToken;
use rustit_services::ServiceError;
use serde::Deserialize;

use super::auth::{TWO_FACTOR_COOKIE, TWO_FACTOR_PATH, login_page, slow_down, start_session, too_many_requests};
use crate::{
    AppState,
    client_ip::{Client, Https},
    error::WebResult,
    urls::safe_redirect_target,
    views::{auth::TwoFactorLoginPage, components::FormError, layout::Page},
};

pub fn routes() -> Router<AppState> {
    Router::new().route(TWO_FACTOR_PATH, get(two_factor_form).post(two_factor))
}

// ------------------------------------------------------------------ sign-in second step

#[derive(Debug, Deserialize)]
pub struct NextQuery {
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TwoFactorForm {
    #[serde(default)]
    code: String,
    next: Option<String>,
}

fn pending_token(jar: &CookieJar) -> Option<SessionToken> {
    jar.get(TWO_FACTOR_COOKIE).and_then(|cookie| SessionToken::parse(cookie.value()))
}

fn without_pending(jar: CookieJar) -> CookieJar {
    jar.remove(Cookie::build(TWO_FACTOR_COOKIE).path(TWO_FACTOR_PATH))
}

/// `GET /login/two-factor`: only while a password has just been accepted.
pub async fn two_factor_form(State(state): State<AppState>, jar: CookieJar, page: Page, Query(query): Query<NextQuery>) -> Response {
    let next = safe_redirect_target(query.next.as_deref()).to_owned();
    if pending_token(&jar).and_then(|token| state.services.two_factor.pending_account(&token)).is_none() {
        return Redirect::to("/login").into_response();
    }
    TwoFactorLoginPage { page: page.titled("Two-factor authentication"), error: FormError::default(), next }.into_response()
}

/// `POST /login/two-factor`
pub async fn two_factor(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Form(form): Form<TwoFactorForm>,
) -> WebResult<Response> {
    let next = safe_redirect_target(form.next.as_deref()).to_owned();
    let Some(pending) = pending_token(&jar) else {
        return Ok(Redirect::to("/login").into_response());
    };
    let page = page.titled("Two-factor authentication");
    match state.services.complete_sign_in(&pending, &form.code, &client.0).await {
        Ok(user) => start_session(&state, without_pending(jar), https, &client, &user, &next).await,
        Err(ServiceError::InvalidCredentials) => {
            let error = FormError::new("That code didn't work. Check your authenticator app and try again.");
            Ok((StatusCode::UNPROCESSABLE_ENTITY, TwoFactorLoginPage { page, error, next }).into_response())
        }
        Err(ServiceError::RateLimited { retry_after_seconds }) => {
            let error = FormError::new(slow_down("code", retry_after_seconds));
            Ok(too_many_requests(TwoFactorLoginPage { page, error, next }, retry_after_seconds))
        }
        Err(ServiceError::NotFound) => {
            let error = FormError::new("Your sign-in expired. Enter your password again.");
            let login = login_page(&state, page, error, String::new(), next).await?;
            Ok((without_pending(jar), login).into_response())
        }
        Err(other) => Err(other.into()),
    }
}
