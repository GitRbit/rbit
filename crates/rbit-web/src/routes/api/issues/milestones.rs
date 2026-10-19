//! Repository milestone API handlers.

use super::{
    ApiError, ApiJson, ApiResult, AppState, Caller, Deserialize, IdPath, IssueState, Json, MilestoneId, MilestoneInput, Paged, Pager, Path,
    Query, RepoPath, Response, State, StatusCode, TokenScope, Value, base, created, issue_json, parse_id, present, require_writer,
};

#[derive(Debug, Deserialize)]
pub(super) struct MilestoneListQuery {
    state: Option<String>,
}

/// `GET /repos/{owner}/{repo}/milestones[?state=open|closed|all]`
pub(super) async fn list_milestones(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<RepoPath>,
    Query(query): Query<MilestoneListQuery>,
) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let wanted = match query.state.as_deref().unwrap_or("open") {
        "open" => Some(IssueState::Open),
        "closed" => Some(IssueState::Closed),
        "all" => None,
        _ => return Err(ApiError::invalid("state", "state must be open, closed or all")),
    };
    let milestones = state.services.issues.milestones(&ctx, wanted).await?;
    Ok(pager.page_of(milestones.iter().map(|progress| issue_json::milestone_detail(progress, &ctx.repo, base(&state))).collect()))
}

#[derive(Debug, Deserialize)]
pub(super) struct MilestoneBody {
    title: Option<String>,
    #[serde(default, deserialize_with = "present")]
    description: Option<Value>,
    /// `YYYY-MM-DD`, an RFC 3339 timestamp (its day is used) or `null`.
    #[serde(default, deserialize_with = "present")]
    due_on: Option<Value>,
    state: Option<String>,
}

/// The day of `due_on`: `YYYY-MM-DD` as is, a timestamp's first ten characters.
fn due_day(value: Option<&Value>, current: Option<&str>) -> ApiResult<String> {
    match value {
        None => Ok(current.unwrap_or_default().to_owned()),
        Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.chars().take(10).collect()),
        Some(_) => Err(ApiError::invalid("due_on", "due_on must be a date or null")),
    }
}

fn text_field(value: Option<&Value>, current: &str, field: &str) -> ApiResult<String> {
    match value {
        None => Ok(current.to_owned()),
        Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(_) => Err(ApiError::invalid(field, format!("{field} must be a string or null"))),
    }
}

fn milestone_state(value: Option<&str>) -> ApiResult<Option<IssueState>> {
    match value {
        None => Ok(None),
        Some("open") => Ok(Some(IssueState::Open)),
        Some("closed") => Ok(Some(IssueState::Closed)),
        Some(_) => Err(ApiError::invalid("state", "state must be open or closed")),
    }
}

/// `POST /repos/{owner}/{repo}/milestones`
pub(super) async fn create_milestone(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    ApiJson(body): ApiJson<MilestoneBody>,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    require_writer(&ctx)?;
    let input = MilestoneInput {
        title: body.title.unwrap_or_default(),
        description: text_field(body.description.as_ref(), "", "description")?,
        due_on: due_day(body.due_on.as_ref(), None)?,
    };
    let wanted = milestone_state(body.state.as_deref())?;
    let issues = &state.services.issues;
    let milestone = issues.create_milestone(user, &ctx, &input).await.map_err(|e| ApiError::service(e, "title"))?;
    if wanted == Some(IssueState::Closed) {
        issues.set_milestone_state(&ctx, milestone.id, IssueState::Closed).await?;
    }
    let progress = issues.milestone(&ctx, milestone.id).await?;
    let value = issue_json::milestone_detail(&progress, &ctx.repo, base(&state));
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    Ok(created(&location, value))
}

/// `GET /repos/{owner}/{repo}/milestones/{id}`
pub(super) async fn get_milestone(State(state): State<AppState>, caller: Caller, Path(path): Path<IdPath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let progress = state.services.issues.milestone(&ctx, MilestoneId(parse_id(&path.id)?)).await?;
    Ok(Json(issue_json::milestone_detail(&progress, &ctx.repo, base(&state))))
}

/// `PATCH /repos/{owner}/{repo}/milestones/{id}`: absent fields keep their value.
pub(super) async fn update_milestone(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IdPath>,
    ApiJson(body): ApiJson<MilestoneBody>,
) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    require_writer(&ctx)?;
    let id = MilestoneId(parse_id(&path.id)?);
    let issues = &state.services.issues;
    let current = issues.milestone(&ctx, id).await?.milestone;
    let input = MilestoneInput {
        title: body.title.unwrap_or(current.title.clone()),
        description: text_field(body.description.as_ref(), &current.description, "description")?,
        due_on: due_day(body.due_on.as_ref(), current.due_on.as_deref())?,
    };
    issues.update_milestone(user, &ctx, id, &input).await.map_err(|e| ApiError::service(e, "title"))?;
    if let Some(wanted) = milestone_state(body.state.as_deref())? {
        issues.set_milestone_state(&ctx, id, wanted).await?;
    }
    let progress = issues.milestone(&ctx, id).await?;
    Ok(Json(issue_json::milestone_detail(&progress, &ctx.repo, base(&state))))
}

/// `DELETE /repos/{owner}/{repo}/milestones/{id}`
pub(super) async fn delete_milestone(State(state): State<AppState>, caller: Caller, Path(path): Path<IdPath>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    caller.require(TokenScope::Write)?;
    require_writer(&ctx)?;
    state.services.issues.delete_milestone(&ctx, MilestoneId(parse_id(&path.id)?)).await?;
    Ok(StatusCode::NO_CONTENT)
}
