//! REST API: Notifications: `/notifications`, `/notifications/threads/{id}…`.
//!
//! Milestone 2 workstream I (issues) owns this file and `openapi/notifications.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! The caller's own inbox, shaped like GitHub's threads. Rows of repositories the caller
//! can no longer read are left out (and deleted). Everything is 404 when
//! `[issues] notifications = false`. `read` scope to list, `write` to change.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rbit_core::{IssueKind, TokenScope};
use rbit_services::{notifications::Notification, webhooks::payloads::timestamp};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Delete, Get, Patch, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    pagination::{Paged, Pager},
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    vec![
        route(Get, "/notifications", list),
        route(Put, "/notifications", mark_all_read),
        route(Patch, "/notifications/threads/{id}", mark_read),
        route(Delete, "/notifications/threads/{id}/subscription", unsubscribe),
    ]
}

fn require_enabled(state: &AppState) -> ApiResult<()> {
    if state.services.notifications.enabled() { Ok(()) } else { Err(ApiError::not_found()) }
}

fn parse_id(id: &str) -> ApiResult<i64> {
    id.parse::<i64>().ok().filter(|id| *id > 0).ok_or_else(ApiError::not_found)
}

/// A thread as GitHub shows it (`id` is a string there too).
pub fn thread(notification: &Notification, base_url: &str) -> Value {
    let full_name = notification.full_name();
    let api = format!("{base_url}/api/v1");
    let number = notification.number.get();
    let (kind, subject_url) = match notification.kind {
        IssueKind::Issue => ("Issue", format!("{api}/repos/{full_name}/issues/{number}")),
        IssueKind::Pull => ("PullRequest", format!("{api}/repos/{full_name}/pulls/{number}")),
    };
    json!({
        "id": notification.id.to_string(),
        "unread": notification.unread,
        "reason": notification.reason.as_str(),
        "updated_at": timestamp(notification.updated_at),
        "subject": { "title": notification.title, "url": subject_url, "type": kind, "number": number },
        "repository": {
            "id": notification.repo.0,
            "name": notification.repo_name,
            "full_name": full_name,
            "owner": { "login": notification.owner },
            "html_url": format!("{base_url}/{full_name}"),
            "url": format!("{api}/repos/{full_name}"),
        },
        "url": format!("{api}/notifications/threads/{}", notification.id),
        "subscription_url": format!("{api}/notifications/threads/{}/subscription", notification.id),
    })
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    all: Option<bool>,
}

/// `GET /notifications[?all=true]`: unread threads (all of them with `all=true`), newest first.
pub async fn list(State(state): State<AppState>, caller: Caller, pager: Pager, Query(query): Query<ListQuery>) -> ApiResult<Paged<Value>> {
    require_enabled(&state)?;
    let user = caller.require(TokenScope::Read)?;
    let per_page = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let page = state.services.notifications.inbox_page(user, !query.all.unwrap_or(false), pager.page, per_page).await?;
    let base_url = state.config.server.base_url();
    Ok(pager.paged(page.notifications.iter().map(|n| thread(n, base_url)).collect(), Some(page.total), false))
}

/// `PUT /notifications`: marks everything read (`205 Reset Content`, as GitHub).
pub async fn mark_all_read(State(state): State<AppState>, caller: Caller) -> ApiResult<(StatusCode, Json<Value>)> {
    require_enabled(&state)?;
    let user = caller.require(TokenScope::Write)?;
    state.services.notifications.mark_all_read(user).await?;
    Ok((StatusCode::RESET_CONTENT, Json(json!({ "message": "Notifications marked as read" }))))
}

/// `PATCH /notifications/threads/{id}`: marks one thread read.
pub async fn mark_read(State(state): State<AppState>, caller: Caller, Path(id): Path<String>) -> ApiResult<StatusCode> {
    require_enabled(&state)?;
    let user = caller.require(TokenScope::Write)?;
    let id = parse_id(&id)?;
    state.services.notifications.find(user, id).await?;
    state.services.notifications.mark_read(user, id).await?;
    Ok(StatusCode::RESET_CONTENT)
}

/// `DELETE /notifications/threads/{id}/subscription`: no more notifications about it.
pub async fn unsubscribe(State(state): State<AppState>, caller: Caller, Path(id): Path<String>) -> ApiResult<StatusCode> {
    require_enabled(&state)?;
    let user = caller.require(TokenScope::Write)?;
    state.services.notifications.unsubscribe(user, parse_id(&id)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
