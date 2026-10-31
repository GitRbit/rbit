//! Account security: `/settings/security` (two-factor auth: passkeys, authenticator app and
//! recovery codes; active sessions) and the sign-in second step.
//!
//! Milestone 1 workstream S owns this file.

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

#[derive(Debug, Deserialize)]
pub struct CodeForm {
    #[serde(default)]
    code: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordAndCodeForm {
    #[serde(default)]
    password: String,
    #[serde(default)]
    code: String,
    /// The browser's answer to the page's passkey challenge, instead of a code.
    #[serde(default)]
    passkey: String,
}

impl PasswordAndCodeForm {
    fn proof(&self) -> SecondFactorProof<'_> {
        proof(&self.code, &self.passkey)
    }
}

#[derive(Debug, Deserialize)]
pub struct AddPasskeyForm {
    #[serde(default)]
    name: String,
    /// The browser's answer to the creation challenge (filled in by `passkey.js`).
    #[serde(default)]
    credential: String,
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

/// `POST /settings/security/two-factor`: confirms enrolment and, for a first factor, shows
/// the recovery codes once (not a redirect: they are never stored in a readable form).
pub async fn confirm_setup(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    client: Client,
    page: Page,
    Form(form): Form<CodeForm>,
) -> WebResult<Response> {
    let (status, message) = match state.services.confirm_two_factor(&user, &form.code, &client.0).await {
        Ok(Some(codes)) => {
            let view = SecurityView::RecoveryCodes(RecoveryCodes { codes, just_enabled: true });
            return Ok(no_store(SecurityPage::new(page, view)));
        }
        Ok(None) => return Ok(Redirect::to(&SecurityNotice::AuthenticatorAdded.url()).into_response()),
        Err(ServiceError::InvalidCredentials) => {
            (StatusCode::UNPROCESSABLE_ENTITY, "That code didn't match. Check the time on your device and try the next code.".to_owned())
        }
        Err(ServiceError::RateLimited { retry_after_seconds }) => (StatusCode::TOO_MANY_REQUESTS, slow_down("code", retry_after_seconds)),
        Err(ServiceError::Invalid(message)) => (StatusCode::UNPROCESSABLE_ENTITY, FormError::from_message(&message).0.unwrap_or_default()),
        Err(other) => return Err(other.into()),
    };
    let enrolment = state.services.two_factor.begin_enrolment(&user, &page.site_name).await?;
    let page = SecurityPage::new(page, SecurityView::Setup(Setup::new(&enrolment, FormError::new(message))));
    Ok((status, no_store(page)).into_response())
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

/// `POST /settings/security/two-factor/disable`
pub async fn disable(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Form(form): Form<PasswordAndCodeForm>,
) -> WebResult<Response> {
    match state.services.disable_two_factor(&user, form.password.clone(), form.proof(), &client.0).await {
        Ok(()) => Ok(Redirect::to(&SecurityNotice::TwoFactorDisabled.url()).into_response()),
        Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::Disable, error).await,
    }
}

/// `POST /settings/security/recovery-codes`: replaces the codes and shows them once.
pub async fn regenerate_codes(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Form(form): Form<PasswordAndCodeForm>,
) -> WebResult<Response> {
    match state.services.regenerate_recovery_codes(&user, form.password.clone(), form.proof(), &client.0).await {
        Ok(codes) => Ok(no_store(SecurityPage::new(page, SecurityView::RecoveryCodes(RecoveryCodes { codes, just_enabled: false })))),
        Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::Regenerate, error).await,
    }
}

/// `POST /settings/security/passkeys`: registers the passkey the browser just created. The
/// account's first factor shows its new recovery codes once.
pub async fn add_passkey(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Form(form): Form<AddPasskeyForm>,
) -> WebResult<Response> {
    if form.credential.trim().is_empty() {
        let error = ServiceError::Invalid("your browser didn't send a passkey; try again".to_owned());
        return overview_with_error(&state, &user, &current, page, SensitiveForm::AddPasskey, error).await;
    }
    match state.services.add_passkey(&user, &form.name, &form.credential, &client.0).await {
        Ok(PasskeyRegistered::FirstFactor(codes)) => {
            Ok(no_store(SecurityPage::new(page, SecurityView::RecoveryCodes(RecoveryCodes { codes, just_enabled: true }))))
        }
        Ok(PasskeyRegistered::Added) => Ok(Redirect::to(&SecurityNotice::PasskeyAdded.url()).into_response()),
        Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::AddPasskey, error).await,
    }
}

/// `POST /settings/security/passkeys/{id}/remove`
pub async fn remove_passkey(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Path(id): Path<i64>,
    Form(form): Form<PasswordAndCodeForm>,
) -> WebResult<Response> {
    match state.services.remove_passkey(&user, id, form.password.clone(), form.proof(), &client.0).await {
        Ok(()) => Ok(Redirect::to(&SecurityNotice::PasskeyRemoved.url()).into_response()),
        Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::RemovePasskey(id), error).await,
    }
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

#[derive(Debug, Deserialize)]
pub struct TwoFactorForm {
    #[serde(default)]
    code: String,
    /// The browser's answer to the page's passkey challenge, instead of a code.
    #[serde(default)]
    passkey: String,
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
    Ok(TwoFactorLoginPage {
        page: page.titled(crate::i18n::text("core", "title-two-factor-authentication")),
        error,
        next,
        passkey_options,
        authenticator,
    })
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
    let used_passkey = !form.passkey.trim().is_empty();
    match state.services.complete_sign_in(&pending, proof(&form.code, &form.passkey), &client.0).await {
        Ok(user) => start_session(&state, without_pending(jar), https, &client, &user, &next).await,
        Err(ServiceError::InvalidCredentials) => {
            let error = FormError::new(if used_passkey {
                "That passkey didn't work. Use one registered to this account, or enter a code."
            } else {
                "That code didn't work. Check your authenticator app and try again."
            });
            let page = second_step_page(&state, &pending, page, error, next).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, no_store(page)).into_response())
        }
        Err(ServiceError::RateLimited { retry_after_seconds }) => {
            let error = FormError::new(slow_down("code", retry_after_seconds));
            Ok(too_many_requests(second_step_page(&state, &pending, page, error, next).await?, retry_after_seconds))
        }
        Err(ServiceError::NotFound) => {
            let error = FormError::new("Your sign-in expired. Enter your password again.");
            let login =
                login_page(&state, page.titled(crate::i18n::text("core", "title-two-factor-authentication")), error, String::new(), next)
                    .await?;
            Ok((without_pending(jar), login).into_response())
        }
        Err(other) => Err(other.into()),
    }
}
