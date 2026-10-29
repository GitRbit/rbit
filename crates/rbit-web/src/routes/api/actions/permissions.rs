//! REST API: Actions permissions, in GitHub's shapes (`…/actions/permissions…`) for a
//! repository (`/repos/{owner}/{repo}`), an organisation (`/orgs/{org}`) and the caller
//! (`/user`, rbit's own: GitHub has no user-level policies).
//!
//! * Everything needs the `admin` scope and the rights to manage the target, as secrets do;
//!   a repository the caller cannot see is 404.
//! * `PUT`s answer `204`. A value an owner's ceiling does not allow is `409`, as GitHub
//!   answers when an organisation's policy overrides a repository's; an unknown value is
//!   `422`.
//! * `selected-actions` can be set only while `allowed_actions` is `selected` (`409`
//!   otherwise, as on GitHub). `verified_allowed` is accepted and always reads `false`:
//!   rbit has no verified creators.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rbit_core::{
    RepoId,
    actions::{
        DefaultTokenPermission,
        permissions::{ActionsPolicies, AllowedActions, ApprovalPolicy, EnabledRepositories, SelectedActions},
    },
};
use rbit_services::actions::settings::{PermissionsChange, PermissionsError};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ActionsPath, Owner, actions, base_url};
use crate::{
    AppState,
    routes::api::{
        auth::Caller,
        error::{ApiError, ApiResult},
        json::ApiJson,
    },
};

/// The endpoints, under each of `prefixes` (a repository's, an organisation's, the user's).
pub(super) fn routes(prefixes: &[&str]) -> Vec<super::ApiRoute> {
    use super::super::{
        ApiMethod::{Delete, Get, Put},
        route,
    };
    let mut routes = Vec::new();
    for prefix in prefixes {
        let path = |suffix: &str| format!("{prefix}/permissions{suffix}");
        routes.extend([
            route(Get, path(""), show),
            route(Put, path(""), update),
            route(Get, path("/selected-actions"), show_selected_actions),
            route(Put, path("/selected-actions"), update_selected_actions),
            route(Get, path("/workflow"), show_workflow),
            route(Put, path("/workflow"), update_workflow),
            route(Get, path("/fork-pr-contributor-approval"), show_approval),
            route(Put, path("/fork-pr-contributor-approval"), update_approval),
        ]);
        // Actions: reusable workflows.
        if prefix.starts_with("/repos/") {
            routes.extend([route(Get, path("/access"), show_access), route(Put, path("/access"), update_access)]);
        }
        if !prefix.starts_with("/repos/") {
            routes.extend([
                route(Get, path("/repositories"), list_repositories),
                route(Put, path("/repositories"), set_repositories),
                route(Put, path("/repositories/{repository_id}"), enable_repository),
                route(Delete, path("/repositories/{repository_id}"), disable_repository),
            ]);
        }
    }
    routes
}

/// The target's policies and, for an owner, which repositories run workflows.
async fn policies(state: &AppState, owner: &Owner) -> ApiResult<(ActionsPolicies, Option<bool>, Option<EnabledRepositories>)> {
    let actions = actions(state)?;
    Ok(match owner {
        Owner::Repository(ctx) => {
            let permissions = actions.repo_permissions(ctx).await?;
            (permissions.policies, Some(permissions.enabled), None)
        }
        Owner::Organization(_) | Owner::User(_) => {
            let settings = actions.owner_permissions(owner.target()).await?;
            (settings.policies, None, Some(settings.enabled_repositories))
        }
    })
}

/// Applies `change` to the target.
async fn apply(state: &AppState, caller: &Caller, owner: &Owner, change: &PermissionsChange) -> ApiResult<StatusCode> {
    let user = caller.require_user()?;
    let actions = actions(state)?;
    match owner {
        Owner::Repository(ctx) => actions.set_repo_permissions(user, ctx, change).await.map_err(|error| match error {
            PermissionsError::Service(error) => ApiError::service(error, "permissions"),
            PermissionsError::Capped(field) => {
                ApiError::new(StatusCode::CONFLICT, format!("The owner's Actions policy does not allow this `{field}` for the repository"))
            }
        })?,
        Owner::Organization(_) | Owner::User(_) => {
            actions.set_owner_permissions(user, owner.target(), change).await.map_err(|error| ApiError::service(error, "permissions"))?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The API path of the target's permissions, for `…_url` fields.
fn permissions_url(state: &AppState, path: &ActionsPath) -> String {
    let base = base_url(state);
    match (&path.owner, &path.repo, &path.org) {
        (Some(owner), Some(repo), _) => format!("{base}/api/v1/repos/{owner}/{repo}/actions/permissions"),
        (_, _, Some(org)) => format!("{base}/api/v1/orgs/{org}/actions/permissions"),
        _ => format!("{base}/api/v1/user/actions/permissions"),
    }
}

fn parse<T>(field: &str, value: Option<&str>, parse: fn(&str) -> Option<T>) -> ApiResult<Option<T>> {
    value.map(|given| parse(given).ok_or_else(|| ApiError::invalid(field, format!("`{given}` is not a valid {field}")))).transpose()
}

// ------------------------------------------------------------------ permissions

/// `GET …/actions/permissions`
pub async fn show(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let (policies, enabled, repositories) = policies(&state, &owner).await?;
    let url = permissions_url(&state, &path);
    // GitHub's `sha_pinning_required`: rbit has no such policy, so it is always false.
    let mut body = json!({ "allowed_actions": policies.allowed_actions.as_str(), "sha_pinning_required": false });
    if let Some(enabled) = enabled {
        body["enabled"] = json!(enabled);
    }
    if let Some(repositories) = repositories {
        body["enabled_repositories"] = json!(repositories.as_str());
        if repositories == EnabledRepositories::Selected {
            body["selected_repositories_url"] = json!(format!("{url}/repositories"));
        }
    }
    if policies.allowed_actions == AllowedActions::Selected {
        body["selected_actions_url"] = json!(format!("{url}/selected-actions"));
    }
    Ok(Json(body))
}

#[derive(Debug, Deserialize)]
pub struct PermissionsBody {
    enabled: Option<bool>,
    enabled_repositories: Option<String>,
    allowed_actions: Option<String>,
}

/// `PUT …/actions/permissions`: `enabled` (a repository) or `enabled_repositories` (an
/// owner), and `allowed_actions`.
pub async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<PermissionsBody>,
) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let repository = matches!(owner, Owner::Repository(_));
    if repository && body.enabled.is_none() {
        return Err(ApiError::invalid("enabled", "enabled is required"));
    }
    if !repository && body.enabled_repositories.is_none() {
        return Err(ApiError::invalid("enabled_repositories", "enabled_repositories is required"));
    }
    let change = PermissionsChange {
        enabled: if repository { body.enabled } else { None },
        enabled_repositories: if repository {
            None
        } else {
            parse("enabled_repositories", body.enabled_repositories.as_deref(), EnabledRepositories::parse)?
        },
        allowed_actions: parse("allowed_actions", body.allowed_actions.as_deref(), AllowedActions::parse)?,
        ..PermissionsChange::default()
    };
    apply(&state, &caller, &owner, &change).await
}

// ------------------------------------------------------------------ selected actions

/// `GET …/actions/permissions/selected-actions`
pub async fn show_selected_actions(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let (policies, ..) = policies(&state, &owner).await?;
    let selected = &policies.selected_actions;
    Ok(Json(json!({
        "github_owned_allowed": selected.github_owned_allowed,
        "verified_allowed": false,
        "patterns_allowed": selected.patterns_allowed,
    })))
}

#[derive(Debug, Deserialize)]
#[expect(clippy::struct_field_names, reason = "GitHub's field names")]
pub struct SelectedActionsBody {
    github_owned_allowed: Option<bool>,
    #[expect(dead_code, reason = "accepted as on GitHub; rbit has no verified creators")]
    verified_allowed: Option<bool>,
    patterns_allowed: Option<Vec<String>>,
}

/// `PUT …/actions/permissions/selected-actions`
pub async fn update_selected_actions(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<SelectedActionsBody>,
) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let (policies, ..) = policies(&state, &owner).await?;
    if policies.allowed_actions != AllowedActions::Selected {
        return Err(ApiError::new(StatusCode::CONFLICT, "Set allowed_actions to `selected` before choosing the selected actions"));
    }
    let selected = SelectedActions {
        github_owned_allowed: body.github_owned_allowed.unwrap_or(policies.selected_actions.github_owned_allowed),
        patterns_allowed: body.patterns_allowed.unwrap_or(policies.selected_actions.patterns_allowed),
    };
    let change = PermissionsChange { selected_actions: Some(selected), ..PermissionsChange::default() };
    apply(&state, &caller, &owner, &change).await
}

// ------------------------------------------------------------------ the workflow token

/// `GET …/actions/permissions/workflow`
pub async fn show_workflow(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let (policies, ..) = policies(&state, &owner).await?;
    Ok(Json(json!({
        "default_workflow_permissions": policies.token_permission.as_str(),
        "can_approve_pull_request_reviews": policies.can_approve_reviews,
    })))
}

#[derive(Debug, Deserialize)]
pub struct WorkflowBody {
    default_workflow_permissions: Option<String>,
    can_approve_pull_request_reviews: Option<bool>,
}

/// `PUT …/actions/permissions/workflow`
pub async fn update_workflow(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<WorkflowBody>,
) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let change = PermissionsChange {
        token_permission: parse(
            "default_workflow_permissions",
            body.default_workflow_permissions.as_deref(),
            DefaultTokenPermission::parse,
        )?,
        can_approve_reviews: body.can_approve_pull_request_reviews,
        ..PermissionsChange::default()
    };
    apply(&state, &caller, &owner, &change).await
}

// ------------------------------------------------------------------ outside contributors

/// `GET …/actions/permissions/fork-pr-contributor-approval`
pub async fn show_approval(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let (policies, ..) = policies(&state, &owner).await?;
    Ok(Json(json!({ "approval_policy": policies.approval_policy.as_str() })))
}

#[derive(Debug, Deserialize)]
pub struct ApprovalBody {
    approval_policy: Option<String>,
}

/// `PUT …/actions/permissions/fork-pr-contributor-approval`
pub async fn update_approval(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<ApprovalBody>,
) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let Some(policy) = parse("approval_policy", body.approval_policy.as_deref(), ApprovalPolicy::parse)? else {
        return Err(ApiError::invalid("approval_policy", "approval_policy is required"));
    };
    let change = PermissionsChange { approval_policy: Some(policy), ..PermissionsChange::default() };
    apply(&state, &caller, &owner, &change).await
}

// ------------------------------------------------------------------ calling a private repository's workflows

// Actions: reusable workflows.
/// `GET /repos/{owner}/{repo}/actions/permissions/access`: who may call the repository's
/// workflows while it is private.
pub async fn show_access(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let Owner::Repository(ctx) = &owner else { return Err(ApiError::not_found()) };
    Ok(Json(json!({ "access_level": actions(&state)?.repo_access_level(ctx).await? })))
}

#[derive(Debug, Deserialize)]
pub struct AccessBody {
    access_level: Option<String>,
}

/// `PUT /repos/{owner}/{repo}/actions/permissions/access`: `none`, `user` or `organization`
/// (the owner's other private repositories may call its workflows).
pub async fn update_access(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<AccessBody>,
) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let Owner::Repository(ctx) = &owner else { return Err(ApiError::not_found()) };
    let user = caller.require_user()?;
    let level = body.access_level.ok_or_else(|| ApiError::invalid("access_level", "access_level is required"))?;
    actions(&state)?.set_repo_access_level(user, ctx, &level).await.map_err(|error| ApiError::service(error, "access_level"))?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ an owner's selected repositories

/// `GET /orgs/{org}/actions/permissions/repositories` (and `/user/…`)
pub async fn list_repositories(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let selected = actions(&state)?.owner_selected_repositories(owner.target()).await?;
    let owner_name = match &owner {
        Owner::Organization(org) => org.org.name().to_string(),
        Owner::User(user) => user.name().to_string(),
        Owner::Repository(_) => return Err(ApiError::not_found()),
    };
    let repositories: Vec<Value> = selected
        .iter()
        .map(|repo| json!({ "id": repo.id.0, "name": repo.name, "full_name": format!("{owner_name}/{}", repo.name) }))
        .collect();
    Ok(Json(json!({ "total_count": repositories.len(), "repositories": repositories })))
}

#[derive(Debug, Deserialize)]
pub struct RepositoriesBody {
    selected_repository_ids: Vec<i64>,
}

/// `PUT /orgs/{org}/actions/permissions/repositories`: replaces the selection.
pub async fn set_repositories(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<RepositoriesBody>,
) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    let repos: Vec<RepoId> = body.selected_repository_ids.into_iter().map(RepoId).collect();
    actions(&state)?.set_owner_selected_repositories(user, owner.target(), &repos).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /orgs/{org}/actions/permissions/repositories/{repository_id}`
pub async fn enable_repository(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    select(&state, &caller, &path, true).await
}

/// `DELETE /orgs/{org}/actions/permissions/repositories/{repository_id}`
pub async fn disable_repository(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    select(&state, &caller, &path, false).await
}

async fn select(state: &AppState, caller: &Caller, path: &ActionsPath, selected: bool) -> ApiResult<StatusCode> {
    let owner = Owner::load(state, caller, path).await?;
    let user = caller.require_user()?;
    let repo = RepoId(super::parse_id(path.repository_id.as_deref())?);
    actions(state)?.select_owner_repository(user, owner.target(), repo, selected).await?;
    Ok(StatusCode::NO_CONTENT)
}
