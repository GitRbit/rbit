//! Email notifications: `/settings/notifications` and the one-click unsubscribe link
//! `/email/unsubscribe/{token}`.
//!
//! Milestone 3 workstream N (email notifications) owns this file.
//!
//! The unsubscribe link needs no session: its token is signed by the server. `GET` only
//! shows a confirmation (mail scanners follow links); `POST` turns the emails off. The
//! `POST` is also what mail clients send for RFC 8058 one-click unsubscribe
//! (`List-Unsubscribe=One-Click`, from another origin), so it is the one path exempt from
//! the cross-site check (see `middleware::csrf_origin_check`); a forged request can only
//! turn off emails of the account the token was issued for. Pages carrying the token are
//! sent with `Cache-Control: no-store` and `Referrer-Policy: no-referrer`.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderName, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_core::config::EmailPreference;
use rbit_services::{
    ServiceError,
    notification_email::{SETTINGS_PATH, UNSUBSCRIBE_PATH},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::WebResult,
    extract::SignedIn,
    views::{
        components::FormError,
        layout::Page,
        notification_email::{NotificationSettingsPage, UnsubscribePage, UnsubscribeState},
    },
};

/// Headers for pages whose URL contains a token.
const TOKEN_URL_HEADERS: [(HeaderName, &str); 2] = [(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer")];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(SETTINGS_PATH, get(settings).post(save_settings))
        .route("/email/unsubscribe/{token}", get(unsubscribe_form).post(unsubscribe))
}

// ------------------------------------------------------------------ settings

#[derive(Debug, Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PreferenceForm {
    #[serde(default)]
    email: String,
}

/// `GET /settings/notifications`
async fn settings(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Query(query): Query<SavedQuery>,
) -> WebResult<NotificationSettingsPage> {
    let settings = state.services.notification_email.settings(&user).await?;
    Ok(NotificationSettingsPage::new(page, &settings).saved(query.saved.is_some()))
}

/// `POST /settings/notifications`
async fn save_settings(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<PreferenceForm>,
) -> WebResult<Response> {
    let email = &state.services.notification_email;
    let Some(preference) = EmailPreference::parse(&form.email) else {
        let settings = email.settings(&user).await?;
        let page = NotificationSettingsPage::new(page, &settings).with_error(FormError::new("Choose which notifications to email."));
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response());
    };
    email.set_preference(&user, preference).await?;
    Ok(Redirect::to(&format!("{SETTINGS_PATH}?saved=1")).into_response())
}

// ------------------------------------------------------------------ one-click unsubscribe

/// `GET /email/unsubscribe/{token}`: asks to confirm.
async fn unsubscribe_form(State(state): State<AppState>, page: Page, Path(token): Path<String>) -> WebResult<Response> {
    let result = state.services.notification_email.unsubscribe_target(&token).await;
    unsubscribe_page(page, &token, result.map(|username| UnsubscribeState::Confirm { username }))
}

/// `POST /email/unsubscribe/{token}`: from the confirmation form or straight from a mail
/// client (RFC 8058, body `List-Unsubscribe=One-Click`, which is not needed).
async fn unsubscribe(State(state): State<AppState>, page: Page, Path(token): Path<String>) -> WebResult<Response> {
    let result = state.services.notification_email.unsubscribe(&token).await;
    unsubscribe_page(page, &token, result.map(|username| UnsubscribeState::Done { username }))
}

fn unsubscribe_page(page: Page, token: &str, result: Result<UnsubscribeState, ServiceError>) -> WebResult<Response> {
    let (status, state) = match result {
        Ok(state) => (StatusCode::OK, state),
        Err(ServiceError::NotFound) => (StatusCode::NOT_FOUND, UnsubscribeState::Invalid),
        Err(other) => return Err(other.into()),
    };
    let page = UnsubscribePage::new(page, state, format!("{UNSUBSCRIBE_PATH}{token}"));
    Ok((status, TOKEN_URL_HEADERS, page).into_response())
}
