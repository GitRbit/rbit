//! Account security: `/settings/security` (two-factor auth: passkeys, authenticator app and
//! recovery codes; active sessions) and the sign-in second step.
//!
//! Milestone 1 workstream S owns this file.

mod credential_forms;
mod password_commands;
pub use credential_forms::{AddPasskeyForm, CodeForm, PasswordAndCodeForm, TwoFactorForm};
pub use password_commands::{add_passkey, confirm_setup, disable, regenerate_codes, remove_passkey, two_factor};

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use rbit_auth::SessionToken;
use rbit_core::User;
use rbit_services::{
    ServiceError,
    two_factor::{PasskeyRegistered, SecondFactorProof},
};
use serde::Deserialize;

use super::auth::{TWO_FACTOR_COOKIE, TWO_FACTOR_PATH, login_page, slow_down, start_session, too_many_requests};
use crate::{
    AppState,
    client_ip::{Client, Https},
    error::WebResult,
    extract::{CurrentSession, SignedIn},
    urls::safe_redirect_target,
    views::{
        auth::TwoFactorLoginPage,
        components::FormError,
        layout::Page,
        security::{Overview, RecoveryCodes, SECURITY_URL, SecurityNotice, SecurityPage, SecurityView, Setup},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(TWO_FACTOR_PATH, get(two_factor_form).post(two_factor))
        .route(SECURITY_URL, get(overview))
        .route("/settings/security/two-factor", get(setup).post(confirm_setup))
        .route("/settings/security/two-factor/cancel", post(cancel_setup))
        .route("/settings/security/two-factor/disable", post(disable))
        .route("/settings/security/passkeys", post(add_passkey))
        .route("/settings/security/passkeys/{id}/remove", post(remove_passkey))
        .route("/settings/security/recovery-codes", post(regenerate_codes))
        .route("/settings/security/sessions/revoke-others", post(revoke_other_sessions))
        .route("/settings/security/sessions/{id}/revoke", post(revoke_session))
}

/// Entries of "Recent activity" on the overview.
const RECENT_ACTIVITY: u8 = 10;

// ------------------------------------------------------------------ /settings/security

#[derive(Debug, Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
}

/// A passkey answer when the form carries one, the typed code otherwise.
fn proof<'a>(code: &'a str, passkey: &'a str) -> SecondFactorProof<'a> {
    if passkey.trim().is_empty() { SecondFactorProof::Code(code) } else { SecondFactorProof::Passkey(passkey) }
}

/// Passkey options are left off the page when passkeys can't work on this site
/// ([`ServiceError::Invalid`]: no usable `server.public_url`).
fn optional_options(options: Result<Option<String>, ServiceError>) -> WebResult<Option<String>> {
    match options {
        Ok(options) => Ok(options),
        Err(ServiceError::Invalid(message)) => {
            tracing::debug!(%message, "passkeys unavailable");
            Ok(None)
        }
        Err(other) => Err(other.into()),
    }
}

/// Security pages are personal and may show secrets: never cache them.
pub(super) fn no_store(response: impl IntoResponse) -> Response {
    ([(header::CACHE_CONTROL, "no-store")], response).into_response()
}

pub(super) async fn overview_model(state: &AppState, user: &User, current: &CurrentSession) -> WebResult<Overview> {
    let two_factor = &state.services.two_factor;
    let status = two_factor.status(user).await?;
    let sessions = state.services.accounts.sessions(user, Some(&current.0)).await?;
    let activity = state.services.audit.recent_for(user, RECENT_ACTIVITY).await?;
    let mut overview = Overview::new(&status, &sessions, &activity);
    overview.passkey_create_options = optional_options(two_factor.creation_options(user).await.map(Some))?;
    overview.passkey_confirm_options = optional_options(two_factor.confirmation_options(user).await)?;
    // Milestone 3: external sign-in (O).
    overview.sign_in = Box::new(super::external_auth::sign_in_section(state, user, current).await?);
    Ok(overview)
}

/// `GET /settings/security`
pub async fn overview(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    page: Page,
    Query(query): Query<SavedQuery>,
) -> WebResult<Response> {
    let mut overview = overview_model(&state, &user, &current).await?;
    overview.notice = query.saved.as_deref().and_then(SecurityNotice::parse);
    Ok(no_store(SecurityPage::new(page, SecurityView::Overview(overview))))
}

/// `GET /settings/security/two-factor`: starts (or resumes) authenticator enrolment.
pub async fn setup(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<Response> {
    if state.services.two_factor.has_authenticator(user.id()).await? {
        return Ok(Redirect::to(SECURITY_URL).into_response());
    }
    let enrolment = state.services.two_factor.begin_enrolment(&user, &page.site_name).await?;
    Ok(no_store(SecurityPage::new(page, SecurityView::Setup(Setup::new(&enrolment, FormError::default())))))
}

/// `POST /settings/security/two-factor/cancel`
pub async fn cancel_setup(State(state): State<AppState>, SignedIn(user): SignedIn) -> WebResult<Redirect> {
    state.services.two_factor.cancel_enrolment(&user).await?;
    Ok(Redirect::to(&SecurityNotice::SetupCancelled.url()))
}

/// Which overview form an error belongs to.
#[derive(Debug, Clone, Copy)]
enum SensitiveForm {
    Disable,
    Regenerate,
    RemovePasskey(i64),
    AddPasskey,
}

/// Re-renders the overview with `error` above `form`.
async fn overview_with_error(
    state: &AppState,
    user: &User,
    current: &CurrentSession,
    page: Page,
    form: SensitiveForm,
    error: ServiceError,
) -> WebResult<Response> {
    let (status, message) = match error {
        ServiceError::InvalidCredentials => (StatusCode::UNPROCESSABLE_ENTITY, "Incorrect password, code or passkey.".to_owned()),
        ServiceError::RateLimited { retry_after_seconds } => (StatusCode::TOO_MANY_REQUESTS, slow_down("code", retry_after_seconds)),
        ServiceError::Invalid(message) => (StatusCode::UNPROCESSABLE_ENTITY, FormError::from_message(&message).0.unwrap_or_default()),
        other => return Err(other.into()),
    };
    let mut overview = overview_model(state, user, current).await?;
    match form {
        SensitiveForm::Disable => overview.disable_error = FormError::new(message),
        SensitiveForm::Regenerate => overview.regenerate_error = FormError::new(message),
        SensitiveForm::AddPasskey => overview.add_passkey_error = FormError::new(message),
        SensitiveForm::RemovePasskey(id) => {
            if let Some(row) = overview.passkeys.iter_mut().find(|row| row.id == id) {
                row.error = FormError::new(message);
            }
        }
    }
    Ok((status, no_store(SecurityPage::new(page, SecurityView::Overview(overview)))).into_response())
}

/// `POST /settings/security/sessions/{id}/revoke`
pub async fn revoke_session(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    client: Client,
    Path(id): Path<String>,
) -> WebResult<Redirect> {
    state.services.accounts.revoke_session(&user, &id, &client.0).await?;
    Ok(Redirect::to(&SecurityNotice::SessionRevoked.url()))
}

/// `POST /settings/security/sessions/revoke-others`
pub async fn revoke_other_sessions(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
) -> WebResult<Redirect> {
    state.services.accounts.revoke_other_sessions(&user, &current.0, &client.0).await?;
    Ok(Redirect::to(&SecurityNotice::OtherSessionsRevoked.url()))
}

// ------------------------------------------------------------------ sign-in second step

#[derive(Debug, Deserialize)]
pub struct NextQuery {
    next: Option<String>,
}

fn pending_token(jar: &CookieJar) -> Option<SessionToken> {
    jar.get(TWO_FACTOR_COOKIE).and_then(|cookie| SessionToken::parse(cookie.value()))
}

fn without_pending(jar: CookieJar) -> CookieJar {
    jar.remove(Cookie::build(TWO_FACTOR_COOKIE).path(TWO_FACTOR_PATH))
}

/// The second-step page for `pending`, with a fresh passkey challenge if the account has
/// passkeys.
async fn second_step_page(
    state: &AppState,
    pending: &SessionToken,
    page: Page,
    error: FormError,
    next: String,
) -> WebResult<TwoFactorLoginPage> {
    let two_factor = &state.services.two_factor;
    let authenticator = match two_factor.pending_account(pending) {
        Some(account) => two_factor.has_authenticator(account).await?,
        None => true,
    };
    let passkey_options = optional_options(two_factor.sign_in_options(pending).await)?;
    Ok(TwoFactorLoginPage { page: page.titled("Two-factor authentication"), error, next, passkey_options, authenticator })
}

/// `GET /login/two-factor`: only while a password has just been accepted.
pub async fn two_factor_form(
    State(state): State<AppState>,
    jar: CookieJar,
    page: Page,
    Query(query): Query<NextQuery>,
) -> WebResult<Response> {
    let next = safe_redirect_target(query.next.as_deref()).to_owned();
    let Some(pending) = pending_token(&jar).filter(|token| state.services.two_factor.pending_account(token).is_some()) else {
        return Ok(Redirect::to("/login").into_response());
    };
    Ok(no_store(second_step_page(&state, &pending, page, FormError::default(), next).await?))
}
