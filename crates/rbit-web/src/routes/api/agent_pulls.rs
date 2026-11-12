//! Pull requests and commits as an agent reads and lands them (agent plan steps 4.2, 4.3
//! and 4.5). rbit's own endpoints, each answering one compact object with a `schema`.
//!
//! * `POST …/pulls/ensure` publishes or recovers one pull request for a pair of branches;
//!   a `request_id` makes a retry return the first answer.
//! * `GET …/pulls/{n}/readiness` evaluates merging the current head with the merge's own
//!   policy code (the merge box is the same evaluation), and `POST …/pulls/{n}/ready`
//!   marks a draft ready for that exact head.
//! * `GET …/agent/compare`, `…/agent/contains` and `…/agent/commits/{sha}/pulls` answer
//!   ancestry questions without a diff.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rbit_core::{
    IssueNumber, TokenScope,
    pull::{HeadSpec, Requirement, RequirementCheck},
};
use rbit_services::{
    RepoContext, ServiceError,
    pulls::{NewPullInput, PullContext, ensure::Ensured},
};
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
        route(Post, repo("/pulls/ensure"), ensure),
        route(Get, repo("/pulls/{number}/readiness"), readiness),
        route(Post, repo("/pulls/{number}/ready"), ready),
        route(Get, repo("/agent/compare"), compare),
        route(Get, repo("/agent/contains"), contains),
        route(Get, repo("/agent/commits/{sha}/pulls"), commit_pulls),
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

async fn open_pull(state: &AppState, caller: &Caller, path: &PullPath) -> ApiResult<PullContext> {
    let repo = caller.open_repo(state, &path.owner, &path.repo).await?;
    let number = IssueNumber::parse(&path.number).map_err(|_| ApiError::not_found())?;
    Ok(state.services.pulls.open(caller.user(), repo, number).await?)
}

// ------------------------------------------------------------------ pr ensure (4.3)

#[derive(Debug, Deserialize)]
pub struct EnsurePull {
    head: String,
    base: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    /// A new pull request is a draft unless this says otherwise.
    #[serde(default = "yes")]
    draft: bool,
    #[serde(default)]
    request_id: Option<String>,
}

const fn yes() -> bool {
    true
}

/// `POST /repos/{owner}/{repo}/pulls/ensure`: `201` with a new pull request, `200` with the
/// existing one (`outcome`: `created`, `existing` or `replayed`).
pub async fn ensure(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    ApiJson(request): ApiJson<EnsurePull>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let base = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    if request.request_id.as_ref().is_some_and(|id| id.is_empty() || id.len() > 200) {
        return Err(ApiError::invalid("request_id", "request_id must have 1 to 200 characters"));
    }
    let spec = HeadSpec::parse(&request.head).ok_or_else(|| ApiError::invalid("head", "head must be branch or owner:branch"))?;
    let pulls = &state.services.pulls;
    let head = pulls.resolve_head(Some(user), &base, &spec).await.map_err(|error| ApiError::service(error, "head"))?;
    let input = NewPullInput {
        base_branch: request.base,
        head_branch: spec.branch,
        title: request.title.unwrap_or_default(),
        body: request.body.unwrap_or_default(),
        draft: request.draft,
    };
    let ensured = pulls.ensure(user, &base, &head, input, request.request_id.as_deref()).await?;
    let (status, outcome) = match ensured {
        Ensured::Created(_) => (StatusCode::CREATED, "created"),
        Ensured::Existing(_) => (StatusCode::OK, "existing"),
        Ensured::Replayed(_) => (StatusCode::OK, "replayed"),
    };
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let ctx = pulls.open(Some(user), repo, ensured.number()).await?;
    let pull = pulls.json(&ctx, state.config.server.base_url()).await?;
    Ok((status, Json(json!({ "schema": "rbit.pr-ensure/1", "number": ensured.number().get(), "outcome": outcome, "pull": pull }))))
}

// ------------------------------------------------------------------ readiness (4.2)

/// The stable name of a merge requirement.
fn requirement_name(requirement: Requirement) -> &'static str {
    match requirement {
        Requirement::Open => "open",
        Requirement::NotDraft => "not_draft",
        Requirement::HeadBranchExists => "head_branch_exists",
        Requirement::NoConflicts(_) => "no_conflicts",
        Requirement::Approvals { .. } => "approvals",
        Requirement::NoChangesRequested { .. } => "no_changes_requested",
        Requirement::MergerMayPush => "merger_may_push",
        Requirement::Checks(_) => "checks",
        Requirement::RequiredChecks(_) => "required_checks",
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct HeadQuery {
    #[serde(default)]
    head: Option<String>,
}

/// Refuses `expected` when it is not the pull request's head (`409 head_changed`).
fn same_head(ctx: &PullContext, expected: Option<&str>) -> ApiResult<()> {
    match expected {
        Some(expected) if !expected.is_empty() && !ctx.pull.head_sha.starts_with(expected) => Err(ApiError::new(
            StatusCode::CONFLICT,
            format!("the head is {}, not {expected}", ctx.pull.head_sha),
        )
        .with_code("head_changed")),
        _ => Ok(()),
    }
}

/// The readiness of `ctx`'s head, from the merge box.
async fn readiness_of(state: &AppState, caller: &Caller, ctx: &PullContext) -> ApiResult<Value> {
    let merge_box = state.services.pulls.merge_box(caller.user(), ctx).await?;
    let requirements: Vec<Value> = merge_box
        .evaluation
        .checks
        .iter()
        .map(|RequirementCheck { requirement, met }| {
            json!({ "name": requirement_name(*requirement), "met": met, "says": requirement.describe(*met), "protection": requirement.is_protection() })
        })
        .collect();
    let mut checks: Vec<Value> = merge_box
        .required
        .iter()
        .map(|context| {
            let found = merge_box.statuses.iter().find(|status| status.context.as_str() == context);
            let state = found.map_or("expected", |status| match status.state.as_str() {
                "success" => "passed",
                "pending" => "running",
                _ => "failed",
            });
            json!({ "context": context, "required": true, "state": state })
        })
        .collect();
    for status in merge_box.statuses.iter().filter(|status| !merge_box.required.iter().any(|context| context == status.context.as_str())) {
        checks.push(json!({ "context": status.context.as_str(), "required": false, "state": status.state.as_str() }));
    }
    let evaluation = &merge_box.evaluation;
    Ok(json!({
        "schema": "rbit.readiness/1",
        "number": ctx.issue().number.get(),
        "head": ctx.pull.head_sha,
        "base": ctx.pull.base_branch,
        "draft": ctx.pull.is_draft,
        "ready": evaluation.can_merge(),
        "blocked": evaluation.is_blocked(),
        "may_bypass": evaluation.can_merge_with_bypass() && !evaluation.can_merge(),
        "requirements": requirements,
        "checks": checks,
    }))
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/readiness[?head=<sha>]`
pub async fn readiness(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<PullPath>,
    Query(query): Query<HeadQuery>,
) -> ApiResult<Json<Value>> {
    let ctx = open_pull(&state, &caller, &path).await?;
    same_head(&ctx, query.head.as_deref())?;
    Ok(Json(readiness_of(&state, &caller, &ctx).await?))
}

#[derive(Debug, Default, Deserialize)]
pub struct ReadyRequest {
    #[serde(default)]
    head: Option<String>,
}

/// `POST /repos/{owner}/{repo}/pulls/{number}/ready {"head": "<sha>"}`: marks a draft ready
/// for review when its head is `head`; answers its readiness.
pub async fn ready(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<PullPath>,
    ApiJson(request): ApiJson<ReadyRequest>,
) -> ApiResult<Json<Value>> {
    let user = caller.require(TokenScope::Write)?;
    let ctx = open_pull(&state, &caller, &path).await?;
    same_head(&ctx, request.head.as_deref())?;
    if ctx.pull.is_draft {
        state.services.pulls.set_draft(user, &ctx, false).await?;
    }
    let ctx = open_pull(&state, &caller, &path).await?;
    Ok(Json(readiness_of(&state, &caller, &ctx).await?))
}

// ------------------------------------------------------------------ queries (4.5)

#[derive(Debug, Deserialize)]
pub struct CompareQuery {
    base: String,
    head: String,
}

async fn open_repo(state: &AppState, caller: &Caller, path: &RepoPath) -> ApiResult<RepoContext> {
    caller.open_repo(state, &path.owner, &path.repo).await
}

/// `GET /repos/{owner}/{repo}/agent/compare?base=<rev>&head=<rev>`: merge base, ahead and
/// behind, without a diff.
pub async fn compare(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    Query(query): Query<CompareQuery>,
) -> ApiResult<Json<Value>> {
    let repo = open_repo(&state, &caller, &path).await?;
    let found = repo.reader.divergence(query.base, query.head).await.map_err(ServiceError::from)?;
    Ok(Json(json!({
        "schema": "rbit.compare/1",
        "base": found.base,
        "head": found.head,
        "merge_base": found.merge_base,
        "ahead": found.ahead,
        "behind": found.behind,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ContainsQuery {
    commit: String,
    #[serde(default)]
    r#ref: Option<String>,
}

/// `GET /repos/{owner}/{repo}/agent/contains?commit=<rev>[&ref=<rev>]`: whether `ref` (the
/// default branch when not given) contains `commit`.
pub async fn contains(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    Query(query): Query<ContainsQuery>,
) -> ApiResult<Json<Value>> {
    let repo = open_repo(&state, &caller, &path).await?;
    let target = query.r#ref.filter(|target| !target.is_empty()).unwrap_or_else(|| format!("refs/heads/{}", repo.repo.repo.default_branch));
    let commit = repo.reader.commit_id(query.commit).await.map_err(ServiceError::from)?;
    let contained = repo.reader.is_ancestor(commit.clone(), target.clone()).await.map_err(ServiceError::from)?;
    Ok(Json(json!({ "schema": "rbit.contains/1", "commit": commit, "ref": target, "contains": contained })))
}

#[derive(Debug, Deserialize)]
pub struct CommitPath {
    owner: String,
    repo: String,
    sha: String,
}

/// `GET /repos/{owner}/{repo}/agent/commits/{sha}/pulls`: the pull requests the commit is
/// the head, the merge commit or the merged head of, open or not, newest first.
pub async fn commit_pulls(State(state): State<AppState>, caller: Caller, Path(path): Path<CommitPath>) -> ApiResult<Json<Value>> {
    let repo = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let sha = repo.reader.commit_id(path.sha.clone()).await.map_err(ServiceError::from)?;
    let found = state.services.pulls.with_commit(&repo, &sha).await?;
    let items: Vec<Value> = found
        .iter()
        .map(|(issue, pull)| {
            let role = if pull.merge_commit_sha.as_deref() == Some(sha.as_str()) {
                "merge_commit"
            } else if pull.head_sha == sha {
                "head"
            } else {
                "merged_head"
            };
            json!({
                "number": issue.number.get(),
                "title": issue.title.as_str(),
                "state": if pull.merged_at.is_some() { "merged" } else { issue.state.as_str() },
                "role": role,
                "base": pull.base_branch,
                "head_branch": pull.head_branch,
            })
        })
        .collect();
    Ok(Json(json!({ "schema": "rbit.commit-pulls/1", "sha": sha, "pulls": items })))
}
