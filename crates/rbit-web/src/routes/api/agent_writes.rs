//! Server-side writes for agents (agent plan step 4.4): update a pull request's branch from
//! its base, revert a commit into a pull request, move a branch by compare-and-swap. Each
//! takes the commit the caller saw and answers `409 head_changed` when it moved.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rbit_core::{IssueNumber, TokenScope};
use rbit_services::pulls::MergeFailure;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::Post,
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::ApiJson,
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    let repo = |suffix: &str| format!("/repos/{{owner}}/{{repo}}{suffix}");
    vec![
        route(Post, repo("/pulls/{number}/update-branch"), update_branch),
        route(Post, repo("/agent/revert"), revert),
        route(Post, repo("/agent/branch-update"), branch_update),
    ]
}

#[derive(Debug, Deserialize)]
pub struct RepoPath {
    owner: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
pub struct PullPath {
    owner: String,
    repo: String,
    number: String,
}

/// A write that did not happen, as the API answers it.
fn refused(failure: MergeFailure) -> ApiError {
    match failure {
        MergeFailure::HeadChanged | MergeFailure::BaseChanged => {
            ApiError::new(StatusCode::CONFLICT, "the branch is not at the commit you expected").with_code("head_changed")
        }
        MergeFailure::Blocked(message) => ApiError::new(StatusCode::METHOD_NOT_ALLOWED, message),
        MergeFailure::Service(error) => error.into(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateBranch {
    expected_head: String,
}

/// `POST /repos/{owner}/{repo}/pulls/{number}/update-branch {"expected_head": "<sha>"}`:
/// merges the base tip into the head branch; the new `head`.
pub async fn update_branch(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<PullPath>,
    ApiJson(request): ApiJson<UpdateBranch>,
) -> ApiResult<Json<Value>> {
    let user = caller.require(TokenScope::Write)?;
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let number = IssueNumber::parse(&path.number).map_err(|_| ApiError::not_found())?;
    let ctx = state.services.pulls.open(Some(user), repo, number).await?;
    if request.expected_head.is_empty() {
        return Err(ApiError::invalid("expected_head", "expected_head is required"));
    }
    let head = state.services.pulls.update_branch(user, &ctx, &request.expected_head).await.map_err(refused)?;
    Ok(Json(json!({ "schema": "rbit.update-branch/1", "number": number.get(), "head": head, "updated": head != ctx.pull.head_sha })))
}

#[derive(Debug, Deserialize)]
pub struct Revert {
    commit: String,
    #[serde(default)]
    base: Option<String>,
}

/// `POST /repos/{owner}/{repo}/agent/revert {"commit": "<sha>", "base": "main"}`: a branch
/// `revert-<commit>` from the base with one commit undoing it, and a pull request for it.
pub async fn revert(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    ApiJson(request): ApiJson<Revert>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let user = caller.require(TokenScope::Write)?;
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let base = request.base.filter(|base| !base.is_empty()).unwrap_or_else(|| repo.repo.repo.default_branch.clone());
    let number = state.services.pulls.revert_commit(user, &repo, &request.commit, &base).await.map_err(refused)?;
    Ok((StatusCode::CREATED, Json(json!({ "schema": "rbit.revert/1", "number": number.get(), "base": base }))))
}

#[derive(Debug, Deserialize)]
pub struct BranchUpdate {
    branch: String,
    sha: String,
    expected: String,
}

/// `POST /repos/{owner}/{repo}/agent/branch-update {"branch", "sha", "expected"}`: moves the
/// branch from `expected` to `sha` (full ids), as a push by the caller would.
pub async fn branch_update(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    ApiJson(request): ApiJson<BranchUpdate>,
) -> ApiResult<Json<Value>> {
    let user = caller.require(TokenScope::Write)?;
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    state.services.pulls.set_branch(user, &repo, &request.branch, &request.sha, &request.expected).await.map_err(refused)?;
    Ok(Json(json!({ "schema": "rbit.branch-update/1", "branch": request.branch, "sha": request.sha })))
}
