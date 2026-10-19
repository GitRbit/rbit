//! Session and password-change request handling.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{Method, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use rbit_auth::SessionToken;

use crate::{
    AppState,
    error::WebError,
    extract::{CurrentSession, Viewer},
};

pub const SESSION_COOKIE: &str = "rbit_session";
/// The session cookie's name from before the rename to rbit, still accepted so nobody is
/// signed out by the upgrade; signing out clears it.
pub const LEGACY_SESSION_COOKIE: &str = "rustit_session";

/// The session token the request carries, under the current cookie name or the legacy one.
pub fn session_token(jar: &CookieJar) -> Option<SessionToken> {
    [SESSION_COOKIE, LEGACY_SESSION_COOKIE].into_iter().find_map(|name| jar.get(name).and_then(|c| SessionToken::parse(c.value())))
}

/// Self-service password changes: the session making one survives it (see
/// [`rbit_services::accounts::PasswordChangeGuard`]); every other session ends.
const PASSWORD_CHANGE_PATH: &str = "/settings/account/password";

/// Resolves the session cookie into a [`Viewer`] (and, for signed-in requests, the
/// [`CurrentSession`]) stored in the request extensions.
pub async fn session(State(state): State<AppState>, jar: CookieJar, mut request: Request, next: Next) -> Response {
    let mut viewer = Viewer::default();
    let mut current = None;
    if let Some(token) = session_token(&jar) {
        match state.services.accounts.resume_session(&token).await {
            Ok(Some(user)) => {
                current = Some((user.id(), token));
                viewer = Viewer(Some(Arc::new(user)));
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "failed to resume session"),
        }
    }
    if let Some(user) = viewer.user() {
        match state.services.accounts.password_change_required(user.id()).await {
            Ok(true) if !allowed_during_password_change(request.method(), request.uri().path()) => {
                return ([(header::CACHE_CONTROL, "no-store")], Redirect::to("/settings/account")).into_response();
            }
            Ok(_) => {}
            Err(error) => return WebError::from(error).into_response(),
        }
    }
    request.extensions_mut().insert(viewer);

    let changes_password = request.method() == Method::POST && request.uri().path() == PASSWORD_CHANGE_PATH;
    let guard = match &current {
        Some((user, token)) if changes_password => Some(state.services.accounts.guard_password_change(*user, token)),
        _ => None,
    };
    if let Some((_, token)) = current {
        request.extensions_mut().insert(CurrentSession(token));
    }

    let response = next.run(request).await;
    if let Some(guard) = guard {
        // Settings forms redirect after success and re-render (422) after a failure.
        guard.finish(response.status().is_redirection()).await;
    }
    response
}

/// Only the password form, its submission, sign-out and presentation assets remain available.
fn allowed_during_password_change(method: &Method, path: &str) -> bool {
    (matches!(*method, Method::GET | Method::HEAD) && matches!(path, "/settings/account" | "/assets/branding.css"))
        || (*method == Method::POST && matches!(path, PASSWORD_CHANGE_PATH | "/logout"))
}
