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
    extract::{CurrentSession, UnreadNotifications, Viewer},
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
/// [`CurrentSession`] and [`UnreadNotifications`]) stored in the request extensions.
///
/// One database query per signed-in request reads the user, the password-change
/// restriction and the header's unread indicator together.
pub async fn session(State(state): State<AppState>, jar: CookieJar, mut request: Request, next: Next) -> Response {
    let mut viewer = Viewer::default();
    let mut current = None;
    if let Some(token) = session_token(&jar) {
        match state.services.accounts.resume_session(&token).await {
            Ok(Some(signed_in)) => {
                if signed_in.password_change_required && !allowed_during_password_change(request.method(), request.uri().path()) {
                    return ([(header::CACHE_CONTROL, "no-store")], Redirect::to("/settings/account")).into_response();
                }
                request.extensions_mut().insert(UnreadNotifications(signed_in.has_unread_notifications));
                current = Some((signed_in.user.id(), token));
                viewer = Viewer(Some(Arc::new(signed_in.user)));
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "failed to resume session"),
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

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use rbit_core::{Config, IssueKind, IssueText, IssueTitle, Timestamp, Visibility};
    use rbit_services::{Services, SignUp};
    use rbit_storage::{
        Database, KvStore, NewRepository,
        sqlite::{issues::NewIssue, notifications::NotificationReason},
    };
    use tower::ServiceExt;

    use super::*;

    /// Whether the header of `/` (the dashboard) shows the unread indicator to a user with
    /// one unread notification, before and after it is read.
    async fn unread_indicator(notifications: bool) -> (bool, bool) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.issues.notifications = notifications;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let router = crate::router(AppState::new(config, services.clone()));

        let signup = SignUp { username: "ada".into(), email: "ada@example.com".into(), password: "correct horse battery".into() };
        let ada = services.accounts.create_user(signup, Some(true)).await.unwrap();
        let new = NewRepository {
            owner_id: ada.id(),
            name: "demo".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        };
        let repo = database.repositories().create(new).await.unwrap().id;
        let new = NewIssue {
            repo,
            kind: IssueKind::Issue,
            title: IssueTitle::parse("Crash").unwrap(),
            body: IssueText::default(),
            author: ada.id(),
            at: Timestamp(1),
        };
        let issue = database.issues().create(&new).await.unwrap().id;
        database.notifications().notify(ada.id(), issue, repo, NotificationReason::Mention, None, Timestamp(1)).await.unwrap();
        let cookie = format!("{SESSION_COOKIE}={}", services.accounts.start_session(&ada).await.unwrap().encode());

        let shows_dot = || async {
            let request = Request::builder().uri("/").header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
            let response = router.clone().oneshot(request).await.unwrap();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            String::from_utf8_lossy(&body).contains("class=\"notification-dot\"")
        };
        let before = shows_dot().await;
        database.notifications().mark_issue_read(ada.id(), issue).await.unwrap();
        (before, shows_dot().await)
    }

    #[tokio::test]
    async fn the_session_query_carries_the_unread_indicator() {
        assert_eq!(unread_indicator(true).await, (true, false));
    }

    #[tokio::test]
    async fn the_unread_indicator_stays_off_while_notifications_are_disabled() {
        assert_eq!(unread_indicator(false).await, (false, false));
    }
}
