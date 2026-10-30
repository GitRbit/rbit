//! Deployment environments in the REST API, in GitHub's shapes (Actions: environments):
//!
//! * `GET /repos/{owner}/{repo}/environments`, `GET …/environments/{environment_name}`;
//! * `PUT …/environments/{environment_name}` creates or changes one: `reviewers` (`[{type:
//!   "User", id}]`), `prevent_self_review`, and `deployment_branch_policy`
//!   (`{protected_branches: true}` for protected branches; rbit takes the custom patterns as
//!   `{custom_branch_policies: true, patterns: [...]}` in the same body, where GitHub has a
//!   separate endpoint); `DELETE` deletes it. Admin scope, administrators only.
//! * `GET /repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments`, and `POST` to it
//!   with `environment_ids`, `state` (`approved` or `rejected`) and `comment`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{
    TokenScope,
    actions::{BranchPolicy, Environment, EnvironmentId, ReviewDecision, RunId},
};
use rbit_services::{
    RepoContext,
    actions::environments::{EnvironmentInput, PendingDeployment},
    webhooks::payloads::{account, timestamp},
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use super::{
    ApiMethod::{Delete, Get, Post, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::ApiJson,
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`] when Actions is enabled.
pub(super) fn routes() -> Vec<ApiRoute> {
    let environments = |suffix: &str| format!("/repos/{{owner}}/{{repo}}/environments{suffix}");
    vec![
        route(Get, environments(""), list),
        route(Get, environments("/{environment_name}"), show),
        route(Put, environments("/{environment_name}"), put),
        route(Delete, environments("/{environment_name}"), delete),
        route(Get, "/repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments".to_owned(), pending),
        route(Post, "/repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments".to_owned(), review),
    ]
}

#[derive(Debug, Deserialize)]
pub struct EnvironmentPath {
    owner: String,
    repo: String,
    environment_name: Option<String>,
    run_id: Option<String>,
}

impl EnvironmentPath {
    fn name(&self) -> ApiResult<&str> {
        self.environment_name.as_deref().ok_or_else(ApiError::not_found)
    }

    fn run(&self) -> ApiResult<RunId> {
        self.run_id.as_deref().and_then(|id| id.parse().ok()).filter(|id| *id > 0).map(RunId).ok_or_else(ApiError::not_found)
    }
}

fn actions(state: &AppState) -> ApiResult<rbit_services::ActionsService> {
    state.services.actions.snapshot().ok_or_else(ApiError::not_found)
}

/// An environment's API and settings URLs.
fn urls(base: &str, ctx: &RepoContext, name: &str) -> (String, String) {
    let (full_name, name) = (ctx.repo.full_name(), utf8_percent_encode(name, NON_ALPHANUMERIC));
    (format!("{base}/api/v1/repos/{full_name}/environments/{name}"), format!("{base}/{full_name}/settings/actions/environments/{name}"))
}

/// An environment as GitHub returns it.
async fn value(state: &AppState, ctx: &RepoContext, environment: &Environment) -> ApiResult<Value> {
    let base = state.config.server.base_url();
    let reviewers: Vec<Value> = actions(state)?
        .environment_reviewers(environment)
        .await?
        .iter()
        .map(|reviewer| json!({ "type": "User", "reviewer": account(reviewer, base) }))
        .collect();
    let mut rules = Vec::new();
    if !reviewers.is_empty() {
        rules.push(json!({ "id": environment.id.0, "type": "required_reviewers", "prevent_self_review": environment.prevent_self_review, "reviewers": reviewers }));
    }
    if environment.wait_timer > 0 {
        rules.push(json!({ "id": environment.id.0, "type": "wait_timer", "wait_timer": environment.wait_timer }));
    }
    let policy = match &environment.branch_policy {
        BranchPolicy::All => Value::Null,
        BranchPolicy::Protected => json!({ "protected_branches": true, "custom_branch_policies": false }),
        BranchPolicy::Selected(patterns) => json!({ "protected_branches": false, "custom_branch_policies": true, "patterns": patterns }),
    };
    if !policy.is_null() {
        rules.push(json!({ "id": environment.id.0, "type": "branch_policy" }));
    }
    let (url, html_url) = urls(base, ctx, &environment.name);
    Ok(json!({
        "id": environment.id.0,
        "name": environment.name,
        "url": url,
        "html_url": html_url,
        "created_at": timestamp(environment.created_at),
        "updated_at": timestamp(environment.updated_at),
        "protection_rules": rules,
        "deployment_branch_policy": policy,
    }))
}

/// `GET /repos/{owner}/{repo}/environments`
pub async fn list(State(state): State<AppState>, caller: Caller, Path(path): Path<EnvironmentPath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let environments = actions(&state)?.environments(&ctx).await?;
    let mut values = Vec::new();
    for environment in &environments {
        values.push(value(&state, &ctx, environment).await?);
    }
    Ok(Json(json!({ "total_count": values.len(), "environments": values })))
}

/// `GET /repos/{owner}/{repo}/environments/{environment_name}`
pub async fn show(State(state): State<AppState>, caller: Caller, Path(path): Path<EnvironmentPath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let environment = actions(&state)?.environment(&ctx, path.name()?).await?;
    Ok(Json(value(&state, &ctx, &environment).await?))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Reviewer {
    #[serde(rename = "type")]
    kind: String,
    id: i64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BranchPolicyBody {
    protected_branches: bool,
    custom_branch_policies: bool,
    patterns: Vec<String>,
}

/// `Some` for a field that is present, `Some(None)` when it is `null`.
#[expect(clippy::option_option, reason = "absent, `null` and a value mean different things to GitHub's PUT")]
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
#[expect(clippy::option_option, reason = "absent, `null` and a value mean different things to GitHub's PUT")]
pub struct EnvironmentBody {
    #[serde(deserialize_with = "present")]
    reviewers: Option<Option<Vec<Reviewer>>>,
    prevent_self_review: Option<bool>,
    wait_timer: Option<u32>,
    #[serde(deserialize_with = "present")]
    deployment_branch_policy: Option<Option<BranchPolicyBody>>,
}

/// `PUT /repos/{owner}/{repo}/environments/{environment_name}`: creates or changes it;
/// absent fields keep their value, and `null` clears `reviewers` or
/// `deployment_branch_policy`, as on GitHub.
pub async fn put(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<EnvironmentPath>,
    ApiJson(body): ApiJson<EnvironmentBody>,
) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Admin)?;
    if !ctx.access.can_admin() {
        return Err(ApiError::forbidden("You must be an administrator of this repository"));
    }
    let actions = actions(&state)?;
    let current = actions.environment(&ctx, path.name()?).await.ok();
    let reviewers = match body.reviewers {
        Some(None) => Vec::new(),
        Some(Some(reviewers)) => {
            if reviewers.iter().any(|reviewer| !reviewer.kind.eq_ignore_ascii_case("user")) {
                return Err(ApiError::invalid("reviewers", "rbit's reviewers are users"));
            }
            let ids: Vec<_> = reviewers.iter().map(|reviewer| rbit_core::AccountId(reviewer.id)).collect();
            actions.usernames(&ids).await.map_err(|error| ApiError::service(error, "reviewers"))?
        }
        None => match &current {
            Some(environment) => actions.environment_reviewers(environment).await?.iter().map(|account| account.name.to_string()).collect(),
            None => Vec::new(),
        },
    };
    let branch_policy = match body.deployment_branch_policy {
        Some(Some(policy)) if policy.protected_branches => BranchPolicy::Protected,
        Some(Some(policy)) if policy.custom_branch_policies => BranchPolicy::Selected(policy.patterns),
        Some(_) => BranchPolicy::All,
        None => current.as_ref().map(|environment| environment.branch_policy.clone()).unwrap_or_default(),
    };
    let prevent_self_review =
        body.prevent_self_review.or_else(|| current.as_ref().map(|environment| environment.prevent_self_review)).unwrap_or(false);
    let wait_timer = body.wait_timer.or_else(|| current.as_ref().map(|environment| environment.wait_timer)).unwrap_or(0);
    let input = EnvironmentInput { reviewers, prevent_self_review, branch_policy, wait_timer };
    let environment =
        actions.put_environment(user, &ctx, path.name()?, &input).await.map_err(|error| ApiError::service(error, "environment"))?;
    Ok(Json(value(&state, &ctx, &environment).await?))
}

/// `DELETE /repos/{owner}/{repo}/environments/{environment_name}`: `204`.
pub async fn delete(State(state): State<AppState>, caller: Caller, Path(path): Path<EnvironmentPath>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Admin)?;
    if !ctx.access.can_admin() {
        return Err(ApiError::forbidden("You must be an administrator of this repository"));
    }
    actions(&state)?.delete_environment(user, &ctx, path.name()?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The pending deployments of a run, as GitHub returns them.
async fn pending_values(state: &AppState, ctx: &RepoContext, pending: &[PendingDeployment]) -> ApiResult<Vec<Value>> {
    let base = state.config.server.base_url();
    let mut values = Vec::new();
    for deployment in pending {
        let reviewers: Vec<Value> = actions(state)?
            .environment_reviewers(&deployment.environment)
            .await?
            .iter()
            .map(|reviewer| json!({ "type": "User", "reviewer": account(reviewer, base) }))
            .collect();
        let (url, html_url) = urls(base, ctx, &deployment.environment.name);
        values.push(json!({
            "environment": {
                "id": deployment.environment.id.0,
                "name": deployment.environment.name,
                "url": url,
                "html_url": html_url,
            },
            "wait_timer": deployment.environment.wait_timer,
            "wait_timer_started_at": null,
            "current_user_can_approve": deployment.can_review,
            "reviewers": reviewers,
        }));
    }
    Ok(values)
}

/// `GET /repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments`
pub async fn pending(State(state): State<AppState>, caller: Caller, Path(path): Path<EnvironmentPath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let actions = actions(&state)?;
    let run = actions.run_by_id(&ctx, path.run()?).await?.run;
    let pending = actions.pending_deployments(caller.user(), &ctx, &run).await?;
    Ok(Json(Value::Array(pending_values(&state, &ctx, &pending).await?)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ReviewBody {
    environment_ids: Vec<i64>,
    state: String,
    comment: String,
}

/// `POST /repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments`: approves or
/// rejects the jobs waiting for these environments; `200` with what still waits.
pub async fn review(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<EnvironmentPath>,
    ApiJson(body): ApiJson<ReviewBody>,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    let decision = match body.state.as_str() {
        "approved" => ReviewDecision::Approved,
        "rejected" => ReviewDecision::Rejected,
        _ => return Err(ApiError::invalid("state", "state must be approved or rejected")),
    };
    let actions = actions(&state)?;
    let run = actions.run_by_id(&ctx, path.run()?).await?.run;
    let pending = actions.pending_deployments(Some(user), &ctx, &run).await?;
    let mut names = Vec::new();
    for id in &body.environment_ids {
        let Some(deployment) = pending.iter().find(|deployment| deployment.environment.id == EnvironmentId(*id)) else {
            return Err(ApiError::invalid("environment_ids", format!("no job of this run waits for environment {id}")));
        };
        names.push(deployment.environment.name.clone());
    }
    actions.review_deployments(user, &ctx, &run, &names, decision, &body.comment).await.map_err(|error| match error {
        rbit_services::ServiceError::Invalid(message) => ApiError::new(StatusCode::CONFLICT, message),
        other => other.into(),
    })?;
    let left = actions.pending_deployments(Some(user), &ctx, &run).await?;
    Ok(Json(Value::Array(pending_values(&state, &ctx, &left).await?)).into_response())
}
