//! External sign-in: `/login/{provider}` (start), `/login/{provider}/callback` (the
//! provider sends the browser back here), `/login/{provider}/register` (choose a
//! username after a first sign-in), and linking from Settings → Security
//! (`/settings/security/linked/{provider}?token=…`, `…/unlink`).
//!
//! Milestone 3 workstream O (external sign-in) owns this file. The rules live in
//! [`rbit_services::external_auth`]; this layer moves cookies:
//!
//! * the flow cookie (`rbit_oauth`) carries the sealed flow from the start to the
//!   callback: `HttpOnly`, `SameSite=Lax` (the provider's redirect back is a top-level
//!   cross-site `GET`, which `Lax` still sends), `Path=/login`, ten minutes, removed by the
//!   callback whatever the outcome;
//! * the registration cookie (`rbit_oauth_signup`) names the pending "choose a username"
//!   step (`SameSite=Lax`, `Path=/login`, 15 minutes).
//!
//! Starting is a plain link, not a form: browsers apply CSP `form-action` to the
//! redirect a form post leads to, which would block the provider's origin. Links are
//! safe to follow from anywhere for sign-in; starting a *link* needs a token bound to the
//! session, so another site cannot link an identity to a signed-in user's account. The
//! callback is a `GET` protected by the `state` check, not by the Origin check.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use rbit_auth::SessionToken;
use rbit_core::User;
use rbit_services::{
    ServiceError,
    external_auth::{CallbackParams, ExternalSignIn, FLOW_TTL_SECONDS, REGISTRATION_TTL},
};
use serde::Deserialize;

use super::{
    auth::{login_page, second_factor_redirect, slow_down, start_session, too_many_requests},
    security::{no_store, overview_model},
};
use crate::{
    AppState,
    client_ip::{Client, Https},
    error::WebResult,
    extract::{CurrentSession, SignedIn, Viewer},
    urls::safe_redirect_target,
    views::{
        components::FormError,
        external_auth::{RefusedPage, RegisterPage, SignInSection, register_url},
        layout::Page,
        security::{SECURITY_URL, SecurityNotice, SecurityPage, SecurityView},
    },
};

/// Carries the sealed flow from the start to the callback.
const FLOW_COOKIE: &str = "rbit_oauth";
/// Names the pending "choose a username" step.
const REGISTER_COOKIE: &str = "rbit_oauth_signup";
/// Both cookies live under `/login/…`.
const COOKIE_PATH: &str = "/login";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login/{provider}", get(start))
        .route("/login/{provider}/callback", get(callback))
        .route("/login/{provider}/register", get(register_form).post(register))
        .route("/settings/security/linked/{provider}", get(start_link))
        .route("/settings/security/linked/{provider}/unlink", post(unlink))
}

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinkQuery {
    #[serde(default)]
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    #[serde(default)]
    username: String,
    #[serde(default)]
    email: String,
}

fn cookie(name: &'static str, value: String, https: Https, max_age_seconds: i64) -> Cookie<'static> {
    Cookie::build((name, value))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(https.0)
        .path(COOKIE_PATH)
        .max_age(time::Duration::seconds(max_age_seconds))
        .build()
}

fn without(jar: CookieJar, name: &'static str) -> CookieJar {
    jar.remove(Cookie::build(name).path(COOKIE_PATH))
}

/// `303` to the provider, with the flow cookie and no caching.
fn to_provider(jar: CookieJar, https: Https, url: &str, sealed: String) -> Response {
    let jar = jar.add(cookie(FLOW_COOKIE, sealed, https, FLOW_TTL_SECONDS));
    let mut response = (jar, Redirect::to(url)).into_response();
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The refusal page with `status` (and the flow cookie gone).
fn refused(jar: CookieJar, page: Page, status: StatusCode, message: &str, linking: bool) -> Response {
    (status, without(jar, FLOW_COOKIE), no_store(RefusedPage::new(page, message, linking))).into_response()
}

/// `GET /login/{provider}`: starts signing in.
pub async fn start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    viewer: Viewer,
    jar: CookieJar,
    https: Https,
    page: Page,
    Query(query): Query<StartQuery>,
) -> WebResult<Response> {
    let next = safe_redirect_target(query.next.as_deref()).to_owned();
    if viewer.user().is_some() {
        return Ok(Redirect::to(&next).into_response());
    }
    match state.services.external_auth.begin(&provider, &next, None).await {
        Ok((url, sealed)) => Ok(to_provider(jar, https, &url, sealed)),
        Err(ServiceError::Invalid(message)) => {
            let page = login_page(&state, page, FormError::from_message(&message), String::new(), next).await?;
            Ok((StatusCode::SERVICE_UNAVAILABLE, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `GET /settings/security/linked/{provider}?token=…`: starts linking `provider` to the
/// signed-in account; the token must belong to this session.
#[expect(clippy::too_many_arguments, reason = "the session, its token and the page to refuse on")]
pub async fn start_link(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    jar: CookieJar,
    https: Https,
    page: Page,
    Query(query): Query<LinkQuery>,
) -> WebResult<Response> {
    let external = &state.services.external_auth;
    if !external.check_link_token(&current.0, &query.token).await? {
        // A stale page or another site: start again from settings.
        return Ok(Redirect::to(SECURITY_URL).into_response());
    }
    match external.begin(&provider, SECURITY_URL, Some(user.id())).await {
        Ok((url, sealed)) => Ok(to_provider(jar, https, &url, sealed)),
        Err(ServiceError::Invalid(message)) => Ok(refused(jar, page, StatusCode::SERVICE_UNAVAILABLE, &message, true)),
        Err(other) => Err(other.into()),
    }
}

/// `GET /login/{provider}/callback`
#[expect(clippy::too_many_arguments, reason = "the flow cookie, the session and everything a refusal renders")]
pub async fn callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    viewer: Viewer,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Query(query): Query<CallbackQuery>,
) -> WebResult<Response> {
    let sealed = jar.get(FLOW_COOKIE).map(|cookie| cookie.value().to_owned());
    let params = CallbackParams {
        code: query.code.as_deref(),
        state: query.state.as_deref(),
        error: query.error.as_deref(),
        error_description: query.error_description.as_deref(),
    };
    let verified = match state.services.external_auth.callback(&provider, sealed.as_deref(), params, &client.0).await {
        Ok(verified) => verified,
        Err(ServiceError::Invalid(message)) => return Ok(refused(jar, page, StatusCode::BAD_REQUEST, &message, false)),
        Err(ServiceError::RateLimited { retry_after_seconds }) => {
            let page = RefusedPage::new(page, &slow_down("sign-in", retry_after_seconds), false);
            return Ok(too_many_requests((without(jar, FLOW_COOKIE), no_store(page)), retry_after_seconds));
        }
        Err(other) => return Err(other.into()),
    };
    let linking = verified.link_account.is_some();
    // A link finishes only in the session that started it.
    if let Some(account) = verified.link_account
        && viewer.user().map(User::id) != Some(account)
    {
        let message = "you were signed out while linking; sign in and link the account again from your security settings";
        return Ok(refused(jar, page, StatusCode::BAD_REQUEST, message, true));
    }
    let next = safe_redirect_target(Some(&verified.next)).to_owned();
    let jar = without(jar, FLOW_COOKIE);
    match state.services.complete_external_sign_in(verified, &client.0).await {
        Ok(ExternalSignIn::SignedIn(user)) => start_session(&state, jar, https, &client, &user, &next).await,
        Ok(ExternalSignIn::SecondFactorRequired(pending)) => Ok(second_factor_redirect(jar, https, &pending, &next)),
        Ok(ExternalSignIn::Linked) => Ok((jar, Redirect::to(&SecurityNotice::IdentityLinked.url())).into_response()),
        Ok(ExternalSignIn::Register(token)) => {
            let max_age = i64::try_from(REGISTRATION_TTL.as_secs()).unwrap_or(i64::MAX);
            let jar = jar.add(cookie(REGISTER_COOKIE, token.encode(), https, max_age));
            Ok((jar, Redirect::to(&register_url(&provider))).into_response())
        }
        Err(ServiceError::Invalid(message)) => Ok(refused(jar, page, StatusCode::FORBIDDEN, &message, linking)),
        Err(ServiceError::Forbidden) => {
            Ok(refused(jar, page, StatusCode::FORBIDDEN, "this account has been suspended. Contact an administrator", linking))
        }
        Err(other) => Err(other.into()),
    }
}

fn registration_token(jar: &CookieJar) -> Option<SessionToken> {
    jar.get(REGISTER_COOKIE).and_then(|cookie| SessionToken::parse(cookie.value()))
}

/// `GET /login/{provider}/register`
pub async fn register_form(State(state): State<AppState>, Path(provider): Path<String>, jar: CookieJar, page: Page) -> WebResult<Response> {
    let draft = registration_token(&jar).and_then(|token| state.services.external_auth.registration_draft(&provider, &token));
    let Some(draft) = draft else {
        return Ok(Redirect::to("/login").into_response());
    };
    Ok(no_store(RegisterPage::new(page, &draft, FormError::default())))
}

/// `POST /login/{provider}/register`
pub async fn register(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Form(form): Form<RegisterForm>,
) -> WebResult<Response> {
    let external = &state.services.external_auth;
    let Some(token) = registration_token(&jar) else {
        return Ok(Redirect::to("/login").into_response());
    };
    let Some(draft) = external.registration_draft(&provider, &token) else {
        let error = FormError::new("Your sign-in expired. Sign in again.");
        let login = login_page(&state, page, error, String::new(), "/".to_owned()).await?;
        return Ok((without(jar, REGISTER_COOKIE), login).into_response());
    };
    let rerender = |error: FormError| {
        let mut page = RegisterPage::new(page.clone(), &draft, error);
        page.username.clone_from(&form.username);
        page.email.clone_from(&form.email);
        page
    };
    match state.services.register_external(&provider, &token, &form.username, &form.email, &client.0).await {
        Ok((user, next)) => {
            let next = safe_redirect_target(Some(&next)).to_owned();
            start_session(&state, without(jar, REGISTER_COOKIE), https, &client, &user, &next).await
        }
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_) | ServiceError::RegistrationClosed)) => {
            Ok((StatusCode::UNPROCESSABLE_ENTITY, no_store(rerender(FormError::from_message(&error.to_string())))).into_response())
        }
        Err(ServiceError::RateLimited { retry_after_seconds }) => {
            Ok(too_many_requests(no_store(rerender(FormError::new(slow_down("sign-up", retry_after_seconds)))), retry_after_seconds))
        }
        Err(ServiceError::NotFound) => Ok(Redirect::to("/login").into_response()),
        Err(other) => Err(other.into()),
    }
}

/// `POST /settings/security/linked/{provider}/unlink`
pub async fn unlink(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
) -> WebResult<Response> {
    match state.services.external_auth.unlink(&user, &provider, &client.0).await {
        Ok(()) => Ok(Redirect::to(&SecurityNotice::IdentityUnlinked.url()).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let mut overview = overview_model(&state, &user, &current).await?;
            overview.sign_in.error = FormError::from_message(&message);
            let page = SecurityPage::new(page, SecurityView::Overview(overview));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, no_store(page)).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// The "Sign-in methods" section of the security overview.
pub(super) async fn sign_in_section(state: &AppState, user: &User, current: &CurrentSession) -> WebResult<SignInSection> {
    let external = &state.services.external_auth;
    let methods = external.methods(user).await?;
    Ok(SignInSection::new(&methods, &external.link_token(&current.0).await?))
}
