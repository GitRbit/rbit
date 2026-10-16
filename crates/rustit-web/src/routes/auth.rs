//! Sign-in, sign-up and sign-out.

use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use rustit_auth::SessionToken;
use rustit_core::User;
use rustit_services::{ServiceError, SignUp};
use serde::Deserialize;

use crate::{
    AppState,
    client_ip::Https,
    error::{WebError, WebResult},
    extract::Viewer,
    middleware::SESSION_COOKIE,
    urls::safe_redirect_target,
    views::{
        auth::{LoginPage, SignupPage},
        components::FormError,
        layout::Page,
    },
};

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
}

pub async fn login_form(State(state): State<AppState>, viewer: Viewer, page: Page, Query(query): Query<NextQuery>) -> WebResult<Response> {
    let next = safe_redirect_target(query.next.as_deref()).to_owned();
    if viewer.user().is_some() {
        return Ok(Redirect::to(&next).into_response());
    }
    let registration_open = state.services.accounts.registration_open().await?;
    Ok(LoginPage { page: page.titled("Sign in"), error: FormError::default(), login: String::new(), next, registration_open }
        .into_response())
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    page: Page,
    Form(form): Form<LoginForm>,
) -> WebResult<Response> {
    let next = safe_redirect_target(form.next.as_deref()).to_owned();
    match state.services.accounts.authenticate(&form.login, form.password).await {
        Ok(user) => start_session(&state, jar, https, &user, &next).await,
        Err(error @ (ServiceError::InvalidCredentials | ServiceError::Forbidden)) => {
            let message = match error {
                ServiceError::Forbidden => "This account has been suspended. Contact an administrator.",
                _ => "Incorrect username or password.",
            };
            let registration_open = state.services.accounts.registration_open().await?;
            let page =
                LoginPage { page: page.titled("Sign in"), error: FormError::new(message), login: form.login, next, registration_open };
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

pub async fn signup_form(State(state): State<AppState>, viewer: Viewer, page: Page) -> WebResult<Response> {
    if viewer.user().is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    Ok(signup_page(&state, page, FormError::default(), String::new(), String::new()).await?.into_response())
}

pub async fn signup(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    page: Page,
    Form(form): Form<SignupForm>,
) -> WebResult<Response> {
    let input = SignUp { username: form.username.clone(), email: form.email.clone(), password: form.password };
    match state.services.accounts.sign_up(input).await {
        Ok(user) => start_session(&state, jar, https, &user, "/").await,
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_) | ServiceError::RegistrationClosed)) => {
            let page = signup_page(&state, page, FormError::from_message(&error.to_string()), form.username, form.email).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> WebResult<Response> {
    if let Some(token) = jar.get(SESSION_COOKIE).and_then(|c| SessionToken::parse(c.value())) {
        state.services.accounts.end_session(&token).await?;
    }
    let jar = jar.remove(Cookie::build(SESSION_COOKIE).path("/"));
    Ok((jar, Redirect::to("/")).into_response())
}

async fn signup_page(state: &AppState, page: Page, error: FormError, username: String, email: String) -> Result<SignupPage, WebError> {
    Ok(SignupPage {
        page: page.titled("Sign up"),
        error,
        username,
        email,
        min_password_length: state.config.security.min_password_length,
        registration_open: state.services.accounts.registration_open().await?,
    })
}

/// Issues the session cookie and redirects with `303 See Other`. The cookie is `Secure`
/// whenever the request arrived over HTTPS, directly or through a trusted proxy.
async fn start_session(state: &AppState, jar: CookieJar, Https(https): Https, user: &User, next: &str) -> WebResult<Response> {
    let token = state.services.accounts.start_session(user).await?;
    let cookie = Cookie::build((SESSION_COOKIE, token.encode()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(https)
        .path("/")
        .max_age(time::Duration::seconds(state.services.accounts.session_lifetime_seconds()));
    Ok((jar.add(cookie), Redirect::to(next)).into_response())
}
