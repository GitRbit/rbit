//! Repository label and issue label API handlers.

use super::{
    ApiError, ApiJson, ApiResult, AppState, Caller, Deserialize, IssueContext, IssuePath, Json, LabelInput, NamedPath, Paged, Pager, Path,
    RepoPath, Response, State, StatusCode, TokenScope, Value, base, created, issue_json, labels_value, open_thread, reopen, require_writer,
};

#[derive(Debug, Deserialize)]
pub(super) struct LabelNames {
    #[serde(default)]
    labels: Vec<String>,
}

async fn labels_response(state: &AppState, caller: &Caller, ctx: &IssueContext) -> ApiResult<Json<Value>> {
    let ctx = reopen(state, caller, ctx).await?;
    let metadata = state.services.issues.metadata(&[ctx.issue.id]).await?.remove(&ctx.issue.id).unwrap_or_default();
    Ok(Json(Value::Array(labels_value(&metadata, &ctx.repo.repo, base(state)))))
}

/// `GET /repos/{owner}/{repo}/issues/{number}/labels`
pub(super) async fn issue_labels(State(state): State<AppState>, caller: Caller, Path(path): Path<IssuePath>) -> ApiResult<Json<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    labels_response(&state, &caller, &ctx).await
}

/// `POST /repos/{owner}/{repo}/issues/{number}/labels`: adds labels by name.
pub(super) async fn add_labels(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IssuePath>,
    ApiJson(body): ApiJson<LabelNames>,
) -> ApiResult<Json<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    let issues = &state.services.issues;
    let labels = issues.labels_named(&ctx.repo, &body.labels).await.map_err(|e| ApiError::service(e, "labels"))?;
    let ids: Vec<_> = labels.iter().map(|l| l.id).collect();
    issues.change_labels(user, &ctx, &ids, &[]).await.map_err(|e| ApiError::service(e, "labels"))?;
    labels_response(&state, &caller, &ctx).await
}

/// `PUT /repos/{owner}/{repo}/issues/{number}/labels`: replaces the labels.
pub(super) async fn set_labels(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IssuePath>,
    ApiJson(body): ApiJson<LabelNames>,
) -> ApiResult<Json<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    let issues = &state.services.issues;
    let labels = issues.labels_named(&ctx.repo, &body.labels).await.map_err(|e| ApiError::service(e, "labels"))?;
    let ids: Vec<_> = labels.iter().map(|l| l.id).collect();
    issues.set_labels(user, &ctx, &ids).await.map_err(|e| ApiError::service(e, "labels"))?;
    labels_response(&state, &caller, &ctx).await
}

/// `DELETE /repos/{owner}/{repo}/issues/{number}/labels`: removes every label.
pub(super) async fn clear_labels(State(state): State<AppState>, caller: Caller, Path(path): Path<IssuePath>) -> ApiResult<StatusCode> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    state.services.issues.set_labels(user, &ctx, &[]).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /repos/{owner}/{repo}/issues/{number}/labels/{name}`: 404 when not on it.
pub(super) async fn remove_label(State(state): State<AppState>, caller: Caller, Path(path): Path<NamedPath>) -> ApiResult<Json<Value>> {
    let number = path.number.as_deref().unwrap_or_default();
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, number).await?;
    let user = caller.require(TokenScope::Write)?;
    let issues = &state.services.issues;
    let label =
        issues.label_named(&ctx.repo, &path.name).await.map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "Label does not exist"))?;
    let metadata = issues.metadata(&[ctx.issue.id]).await?.remove(&ctx.issue.id).unwrap_or_default();
    if !metadata.labels.iter().any(|l| l.id == label.id) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "Label does not exist"));
    }
    issues.change_labels(user, &ctx, &[], &[label.id]).await?;
    labels_response(&state, &caller, &ctx).await
}

/// `GET /repos/{owner}/{repo}/labels`
pub(super) async fn list_labels(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<RepoPath>,
) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let labels = state.services.issues.labels(&ctx).await?;
    Ok(pager.page_of(labels.iter().map(|label| issue_json::label_detail(label, &ctx.repo, base(&state))).collect()))
}

#[derive(Debug, Deserialize)]
pub(super) struct NewLabelBody {
    name: String,
    color: String,
    #[serde(default)]
    description: Option<String>,
}

/// `POST /repos/{owner}/{repo}/labels`
pub(super) async fn create_label(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    ApiJson(body): ApiJson<NewLabelBody>,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    require_writer(&ctx)?;
    let input = LabelInput { name: body.name, color: body.color, description: body.description.unwrap_or_default() };
    let label = state.services.issues.create_label(user, &ctx, &input).await.map_err(|e| ApiError::service(e, "name"))?;
    let value = issue_json::label_detail(&label, &ctx.repo, base(&state));
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    Ok(created(&location, value))
}

/// `GET /repos/{owner}/{repo}/labels/{name}`
pub(super) async fn get_label(State(state): State<AppState>, caller: Caller, Path(path): Path<NamedPath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let label = state.services.issues.label_named(&ctx, &path.name).await?;
    Ok(Json(issue_json::label_detail(&label, &ctx.repo, base(&state))))
}

#[derive(Debug, Deserialize)]
pub(super) struct LabelChanges {
    new_name: Option<String>,
    color: Option<String>,
    description: Option<String>,
}

/// `PATCH /repos/{owner}/{repo}/labels/{name}`
pub(super) async fn update_label(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<NamedPath>,
    ApiJson(changes): ApiJson<LabelChanges>,
) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    require_writer(&ctx)?;
    let issues = &state.services.issues;
    let label = issues.label_named(&ctx, &path.name).await?;
    let input = LabelInput {
        name: changes.new_name.unwrap_or(label.name),
        color: changes.color.unwrap_or(label.color),
        description: changes.description.unwrap_or(label.description),
    };
    let updated = issues.update_label(user, &ctx, label.id, &input).await.map_err(|e| ApiError::service(e, "new_name"))?;
    Ok(Json(issue_json::label_detail(&updated, &ctx.repo, base(&state))))
}

/// `DELETE /repos/{owner}/{repo}/labels/{name}`
pub(super) async fn delete_label(State(state): State<AppState>, caller: Caller, Path(path): Path<NamedPath>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    caller.require(TokenScope::Write)?;
    require_writer(&ctx)?;
    let issues = &state.services.issues;
    let label = issues.label_named(&ctx, &path.name).await?;
    issues.delete_label(&ctx, label.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
