//! The forms every issue and pull request page shares: comments, title, description,
//! state and lock, under `/{owner}/{repo}/issues/{n}/…` for both kinds (pull requests
//! share the number sequence).
//!
//! The integrator owns this file (T0 in `docs/design/milestone-2b.md`, frozen for I and
//! P). Merged into the page router, so its routes get sessions, the Origin check,
//! security headers, the per-client rate limit and error pages automatically. Every
//! route answers `303 See Other` to the canonical page (`/issues/{n}` or `/pull/{n}` by
//! kind) with an anchor (see `views::timeline`). Hidden repositories and unknown or
//! malformed numbers are 404; actions the viewer may not take on a thread they can see
//! are 403; invalid text is 400 with the reason.

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header::LOCATION},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{CommentId, IssueNumber, StateReason, User};
use rbit_services::timeline::{IssueContext, StateChange};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::timeline::{BODY_ANCHOR, NEW_COMMENT_ANCHOR, TITLE_ANCHOR, ThreadUrls, comment_anchor},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/issues/suggestions", get(suggestions))
        .route("/{owner}/{repo}/issues/{number}/comments", post(add_comment))
        .route("/{owner}/{repo}/issues/{number}/comments/{id}", post(edit_comment))
        .route("/{owner}/{repo}/issues/{number}/comments/{id}/delete", post(delete_comment))
        .route("/{owner}/{repo}/issues/{number}/title", post(edit_title))
        .route("/{owner}/{repo}/issues/{number}/body", post(edit_body))
        .route("/{owner}/{repo}/issues/{number}/state", post(set_state))
        .route("/{owner}/{repo}/issues/{number}/lock", post(set_lock))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SuggestQuery {
    q: String,
    accounts: bool,
}

async fn suggestions(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<SuggestQuery>,
) -> WebResult<Response> {
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let choices = state.services.references.suggestions(&ctx, &query.q, query.accounts).await?;
    Ok(([(axum::http::header::CACHE_CONTROL, "private, no-store")], Json(choices)).into_response())
}

/// Opens thread `number` of `owner/repo` for `user`; 404 for hidden repositories and
/// unknown or malformed numbers.
async fn open(state: &AppState, user: &User, owner: &str, repo: &str, number: &str) -> WebResult<IssueContext> {
    let number = IssueNumber::parse(number).map_err(|_| WebError::NotFound)?;
    let repo = state.services.repositories.open(Some(user), owner, repo).await?;
    Ok(state.services.timeline.open(Some(user), repo, number).await?)
}

/// `303` to the thread's canonical page, scrolled to `anchor`.
fn back(ctx: &IssueContext, anchor: &str) -> Redirect {
    Redirect::to(&ThreadUrls::for_context(ctx).at(anchor))
}

/// `301 Moved Permanently` to `url`: a thread number opened under the other kind's path
/// (`/issues/{n}` for a pull request, `/pull/{n}` for an issue), as GitHub answers. Only
/// for threads the viewer may see; hidden ones are 404 before this is reached.
pub(super) fn moved_to_kind(url: &str) -> Response {
    (StatusCode::MOVED_PERMANENTLY, [(LOCATION, url.to_owned())]).into_response()
}

fn parse_comment_id(id: &str) -> WebResult<CommentId> {
    id.parse::<i64>().ok().filter(|id| *id > 0).map(CommentId).ok_or(WebError::NotFound)
}

/// `close_completed`, `close_not_planned` or `reopen`; anything else is a 400.
fn parse_state(value: &str) -> WebResult<StateChange> {
    match value {
        "close_completed" => Ok(StateChange::Close(StateReason::Completed)),
        "close_not_planned" => Ok(StateChange::Close(StateReason::NotPlanned)),
        "reopen" => Ok(StateChange::Reopen),
        _ => Err(WebError::BadRequest("unknown state change".to_owned())),
    }
}

#[derive(Debug, Deserialize)]
pub struct CommentForm {
    #[serde(default)]
    body: String,
    /// "Close with comment": a state change applied after the comment.
    state: Option<String>,
}

/// `POST /{owner}/{repo}/issues/{n}/comments`: comments, then applies `state` if given.
/// With `state` and an empty body only the state changes.
async fn add_comment(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<CommentForm>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let change = form.state.as_deref().filter(|value| !value.is_empty()).map(parse_state).transpose()?;
    if change.is_some() && !ctx.permissions.can_set_state {
        // Refuse before commenting, so a refused "close with comment" leaves no comment.
        return Err(WebError::Forbidden);
    }
    let timeline = &state.services.timeline;
    let comment = match (change, form.body.trim().is_empty()) {
        (Some(_), true) => None,
        _ => Some(timeline.comment(&user, &ctx, &form.body).await?),
    };
    if let Some(change) = change {
        timeline.set_state(&user, &ctx, change).await?;
    }
    Ok(match comment {
        Some(comment) => back(&ctx, &comment_anchor(comment.id)),
        None => back(&ctx, NEW_COMMENT_ANCHOR),
    })
}

#[derive(Debug, Deserialize)]
pub struct BodyForm {
    #[serde(default)]
    body: String,
}

/// `POST /{owner}/{repo}/issues/{n}/comments/{id}`: replaces the text of a comment.
async fn edit_comment(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number, id)): Path<(String, String, String, String)>,
    Form(form): Form<BodyForm>,
) -> WebResult<Redirect> {
    let id = parse_comment_id(&id)?;
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let comment = state.services.timeline.edit_comment(&user, &ctx, id, &form.body).await?;
    Ok(back(&ctx, &comment_anchor(comment.id)))
}

/// `POST /{owner}/{repo}/issues/{n}/comments/{id}/delete`
async fn delete_comment(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number, id)): Path<(String, String, String, String)>,
) -> WebResult<Redirect> {
    let id = parse_comment_id(&id)?;
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    state.services.timeline.delete_comment(&user, &ctx, id).await?;
    Ok(back(&ctx, NEW_COMMENT_ANCHOR))
}

#[derive(Debug, Deserialize)]
pub struct TitleForm {
    #[serde(default)]
    title: String,
}

/// `POST /{owner}/{repo}/issues/{n}/title`
async fn edit_title(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<TitleForm>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    state.services.timeline.edit(&user, &ctx, Some(&form.title), None).await?;
    Ok(back(&ctx, TITLE_ANCHOR))
}

/// `POST /{owner}/{repo}/issues/{n}/body`: the description (may be emptied).
async fn edit_body(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<BodyForm>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    state.services.timeline.edit(&user, &ctx, None, Some(&form.body)).await?;
    Ok(back(&ctx, BODY_ANCHOR))
}

#[derive(Debug, Deserialize)]
pub struct StateForm {
    #[serde(default)]
    state: String,
}

/// `POST /{owner}/{repo}/issues/{n}/state`: `close_completed`, `close_not_planned` or
/// `reopen`.
async fn set_state(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<StateForm>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let change = parse_state(&form.state)?;
    state.services.timeline.set_state(&user, &ctx, change).await?;
    Ok(back(&ctx, NEW_COMMENT_ANCHOR))
}

#[derive(Debug, Deserialize)]
pub struct LockForm {
    #[serde(default)]
    locked: String,
}

/// `POST /{owner}/{repo}/issues/{n}/lock`: `locked=true` or `locked=false`.
async fn set_lock(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<LockForm>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let locked = match form.locked.as_str() {
        "true" => true,
        "false" => false,
        _ => return Err(WebError::BadRequest("locked must be true or false".to_owned())),
    };
    state.services.timeline.set_locked(&user, &ctx, locked).await?;
    Ok(back(&ctx, NEW_COMMENT_ANCHOR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_redirects_are_a_true_301() {
        let moved = moved_to_kind("/alice/demo/pull/3");
        assert_eq!(moved.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(moved.headers().get(LOCATION).and_then(|value| value.to_str().ok()), Some("/alice/demo/pull/3"));
    }

    #[test]
    fn state_values_and_comment_ids_are_strict() {
        assert_eq!(parse_state("close_completed").unwrap(), StateChange::Close(StateReason::Completed));
        assert_eq!(parse_state("close_not_planned").unwrap(), StateChange::Close(StateReason::NotPlanned));
        assert_eq!(parse_state("reopen").unwrap(), StateChange::Reopen);
        for bad in ["", "close", "close_merged", "merged", "Reopen"] {
            assert!(matches!(parse_state(bad), Err(WebError::BadRequest(_))), "{bad:?}");
        }
        assert_eq!(parse_comment_id("12").unwrap(), CommentId(12));
        for bad in ["0", "-1", "x", ""] {
            assert!(matches!(parse_comment_id(bad), Err(WebError::NotFound)), "{bad:?}");
        }
    }
}
