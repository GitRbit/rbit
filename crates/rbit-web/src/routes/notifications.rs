//! The notification inbox: `/notifications` (Unread / All, mark read, unsubscribe).
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Everything here is 404 when
//! `[issues] notifications = false`; anonymous visitors are sent to sign in. Rows of
//! repositories the user can no longer read are hidden (and deleted) by the service.

use axum::{
    Router,
    extract::{Path, Query, State},
    response::Redirect,
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::SignedIn,
    views::{
        components::Pagination,
        layout::Page,
        notifications::{InboxPage, group},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(inbox))
        .route("/notifications/read-all", post(mark_all_read))
        .route("/notifications/{id}/read", post(mark_read))
        .route("/notifications/{id}/unsubscribe", post(unsubscribe))
}

fn require_enabled(state: &AppState) -> WebResult<()> {
    if state.services.notifications.enabled() { Ok(()) } else { Err(WebError::NotFound) }
}

fn parse_id(id: &str) -> WebResult<i64> {
    id.parse::<i64>().ok().filter(|id| *id > 0).ok_or(WebError::NotFound)
}

#[derive(Debug, Deserialize)]
pub struct InboxQuery {
    all: Option<String>,
    page: Option<u32>,
}

/// `GET /notifications[?all=1&page=n]`
async fn inbox(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Query(query): Query<InboxQuery>,
) -> WebResult<InboxPage> {
    require_enabled(&state)?;
    let showing_all = query.all.is_some();
    let inbox = state.services.notifications.inbox(&user, !showing_all, query.page.unwrap_or(1)).await?;
    let base = if showing_all { "/notifications?all=1&" } else { "/notifications?" };
    let page_url = |n: u32| format!("{base}page={n}");
    let page_count = inbox.page_count();
    let pagination = Pagination {
        page: inbox.page,
        page_count,
        previous_url: (inbox.page > 1).then(|| page_url(inbox.page - 1)),
        next_url: (inbox.page < page_count).then(|| page_url(inbox.page + 1)),
    };
    Ok(InboxPage {
        page: page.titled("Notifications"),
        groups: group(&inbox.notifications),
        showing_all,
        unread_url: "/notifications".to_owned(),
        all_url: "/notifications?all=1".to_owned(),
        mark_all_url: "/notifications/read-all".to_owned(),
        pagination,
        total: inbox.total,
    })
}

/// `POST /notifications/read-all`
async fn mark_all_read(State(state): State<AppState>, SignedIn(user): SignedIn) -> WebResult<Redirect> {
    require_enabled(&state)?;
    state.services.notifications.mark_all_read(&user).await?;
    Ok(Redirect::to("/notifications"))
}

/// `POST /notifications/{id}/read`
async fn mark_read(State(state): State<AppState>, SignedIn(user): SignedIn, Path(id): Path<String>) -> WebResult<Redirect> {
    require_enabled(&state)?;
    state.services.notifications.mark_read(&user, parse_id(&id)?).await?;
    Ok(Redirect::to("/notifications"))
}

/// `POST /notifications/{id}/unsubscribe`: no more notifications about that thread.
async fn unsubscribe(State(state): State<AppState>, SignedIn(user): SignedIn, Path(id): Path<String>) -> WebResult<Redirect> {
    require_enabled(&state)?;
    state.services.notifications.unsubscribe(&user, parse_id(&id)?).await?;
    Ok(Redirect::to("/notifications?all=1"))
}
