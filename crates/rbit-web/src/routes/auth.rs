//! Sign-in, sign-up and sign-out.
//!
//! Password sign-in and sign-up are throttled per client address (and sign-in per login
//! too); a throttled attempt re-renders the form with a friendly message, status `429`
//! and `Retry-After`. Accounts with two-factor authentication continue at
//! `/login/two-factor` (see [`super::security`]), carried by a short-lived cookie.

use axum::{
    Form,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_auth::SessionToken;
use rbit_core::User;
use rbit_services::{ServiceError, SignUp, ThrottleScope, two_factor::SignInOutcome};
use serde::Deserialize;

use crate::{
    AppState,
    client_ip::{Client, Https},
    error::{WebError, WebResult},
    extract::Viewer,
    middleware::{self, SESSION_COOKIE},
    themes::Theme,
    urls::safe_redirect_target,
    views::{
        auth::{LoginPage, SignupPage},
        components::FormError,
        external_auth::ProviderButton,
        layout::Page,
    },
};

/// Identifies a sign-in waiting for its second factor. Lives as long as the pending state.
pub(crate) const TWO_FACTOR_COOKIE: &str = "rbit_2fa";
/// Where the second step is served.
pub(crate) const TWO_FACTOR_PATH: &str = "/login/two-factor";

#[derive(Debug, Deserialize)]
pub struct NextQuery {
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    login: String,
    password: String,
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SignupForm {
    username: String,
    email: String,
    password: String,
    /// Only asked for (and only checked) while the instance has no user.
    #[serde(default)]
    setup_token: String,
}

pub async fn login_form(State(state): State<AppState>, viewer: Viewer, page: Page, Query(query): Query<NextQuery>) -> WebResult<Response> {
    let next = safe_redirect_target(query.next.as_deref()).to_owned();
    if viewer.user().is_some() {
        return Ok(Redirect::to(&next).into_response());
    }
    Ok(login_page(&state, page, FormError::default(), String::new(), next).await?.into_response())
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Form(form): Form<LoginForm>,
) -> WebResult<Response> {
    let next = safe_redirect_target(form.next.as_deref()).to_owned();
    let message = match state.services.sign_in(&form.login, form.password, &client.0).await {
        Ok(SignInOutcome::SignedIn(user)) => return start_session(&state, jar, https, &client, &user, &next).await,
        Ok(SignInOutcome::SecondFactorRequired(pending)) => return Ok(second_factor_redirect(jar, https, &pending, &next)),
        Err(ServiceError::InvalidCredentials) => "Incorrect username or password.".to_owned(),
        Err(ServiceError::Forbidden) => "This account has been suspended. Contact an administrator.".to_owned(),
        Err(ServiceError::RateLimited { retry_after_seconds }) => {
            let page = login_page(&state, page, FormError::new(slow_down("sign-in", retry_after_seconds)), form.login, next).await?;
            return Ok(too_many_requests(page, retry_after_seconds));
        }
        Err(other) => return Err(other.into()),
    };
    let page = login_page(&state, page, FormError::new(message), form.login, next).await?;
    Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
}

pub async fn signup_form(State(state): State<AppState>, viewer: Viewer, page: Page) -> WebResult<Response> {
    if viewer.user().is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    Ok(signup_page(&state, page, FormError::default(), String::new(), String::new(), String::new()).await?.into_response())
}

pub async fn signup(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Form(form): Form<SignupForm>,
) -> WebResult<Response> {
    let SignupForm { username, email, password, setup_token } = form;
    let ip = client.ip().map(|ip| ip.to_canonical().to_string()).unwrap_or_default();
    if let Err(ServiceError::RateLimited { retry_after_seconds }) = state.services.throttle.check(ThrottleScope::SignUp, &ip) {
        let error = FormError::new(slow_down("sign-up", retry_after_seconds));
        let page = signup_page(&state, page, error, username, email, setup_token).await?;
        return Ok(too_many_requests(page, retry_after_seconds));
    }
    let input = SignUp { username: username.clone(), email: email.clone(), password };
    match state.services.accounts.sign_up(input, &setup_token).await {
        Ok(user) => start_session(&state, jar, https, &client, &user, "/").await,
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_) | ServiceError::RegistrationClosed)) => {
            let error = FormError::from_message(&error.to_string());
            let page = signup_page(&state, page, error, username, email, setup_token).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> WebResult<Response> {
    if let Some(token) = middleware::session_token(&jar) {
        state.services.accounts.end_session(&token).await?;
    }
    let jar = jar.remove(Cookie::build(SESSION_COOKIE).path("/")).remove(Cookie::build(middleware::LEGACY_SESSION_COOKIE).path("/"));
    Ok((jar, Redirect::to("/")).into_response())
}

pub(crate) async fn login_page(state: &AppState, page: Page, error: FormError, login: String, next: String) -> Result<LoginPage, WebError> {
    let registration_open = state.services.accounts.registration_open().await?;
    // Milestone 3: external sign-in (O).
    let providers = ProviderButton::all(&state.services.external_auth.providers(), &next);
    Ok(LoginPage { page: page.titled(crate::i18n::text("core", "title-sign-in")), error, login, next, registration_open, providers })
}

/// Continues a sign-in whose first step passed at the second step: the pending sign-in
/// rides in a short-lived cookie scoped to that page. Shared with external sign-in.
pub(crate) fn second_factor_redirect(jar: CookieJar, https: Https, pending: &SessionToken, next: &str) -> Response {
    let cookie = Cookie::build((TWO_FACTOR_COOKIE, pending.encode()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(https.0)
        .path(TWO_FACTOR_PATH)
        .max_age(time::Duration::seconds(rbit_services::two_factor::PENDING_SIGN_IN_TTL.as_secs().try_into().unwrap_or(i64::MAX)));
    let target = format!("{TWO_FACTOR_PATH}?next={}", utf8_percent_encode(next, NON_ALPHANUMERIC));
    (jar.add(cookie), Redirect::to(&target)).into_response()
}

async fn signup_page(
    state: &AppState,
    page: Page,
    error: FormError,
    username: String,
    email: String,
    setup_token: String,
) -> Result<SignupPage, WebError> {
    let accounts = &state.services.accounts;
    Ok(SignupPage {
        page: page.titled(crate::i18n::text("core", "title-sign-up")),
        error,
        username,
        email,
        setup_token,
        min_password_length: state.config.security.min_password_length,
        registration_open: accounts.registration_open().await?,
        setup_required: accounts.setup_required().await?,
    })
}

/// Issues the session cookie and redirects with `303 See Other`. The cookie is `Secure`
/// whenever the request arrived over HTTPS, directly or through a trusted proxy. The
/// user's stored theme, if any, comes along as the theme cookie.
pub(crate) async fn start_session(
    state: &AppState,
    jar: CookieJar,
    Https(https): Https,
    client: &Client,
    user: &User,
    next: &str,
) -> WebResult<Response> {
    let token = state.services.accounts.start_session_for(user, &client.0).await?;
    let cookie = Cookie::build((SESSION_COOKIE, token.encode()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(https)
        .path("/")
        .max_age(time::Duration::seconds(state.services.accounts.session_lifetime_seconds()));
    let jar = jar.add(cookie);
    // The user's theme follows them: restore it on this device. A failure here must not
    // block signing in; the device simply keeps its current theme.
    let jar = match state.services.preferences.theme(user).await {
        Ok(stored) => match stored.as_deref().and_then(Theme::parse) {
            Some(theme) => jar.add(theme.cookie(https)),
            None => jar,
        },
        Err(error) => {
            tracing::warn!(%error, "could not load the stored theme");
            jar
        }
    };
    let next = if state.services.accounts.password_change_required(user.id()).await? { "/settings/account" } else { next };
    Ok((jar, Redirect::to(next)).into_response())
}

/// A re-rendered form with `429 Too Many Requests` and `Retry-After`.
pub(crate) fn too_many_requests(page: impl IntoResponse, retry_after_seconds: u64) -> Response {
    let mut response = (StatusCode::TOO_MANY_REQUESTS, page).into_response();
    response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(retry_after_seconds));
    response
}

/// e.g. "Too many sign-in attempts. Try again in 2 minutes."
pub(crate) fn slow_down(what: &str, retry_after_seconds: u64) -> String {
    let wait = match retry_after_seconds {
        0..=59 => format!("{retry_after_seconds} second{}", if retry_after_seconds == 1 { "" } else { "s" }),
        seconds => {
            let minutes = seconds.div_ceil(60);
            format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" })
        }
    };
    format!("Too many {what} attempts. Try again in {wait}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_wait_times() {
        assert_eq!(slow_down("sign-in", 1), "Too many sign-in attempts. Try again in 1 second.");
        assert_eq!(slow_down("sign-in", 45), "Too many sign-in attempts. Try again in 45 seconds.");
        assert_eq!(slow_down("sign-up", 60), "Too many sign-up attempts. Try again in 1 minute.");
        assert_eq!(slow_down("sign-up", 61), "Too many sign-up attempts. Try again in 2 minutes.");
    }

    #[test]
    fn throttled_forms_carry_retry_after() {
        let response = too_many_requests("page", 30);
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "30");
    }
}
