//! REST API: Pull requests, reviews, review comments, requested reviewers and merging:
//! `/repos/{owner}/{repo}/pulls…`.
//!
//! Milestone 2 workstream P (pull requests) owns this file and `openapi/pulls.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Reads need read access; every change needs a token with the `write` scope (merging
//! also write access to the base repository). Repositories and pull request numbers the
//! caller may not see are 404; an issue number is 404 here too.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rbit_core::{
    IssueNumber, ReviewId, TokenScope, User,
    issue::MergeStrategy,
    pull::{HeadSpec, ReviewCommentId, ReviewSide},
};
use rbit_git::{ChangeStatus, DiffLimits, FileDiffBody, diff::LineKind};
use rbit_services::{
    RepoContext, ServiceError,
    pulls::{
        LineComment, MergeFailure, MergeInput, NewPullInput, PullContext, PullFilter, PullSort, PullStateFilter, ReviewVerdict,
        json as pull_json,
    },
    timeline::StateChange,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod, ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::{self as api_json, ApiJson},
    pagination::{Paged, Pager},
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    use ApiMethod::{Delete, Get, Patch, Post, Put};
    let path = |suffix: &str| format!("/repos/{{owner}}/{{repo}}/pulls{suffix}");
    vec![
        route(Get, path(""), list),
        route(Post, path(""), create),
        route(Get, path("/{number}"), show),
        route(Patch, path("/{number}"), update),
        route(Get, path("/{number}/commits"), commits),
        route(Get, path("/{number}/files"), files),
        route(Get, path("/{number}/merge"), is_merged),
        route(Put, path("/{number}/merge"), merge),
        route(Get, path("/{number}/reviews"), reviews),
        route(Post, path("/{number}/reviews"), create_review),
        route(Get, path("/{number}/reviews/{id}"), review),
        route(Put, path("/{number}/reviews/{id}/dismissals"), dismiss),
        route(Get, path("/{number}/comments"), comments),
        route(Post, path("/{number}/comments"), create_comment),
        route(Get, path("/{number}/requested_reviewers"), requested_reviewers),
        route(Post, path("/{number}/requested_reviewers"), request_reviewers),
        route(Delete, path("/{number}/requested_reviewers"), remove_reviewers),
    ]
}

fn base_url(state: &AppState) -> &str {
    state.config.server.base_url()
}

/// Opens pull request `number` for the caller; 404 for issue numbers and anything hidden.
async fn open(state: &AppState, caller: &Caller, owner: &str, name: &str, number: &str) -> ApiResult<PullContext> {
    let repo = caller.open_repo(state, owner, name).await?;
    let number = IssueNumber::parse(number).map_err(|_| ApiError::not_found())?;
    Ok(state.services.pulls.open(caller.user(), repo, number).await?)
}

/// The caller for a change: signed in with the `write` scope.
fn writer(caller: &Caller) -> ApiResult<&User> {
    caller.require(TokenScope::Write)
}

// ------------------------------------------------------------------ pull requests

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    state: Option<String>,
    head: Option<String>,
    base: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
}

/// `GET /repos/{owner}/{repo}/pulls?state=open|closed|all&head=&base=&sort=&direction=`
pub async fn list(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &owner, &name).await?;
    let state_filter = match query.state.as_deref().unwrap_or("open") {
        "open" => PullStateFilter::Open,
        "closed" => PullStateFilter::Closed,
        "all" => PullStateFilter::All,
        _ => return Err(ApiError::invalid("state", "state must be open, closed or all")),
    };
    let sort = match query.sort.as_deref().unwrap_or("created") {
        "created" => PullSort::Created,
        "updated" => PullSort::Updated,
        _ => return Err(ApiError::invalid("sort", "sort must be created or updated")),
    };
    let ascending = match query.direction.as_deref().unwrap_or("desc") {
        "asc" => true,
        "desc" => false,
        _ => return Err(ApiError::invalid("direction", "direction must be asc or desc")),
    };
    let mut filter =
        PullFilter { state: state_filter, sort, ascending, base_branch: query.base.filter(|b| !b.is_empty()), ..PullFilter::default() };
    if let Some(head) = query.head.filter(|head| !head.is_empty()) {
        let spec = HeadSpec::parse(&head).ok_or_else(|| ApiError::invalid("head", "head must be branch or owner:branch"))?;
        match state.services.pulls.resolve_head(caller.user(), &ctx, &spec).await {
            Ok(head_repo) => {
                filter.head_repo = Some(head_repo.repo.repo.id);
                filter.head_branch = Some(spec.branch);
            }
            Err(ServiceError::NotFound) => return Ok(pager.paged(Vec::new(), Some(0), false)),
            Err(error) => return Err(error.into()),
        }
    }
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u32::try_from(pager.offset()).unwrap_or(u32::MAX);
    let listed = state.services.pulls.list_page(&ctx, &filter, limit, offset).await?;
    let mut items = Vec::with_capacity(listed.items.len());
    for item in &listed.items {
        items.push(state.services.pulls.json_of(&item.issue, &item.pull, base_url(&state)).await?);
    }
    Ok(pager.paged(items, Some(listed.total), false))
}

#[derive(Debug, Deserialize)]
pub struct NewPull {
    title: Option<String>,
    head: String,
    base: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
}

/// `POST /repos/{owner}/{repo}/pulls`: `201` with the pull request.
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name)): Path<(String, String)>,
    ApiJson(new): ApiJson<NewPull>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let base = caller.open_repo(&state, &owner, &name).await?;
    let user = writer(&caller)?;
    let spec = HeadSpec::parse(&new.head).ok_or_else(|| ApiError::invalid("head", "head must be branch or owner:branch"))?;
    let pulls = &state.services.pulls;
    let head = pulls.resolve_head(Some(user), &base, &spec).await.map_err(|error| ApiError::service(error, "head"))?;
    let input = NewPullInput {
        base_branch: new.base,
        head_branch: spec.branch,
        title: new.title.unwrap_or_default(),
        body: new.body.unwrap_or_default(),
        draft: new.draft,
    };
    let (issue, _) = pulls.create(user, &base, &head, input).await?;
    let ctx = pulls.open(Some(user), reopen(&state, &caller, &base).await?, issue.number).await?;
    Ok((StatusCode::CREATED, Json(pulls.json(&ctx, base_url(&state)).await?)))
}

async fn reopen(state: &AppState, caller: &Caller, repo: &RepoContext) -> ApiResult<RepoContext> {
    caller.open_repo(state, repo.repo.owner.name.as_str(), repo.repo.repo.name.as_str()).await
}

/// `GET /repos/{owner}/{repo}/pulls/{number}`
pub async fn show(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    Ok(Json(state.services.pulls.json(&ctx, base_url(&state)).await?))
}

#[derive(Debug, Deserialize)]
pub struct PullChanges {
    title: Option<String>,
    body: Option<String>,
    state: Option<String>,
    base: Option<String>,
}

/// `PATCH /repos/{owner}/{repo}/pulls/{number}`: title, body, `state` (open/closed), base.
pub async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
    ApiJson(changes): ApiJson<PullChanges>,
) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    let services = &state.services;
    if changes.title.is_some() || changes.body.is_some() {
        services
            .timeline
            .edit(user, &ctx.thread, changes.title.as_deref(), changes.body.as_deref())
            .await
            .map_err(|error| ApiError::service(error, "title"))?;
    }
    if let Some(base) = &changes.base {
        services.pulls.set_base(user, &ctx, base).await.map_err(|error| ApiError::service(error, "base"))?;
    }
    if let Some(wanted) = &changes.state {
        let change = match wanted.as_str() {
            "open" => StateChange::Reopen,
            "closed" => StateChange::Close(rbit_core::StateReason::Completed),
            _ => return Err(ApiError::invalid("state", "state must be open or closed")),
        };
        let fresh = services.timeline.open(Some(user), ctx.thread.repo.clone(), ctx.issue().number).await?;
        services.timeline.set_state(user, &fresh, change).await.map_err(|error| ApiError::service(error, "state"))?;
    }
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    Ok(Json(services.pulls.json(&ctx, base_url(&state)).await?))
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/commits` (at most `[pulls] max_commits`, oldest first).
pub async fn commits(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path((owner, name, number)): Path<(String, String, String)>,
) -> ApiResult<Paged<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let mut commits = state.services.pulls.commits(&ctx).await?;
    commits.reverse();
    let full_name = ctx.repo().repo.full_name();
    let (api, html) = (format!("{}/api/v1/repos/{full_name}", base_url(&state)), format!("{}/{full_name}", base_url(&state)));
    Ok(pager.page_of(commits.iter().map(|commit| api_json::commit(commit, &api, &html)).collect()))
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/files`: GitHub's file objects with `patch`.
pub async fn files(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path((owner, name, number)): Path<(String, String, String)>,
) -> ApiResult<Paged<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let changes = state.services.pulls.changes(&ctx, DiffLimits::default()).await?;
    let files = changes
        .files
        .iter()
        .map(|file| {
            let status = match file.status {
                ChangeStatus::Added => "added",
                ChangeStatus::Deleted => "removed",
                ChangeStatus::Modified => "modified",
                ChangeStatus::Renamed => "renamed",
                ChangeStatus::Copied => "copied",
            };
            let patch = match &file.body {
                FileDiffBody::Text(hunks) => Value::String(
                    hunks
                        .iter()
                        .map(|hunk| {
                            let lines = hunk.lines.iter().map(|line| {
                                let sign = match line.kind {
                                    LineKind::Added => '+',
                                    LineKind::Removed => '-',
                                    LineKind::Context => ' ',
                                };
                                format!("{sign}{}", line.text)
                            });
                            std::iter::once(hunk.header.clone()).chain(lines).collect::<Vec<_>>().join("\n")
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => Value::Null,
            };
            json!({
                "filename": file.path,
                "previous_filename": file.old_path,
                "status": status,
                "additions": file.additions,
                "deletions": file.deletions,
                "changes": file.additions + file.deletions,
                "patch": patch,
            })
        })
        .collect();
    Ok(pager.page_of(files))
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/merge`: `204` when merged, `404` otherwise.
pub async fn is_merged(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    if ctx.pull.is_merged() { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::not_found()) }
}

#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    merge_method: Option<String>,
    commit_title: Option<String>,
    commit_message: Option<String>,
    sha: Option<String>,
    /// The base branch the caller reviewed against (an rbit extension).
    base: Option<String>,
}

/// `PUT /repos/{owner}/{repo}/pulls/{number}/merge`: `200 {sha, merged, message}`, `405`
/// when it cannot be merged, `409` when `sha` is not the head or `base` not the base branch.
pub async fn merge(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
    ApiJson(request): ApiJson<MergeRequest>,
) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    let method = request.merge_method.as_deref().unwrap_or("merge");
    let strategy =
        MergeStrategy::parse(method).ok_or_else(|| ApiError::invalid("merge_method", "merge_method must be merge, squash or rebase"))?;
    let input = MergeInput {
        strategy,
        title: request.commit_title.unwrap_or_default(),
        message: request.commit_message.unwrap_or_default(),
        expected_head: request.sha.filter(|sha| !sha.is_empty()),
        expected_base: request.base.filter(|base| !base.is_empty()),
        delete_head_branch: false,
        bypass: false,
    };
    match state.services.pulls.merge(user, &ctx, input).await {
        Ok(sha) => Ok(Json(json!({ "sha": sha, "merged": true, "message": "Pull Request successfully merged" }))),
        Err(MergeFailure::HeadChanged) => {
            Err(ApiError::new(StatusCode::CONFLICT, "Head branch was modified. Review and try the merge again."))
        }
        Err(MergeFailure::BaseChanged) => {
            Err(ApiError::new(StatusCode::CONFLICT, "Base branch was changed. Review and try the merge again."))
        }
        Err(MergeFailure::Blocked(message)) => Err(ApiError::new(StatusCode::METHOD_NOT_ALLOWED, message)),
        Err(MergeFailure::Service(error)) => Err(error.into()),
    }
}

// ------------------------------------------------------------------ reviews

fn review_id(id: &str) -> ApiResult<ReviewId> {
    id.parse::<i64>().ok().filter(|id| *id > 0).map(ReviewId).ok_or_else(ApiError::not_found)
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/reviews`: submitted reviews (and the caller's pending one).
pub async fn reviews(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path((owner, name, number)): Path<(String, String, String)>,
) -> ApiResult<Paged<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let data = state.services.pulls.reviews(caller.user(), &ctx).await?;
    let base = base_url(&state);
    let number = ctx.issue().number.get();
    let items = data
        .reviews
        .iter()
        .chain(data.pending.iter())
        .map(|entry| pull_json::review(&entry.review, entry.reviewer.as_ref(), &ctx.repo().repo, number, base))
        .collect();
    Ok(pager.page_of(items))
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/reviews/{id}`
pub async fn review(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number, id)): Path<(String, String, String, String)>,
) -> ApiResult<Json<Value>> {
    let id = review_id(&id)?;
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let data = state.services.pulls.reviews(caller.user(), &ctx).await?;
    let entry = data.reviews.iter().chain(data.pending.iter()).find(|entry| entry.review.id == id).ok_or_else(ApiError::not_found)?;
    Ok(Json(pull_json::review(&entry.review, entry.reviewer.as_ref(), &ctx.repo().repo, ctx.issue().number.get(), base_url(&state))))
}

#[derive(Debug, Deserialize)]
pub struct NewReviewComment {
    path: Option<String>,
    line: Option<u32>,
    side: Option<String>,
    body: String,
    in_reply_to: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct NewReview {
    #[serde(default)]
    body: String,
    /// `APPROVE`, `REQUEST_CHANGES` or `COMMENT`; without it the review stays pending.
    event: Option<String>,
    #[serde(default)]
    comments: Vec<NewReviewComment>,
}

fn line_comment(comment: NewReviewComment, start_review: bool) -> ApiResult<LineComment> {
    let in_reply_to = comment.in_reply_to.filter(|id| *id > 0).map(ReviewCommentId);
    let side = match comment.side.as_deref().unwrap_or("RIGHT") {
        "RIGHT" | "right" => ReviewSide::Right,
        "LEFT" | "left" => ReviewSide::Left,
        _ => return Err(ApiError::invalid("side", "side must be LEFT or RIGHT")),
    };
    let (path, line) = match (comment.path, comment.line, in_reply_to) {
        (_, _, Some(_)) => (String::new(), 1),
        (Some(path), Some(line), None) if line > 0 && !path.is_empty() => (path, line),
        _ => return Err(ApiError::invalid("line", "a comment needs a path and a line, or in_reply_to")),
    };
    Ok(LineComment { path, side, line, body: comment.body, in_reply_to, start_review })
}

/// `POST /repos/{owner}/{repo}/pulls/{number}/reviews`: `200` with the review.
pub async fn create_review(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
    ApiJson(new): ApiJson<NewReview>,
) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    let pulls = &state.services.pulls;
    let verdict = match new.event.as_deref() {
        None | Some("" | "PENDING") => None,
        Some(event) => Some(
            ReviewVerdict::parse(event).ok_or_else(|| ApiError::invalid("event", "event must be APPROVE, REQUEST_CHANGES or COMMENT"))?,
        ),
    };
    for comment in new.comments {
        pulls.comment_on_line(user, &ctx, line_comment(comment, true)?).await.map_err(|error| ApiError::service(error, "comments"))?;
    }
    let base = base_url(&state);
    let number = ctx.issue().number.get();
    if let Some(verdict) = verdict {
        let review = pulls.submit_review(user, &ctx, verdict, &new.body).await?;
        return Ok(Json(pull_json::review(&review, Some(&user.account), &ctx.repo().repo, number, base)));
    }
    let data = pulls.reviews(Some(user), &ctx).await?;
    let pending = data.pending.ok_or_else(|| ApiError::invalid("event", "a pending review needs at least one comment"))?;
    Ok(Json(pull_json::review(&pending.review, Some(&user.account), &ctx.repo().repo, number, base)))
}

#[derive(Debug, Deserialize)]
pub struct Dismissal {
    message: String,
}

/// `PUT /repos/{owner}/{repo}/pulls/{number}/reviews/{id}/dismissals`
pub async fn dismiss(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number, id)): Path<(String, String, String, String)>,
    ApiJson(dismissal): ApiJson<Dismissal>,
) -> ApiResult<Json<Value>> {
    let id = review_id(&id)?;
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    let review = state
        .services
        .pulls
        .dismiss_review(user, &ctx, id, &dismissal.message)
        .await
        .map_err(|error| ApiError::service(error, "message"))?;
    let reviewer = match review.reviewer {
        Some(id) if id == user.id() => Some(user.account.clone()),
        Some(_) => state
            .services
            .pulls
            .reviews(Some(user), &ctx)
            .await?
            .reviews
            .into_iter()
            .find(|entry| entry.review.id == id)
            .and_then(|entry| entry.reviewer),
        None => None,
    };
    Ok(Json(pull_json::review(&review, reviewer.as_ref(), &ctx.repo().repo, ctx.issue().number.get(), base_url(&state))))
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/comments`: review comments the caller can see.
pub async fn comments(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path((owner, name, number)): Path<(String, String, String)>,
) -> ApiResult<Paged<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let data = state.services.pulls.reviews(caller.user(), &ctx).await?;
    let base = base_url(&state);
    let number = ctx.issue().number.get();
    let mut items: Vec<_> = data.visible_comments().collect();
    items.sort_by_key(|(comment, _)| (comment.created_at, comment.id));
    Ok(pager.page_of(
        items
            .into_iter()
            .map(|(comment, author)| pull_json::review_comment(comment, author.as_ref(), &ctx.repo().repo, number, base))
            .collect(),
    ))
}

/// `POST /repos/{owner}/{repo}/pulls/{number}/comments`: one comment, posted at once (`201`).
pub async fn create_comment(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
    ApiJson(new): ApiJson<NewReviewComment>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    let comment = state
        .services
        .pulls
        .comment_on_line(user, &ctx, line_comment(new, false)?)
        .await
        .map_err(|error| ApiError::service(error, "line"))?;
    let value = pull_json::review_comment(&comment, Some(&user.account), &ctx.repo().repo, ctx.issue().number.get(), base_url(&state));
    Ok((StatusCode::CREATED, Json(value)))
}

// ------------------------------------------------------------------ review requests

#[derive(Debug, Deserialize)]
pub struct Reviewers {
    #[serde(default)]
    reviewers: Vec<String>,
}

async fn requested_json(state: &AppState, caller: &Caller, ctx: &PullContext) -> ApiResult<Value> {
    let data = state.services.pulls.reviews(caller.user(), ctx).await?;
    let base = base_url(state);
    let users: Vec<Value> = data.requested.iter().map(|account| rbit_services::webhooks::payloads::account(account, base)).collect();
    Ok(json!({ "users": users, "teams": [] }))
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/requested_reviewers`
pub async fn requested_reviewers(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    Ok(Json(requested_json(&state, &caller, &ctx).await?))
}

/// `POST /repos/{owner}/{repo}/pulls/{number}/requested_reviewers`: `201` with the pull request.
pub async fn request_reviewers(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
    ApiJson(body): ApiJson<Reviewers>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    state.services.pulls.request_reviewers(user, &ctx, &body.reviewers).await.map_err(|error| ApiError::service(error, "reviewers"))?;
    Ok((StatusCode::CREATED, Json(state.services.pulls.json(&ctx, base_url(&state)).await?)))
}

/// `DELETE /repos/{owner}/{repo}/pulls/{number}/requested_reviewers`: `200` with the pull request.
pub async fn remove_reviewers(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number)): Path<(String, String, String)>,
    ApiJson(body): ApiJson<Reviewers>,
) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    state.services.pulls.remove_reviewers(user, &ctx, &body.reviewers).await.map_err(|error| ApiError::service(error, "reviewers"))?;
    Ok(Json(state.services.pulls.json(&ctx, base_url(&state)).await?))
}
