//! Issue assignee API handlers.

use super::{
    Account, AccountId, ApiError, ApiJson, ApiResult, AppState, Caller, Deserialize, IntoResponse, IssuePath, Json, Paged, Pager, Path,
    RepoPath, Response, State, StatusCode, TokenScope, Value, account, base, issue_value, open_thread, reopen,
};

#[derive(Debug, Deserialize)]
pub(super) struct AssigneeNames {
    #[serde(default)]
    assignees: Vec<String>,
}

/// `GET /repos/{owner}/{repo}/assignees`: people who may be assigned (can push).
pub(super) async fn assignees(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<RepoPath>,
) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let people: Vec<Account> = state.services.issues.assignable(&ctx, "").await?;
    Ok(pager.page_of(people.iter().map(|person| account(person, base(&state))).collect()))
}

async fn change_assignees(state: &AppState, caller: &Caller, path: &IssuePath, names: &[String], adding: bool) -> ApiResult<Response> {
    let ctx = open_thread(state, caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    let issues = &state.services.issues;
    let ids: Vec<AccountId> =
        issues.users_named(names).await.map_err(|e| ApiError::service(e, "assignees"))?.iter().map(|a| a.id).collect();
    let result =
        if adding { issues.change_assignees(user, &ctx, &ids, &[]).await } else { issues.change_assignees(user, &ctx, &[], &ids).await };
    result.map_err(|e| ApiError::service(e, "assignees"))?;
    let ctx = reopen(state, caller, &ctx).await?;
    let value = issue_value(state, &ctx).await?;
    Ok(if adding { (StatusCode::CREATED, Json(value)).into_response() } else { Json(value).into_response() })
}

/// `POST /repos/{owner}/{repo}/issues/{number}/assignees`
pub(super) async fn add_assignees(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IssuePath>,
    ApiJson(body): ApiJson<AssigneeNames>,
) -> ApiResult<Response> {
    change_assignees(&state, &caller, &path, &body.assignees, true).await
}

/// `DELETE /repos/{owner}/{repo}/issues/{number}/assignees`
pub(super) async fn remove_assignees(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IssuePath>,
    ApiJson(body): ApiJson<AssigneeNames>,
) -> ApiResult<Response> {
    change_assignees(&state, &caller, &path, &body.assignees, false).await
}
