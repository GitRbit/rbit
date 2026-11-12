//! Server-side writes for agents (agent plan step 4.4): update a pull request's branch from
//! its base, revert a commit into a pull request, move a branch by compare-and-swap. Each
//! takes the commit the caller saw and answers `409 head_changed` when it moved.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rbit_core::{
    IssueNumber, TokenScope,
    actions::{Artifact, ArtifactId},
    release::ReleaseId,
};
use rbit_services::ActionsService;
use rbit_services::pulls::MergeFailure;
use rbit_services::tokens::Right;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Get, Post},
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
        route(Get, repo("/agent/artifacts"), artifact_of_commit),
        route(Post, repo("/releases/{release_id}/assets/from-artifact"), promote_artifact),
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
    caller.require_right(Right::Push)?;
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
    caller.require_right(Right::Push)?;
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
    caller.require_right(Right::Push)?;
    let user = caller.require(TokenScope::Write)?;
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    state.services.pulls.set_branch(user, &repo, &request.branch, &request.sha, &request.expected).await.map_err(refused)?;
    Ok(Json(json!({ "schema": "rbit.branch-update/1", "branch": request.branch, "sha": request.sha })))
}

// ------------------------------------------------------------------ artifacts (4.6)

fn actions(state: &AppState) -> ApiResult<&ActionsService> {
    state.services.actions.as_ref().ok_or_else(ApiError::not_found)
}

fn artifact_json(owner: &str, repo: &str, artifact: &Artifact) -> Value {
    json!({
        "id": artifact.id.0,
        "name": artifact.name,
        "size_in_bytes": artifact.size,
        "run_id": artifact.run_id.0,
        "archive_download_url": format!("/api/v1/repos/{owner}/{repo}/actions/artifacts/{}/zip", artifact.id.0),
    })
}

#[derive(Debug, Deserialize)]
pub struct ArtifactQuery {
    sha: String,
    name: String,
}

/// `GET /repos/{owner}/{repo}/agent/artifacts?sha=<commit>&name=<name>`: the newest artifact
/// of that name built for that commit.
pub async fn artifact_of_commit(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    Query(query): Query<ArtifactQuery>,
) -> ApiResult<Json<Value>> {
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let artifact = actions(&state)?.artifact_of_commit(&repo, &query.sha, &query.name).await?;
    let mut value = artifact_json(&path.owner, &path.repo, &artifact);
    value["schema"] = json!("rbit.artifact/1");
    Ok(Json(value))
}

#[derive(Debug, Deserialize)]
pub struct ReleasePath {
    owner: String,
    repo: String,
    release_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct Promote {
    artifact_id: i64,
}

/// `POST /repos/{owner}/{repo}/releases/{release_id}/assets/from-artifact {"artifact_id": n}`:
/// attaches every file of a built artifact to the release, as built.
pub async fn promote_artifact(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ReleasePath>,
    ApiJson(request): ApiJson<Promote>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    caller.require_right(Right::Merge)?;
    let user = caller.require(TokenScope::Write)?;
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let (artifact, _dir, files) = actions(&state)?.artifact_files(&repo, ArtifactId(request.artifact_id)).await?;
    let mut assets = Vec::new();
    for (name, file) in files {
        let body =
            tokio::fs::File::open(&file).await.map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "cannot read the artifact"))?;
        let asset =
            state.services.releases.upload_asset(&repo, user, ReleaseId(path.release_id), &name, "application/octet-stream", body).await?;
        assets.push(json!({ "id": asset.id.0, "name": asset.name, "size": asset.size }));
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({ "schema": "rbit.promoted/1", "artifact": artifact_json(&path.owner, &path.repo, &artifact), "assets": assets })),
    ))
}
