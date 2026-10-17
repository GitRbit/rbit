//! REST API: Branch protection rules: `/repos/{owner}/{repo}/branch_protections…` (admin scope).
//!
//! Milestone 2 workstream B (branch protection) owns this file and `openapi/protection.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Every endpoint needs the `admin` scope and repository administrator rights: callers who
//! cannot read the repository get 404, those who can but lack either get 403. Rule ids
//! the repository does not have are 404.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{
    TokenScope,
    protection::{BranchProtection, ProtectionId, RuleSettings},
    status::StatusContext,
};
use rbit_services::{RepoContext, ServiceError, protection::ProtectionInput, webhooks::payloads};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Delete, Get, Patch, Post},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::ApiJson,
    pagination::{Paged, Pager},
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    let base = "/repos/{owner}/{repo}/branch_protections";
    vec![
        route(Get, base, list),
        route(Post, base, create),
        route(Get, format!("{base}/{{id}}"), show),
        route(Patch, format!("{base}/{{id}}"), update),
        route(Delete, format!("{base}/{{id}}"), delete),
    ]
}

/// The path parameters of every endpoint.
#[derive(Debug, Deserialize)]
pub struct RulePath {
    owner: String,
    repo: String,
    id: Option<String>,
}

impl RulePath {
    fn rule(&self) -> ApiResult<ProtectionId> {
        self.id.as_deref().and_then(|id| id.parse::<i64>().ok()).filter(|id| *id > 0).map(ProtectionId).ok_or_else(ApiError::not_found)
    }
}

/// Opens the repository (404 if hidden), then checks the token scope and the caller's
/// rights (403).
async fn open(state: &AppState, caller: &Caller, path: &RulePath) -> ApiResult<RepoContext> {
    let ctx = caller.open_repo(state, &path.owner, &path.repo).await?;
    caller.require(TokenScope::Admin)?;
    if !ctx.access.can_admin() {
        return Err(ApiError::forbidden("You must be an administrator of this repository"));
    }
    Ok(ctx)
}

/// `…/branch_protections` of `ctx`.
fn rules_url(state: &AppState, ctx: &RepoContext) -> String {
    format!("{}/api/v1/repos/{}/branch_protections", state.config.server.base_url(), ctx.repo.full_name())
}

/// A rule as the API shows it.
fn rule_json(rule: &BranchProtection, rules_url: &str) -> Value {
    let settings = &rule.rule.settings;
    json!({
        "id": rule.id.0,
        "url": format!("{rules_url}/{}", rule.id.0),
        "pattern": rule.rule.pattern.as_str(),
        "deny_force_push": settings.deny_force_push,
        "deny_deletion": settings.deny_deletion,
        "require_pull_request": settings.require_pull_request,
        "required_approvals": settings.required_approvals,
        "dismiss_stale_approvals": settings.dismiss_stale_approvals,
        "block_on_changes_requested": settings.block_on_changes_requested,
        "restrict_pushers": settings.restrict_pushers,
        "allow_admin_bypass": settings.allow_admin_bypass,
        "require_status_checks": settings.require_status_checks,
        "required_status_checks": rule.rule.required_contexts.iter().map(StatusContext::as_str).collect::<Vec<_>>(),
        "push_allowlist_users": rule.users.iter().map(|user| user.name.as_str()).collect::<Vec<_>>(),
        "push_allowlist_teams": rule.teams.iter().map(|team| team.name.as_str()).collect::<Vec<_>>(),
        "created_at": payloads::timestamp(rule.created_at),
        "updated_at": payloads::timestamp(rule.updated_at),
    })
}

/// The body of `POST` and `PATCH`; absent fields keep their value (or the default of a
/// new rule).
#[derive(Debug, Default, Deserialize)]
pub struct RuleBody {
    pattern: Option<String>,
    deny_force_push: Option<bool>,
    deny_deletion: Option<bool>,
    require_pull_request: Option<bool>,
    required_approvals: Option<u8>,
    dismiss_stale_approvals: Option<bool>,
    block_on_changes_requested: Option<bool>,
    restrict_pushers: Option<bool>,
    allow_admin_bypass: Option<bool>,
    push_allowlist_users: Option<Vec<String>>,
    push_allowlist_teams: Option<Vec<String>>,
    require_status_checks: Option<bool>,
    required_status_checks: Option<Vec<String>>,
}

impl RuleBody {
    /// `current` with this body's fields applied.
    fn apply(self, current: ProtectionInput) -> ProtectionInput {
        let base = current.settings;
        let settings = RuleSettings {
            deny_force_push: self.deny_force_push.unwrap_or(base.deny_force_push),
            deny_deletion: self.deny_deletion.unwrap_or(base.deny_deletion),
            require_pull_request: self.require_pull_request.unwrap_or(base.require_pull_request),
            required_approvals: self.required_approvals.unwrap_or(base.required_approvals),
            dismiss_stale_approvals: self.dismiss_stale_approvals.unwrap_or(base.dismiss_stale_approvals),
            block_on_changes_requested: self.block_on_changes_requested.unwrap_or(base.block_on_changes_requested),
            restrict_pushers: self.restrict_pushers.unwrap_or(base.restrict_pushers),
            allow_admin_bypass: self.allow_admin_bypass.unwrap_or(base.allow_admin_bypass),
            require_status_checks: self.require_status_checks.unwrap_or(base.require_status_checks),
        };
        ProtectionInput {
            pattern: self.pattern.unwrap_or(current.pattern),
            settings,
            users: self.push_allowlist_users.unwrap_or(current.users),
            teams: self.push_allowlist_teams.unwrap_or(current.teams),
            contexts: self.required_status_checks.unwrap_or(current.contexts),
        }
    }
}

/// Validation messages say what they are about; attribute them to the closest field.
fn field_error(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::Invalid(message) if message.contains("approvals") => "required_approvals",
        ServiceError::Invalid(message) if message.contains("check") => "required_status_checks",
        ServiceError::Invalid(message) if message.contains("team") => "push_allowlist_teams",
        ServiceError::Invalid(message) if message.contains("user") || message.contains("write access") => "push_allowlist_users",
        _ => "pattern",
    };
    ApiError::service(error, field)
}

/// `GET …/branch_protections`, by pattern.
pub async fn list(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<RulePath>) -> ApiResult<Paged<Value>> {
    let ctx = open(&state, &caller, &path).await?;
    let rules = state.services.protection.list(&ctx).await?;
    let url = rules_url(&state, &ctx);
    Ok(pager.page_of(rules.iter().map(|rule| rule_json(rule, &url)).collect()))
}

/// `POST …/branch_protections`: `201 Created`.
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RulePath>,
    ApiJson(body): ApiJson<RuleBody>,
) -> ApiResult<Response> {
    let ctx = open(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    if body.pattern.is_none() {
        return Err(ApiError::invalid("pattern", "pattern is required"));
    }
    let input = body.apply(ProtectionInput::default());
    let rule = state.services.protection.create(user, &ctx, &input).await.map_err(field_error)?;
    let value = rule_json(&rule, &rules_url(&state, &ctx));
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    Ok((StatusCode::CREATED, [(header::LOCATION, location)], Json(value)).into_response())
}

/// `GET …/branch_protections/{id}`
pub async fn show(State(state): State<AppState>, caller: Caller, Path(path): Path<RulePath>) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &path).await?;
    let rule = state.services.protection.get(&ctx, path.rule()?).await?;
    Ok(Json(rule_json(&rule, &rules_url(&state, &ctx))))
}

/// `PATCH …/branch_protections/{id}`: absent fields keep their value.
pub async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RulePath>,
    ApiJson(body): ApiJson<RuleBody>,
) -> ApiResult<Json<Value>> {
    let ctx = open(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    let protection = &state.services.protection;
    let current = protection.get(&ctx, path.rule()?).await?;
    let input = body.apply(ProtectionInput::from_rule(&current));
    let rule = protection.update(user, &ctx, current.id, &input).await.map_err(field_error)?;
    Ok(Json(rule_json(&rule, &rules_url(&state, &ctx))))
}

/// `DELETE …/branch_protections/{id}`: `204`.
pub async fn delete(State(state): State<AppState>, caller: Caller, Path(path): Path<RulePath>) -> ApiResult<StatusCode> {
    let ctx = open(&state, &caller, &path).await?;
    state.services.protection.delete(caller.require_user()?, &ctx, path.rule()?).await?;
    Ok(StatusCode::NO_CONTENT)
}
