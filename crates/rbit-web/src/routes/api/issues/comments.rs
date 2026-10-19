//! Issue comments and timeline API handlers.

use super::{
    ApiError, ApiJson, ApiResult, AppState, Caller, CommentId, Deserialize, IdPath, IssueContext, IssueKind, IssuePath, Json,
    MAX_TIMELINE_PAGE, Paged, Pager, Path, Response, State, StatusCode, TimelineEntry, TokenScope, Value, account, base, created,
    issue_json, json, open_thread, parse_id, timestamp,
};

/// Every entry of `ctx`'s timeline, page by page (bounded by the thread's size).
async fn all_entries(state: &AppState, caller: &Caller, ctx: &IssueContext) -> ApiResult<Vec<TimelineEntry>> {
    let mut entries = Vec::new();
    let mut cursor = None;
    loop {
        let page = state.services.timeline.timeline(caller.user(), ctx, cursor, MAX_TIMELINE_PAGE).await?;
        entries.extend(page.entries);
        match page.next {
            Some(next) => cursor = Some(next),
            None => return Ok(entries),
        }
    }
}

fn comment_value(entry: &TimelineEntry, ctx: &IssueContext, base_url: &str) -> Option<Value> {
    match entry {
        TimelineEntry::Comment { comment, author, .. } => {
            Some(issue_json::comment(comment, author.as_ref(), &ctx.issue, &ctx.repo.repo, base_url))
        }
        TimelineEntry::Event { .. } => None,
    }
}

/// `GET /repos/{owner}/{repo}/issues/{number}/comments`
pub(super) async fn list_comments(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<IssuePath>,
) -> ApiResult<Paged<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let entries = all_entries(&state, &caller, &ctx).await?;
    Ok(pager.page_of(entries.iter().filter_map(|entry| comment_value(entry, &ctx, base(&state))).collect()))
}

#[derive(Debug, Deserialize)]
pub(super) struct CommentBody {
    body: String,
}

/// `POST /repos/{owner}/{repo}/issues/{number}/comments`
pub(super) async fn create_comment(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IssuePath>,
    ApiJson(body): ApiJson<CommentBody>,
) -> ApiResult<Response> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    let comment = state.services.timeline.comment(user, &ctx, &body.body).await.map_err(|e| ApiError::service(e, "body"))?;
    let value = issue_json::comment(&comment, Some(&user.account), &ctx.issue, &ctx.repo.repo, base(&state));
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    Ok(created(&location, value))
}

/// The thread holding comment `id` of the repository, for the caller.
async fn comment_thread(state: &AppState, caller: &Caller, path: &IdPath) -> ApiResult<(IssueContext, CommentId)> {
    let id = CommentId(parse_id(&path.id)?);
    let repo = caller.open_repo(state, &path.owner, &path.repo).await?;
    let number = state.services.issues.comment_thread(&repo, id).await?;
    Ok((state.services.timeline.open(caller.user(), repo, number).await?, id))
}

/// The comment `id` on `ctx` as the API shows it.
async fn find_comment(state: &AppState, caller: &Caller, ctx: &IssueContext, id: CommentId) -> ApiResult<Value> {
    let entries = all_entries(state, caller, ctx).await?;
    entries
        .iter()
        .find(|entry| matches!(entry, TimelineEntry::Comment { comment, .. } if comment.id == id))
        .and_then(|entry| comment_value(entry, ctx, base(state)))
        .ok_or_else(ApiError::not_found)
}

/// `GET /repos/{owner}/{repo}/issues/comments/{id}`
pub(super) async fn get_comment(State(state): State<AppState>, caller: Caller, Path(path): Path<IdPath>) -> ApiResult<Json<Value>> {
    let (ctx, id) = comment_thread(&state, &caller, &path).await?;
    Ok(Json(find_comment(&state, &caller, &ctx, id).await?))
}

/// `PATCH /repos/{owner}/{repo}/issues/comments/{id}`: its author only.
pub(super) async fn edit_comment(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IdPath>,
    ApiJson(body): ApiJson<CommentBody>,
) -> ApiResult<Json<Value>> {
    let (ctx, id) = comment_thread(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Write)?;
    let comment = state.services.timeline.edit_comment(user, &ctx, id, &body.body).await.map_err(|e| ApiError::service(e, "body"))?;
    Ok(Json(issue_json::comment(&comment, Some(&user.account), &ctx.issue, &ctx.repo.repo, base(&state))))
}

/// `DELETE /repos/{owner}/{repo}/issues/comments/{id}`: its author or a moderator.
pub(super) async fn delete_comment(State(state): State<AppState>, caller: Caller, Path(path): Path<IdPath>) -> ApiResult<StatusCode> {
    let (ctx, id) = comment_thread(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Write)?;
    state.services.timeline.delete_comment(user, &ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /repos/{owner}/{repo}/issues/{number}/timeline`: comments (`event: "commented"`)
/// and events in order; cross-references the caller cannot read are left out.
pub(super) async fn timeline(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<IssuePath>,
) -> ApiResult<Paged<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let base_url = base(&state);
    let entries = all_entries(&state, &caller, &ctx).await?;
    let items = entries
        .iter()
        .map(|entry| match entry {
            TimelineEntry::Comment { comment, author, .. } => {
                let mut value = issue_json::comment(comment, author.as_ref(), &ctx.issue, &ctx.repo.repo, base_url);
                value["event"] = json!("commented");
                value["actor"] = value["user"].clone();
                value
            }
            TimelineEntry::Event { id, actor, event, at, subjects } => {
                let mut value = json!({
                    "id": id.0,
                    "event": event.kind(),
                    "actor": actor.as_ref().map(|actor| account(actor, base_url)),
                    "created_at": timestamp(*at),
                });
                if let Ok(Value::Object(tagged)) = serde_json::to_value(event)
                    && let Some(Value::Object(data)) = tagged.get("data")
                {
                    value["data"] = Value::Object(data.clone());
                }
                if let Some(subject) = &subjects.account {
                    value["subject"] = account(subject, base_url);
                }
                if let Some(source) = &subjects.source {
                    value["source"] = json!({
                        "repository": source.full_name,
                        "number": source.number.get(),
                        "type": if source.kind == IssueKind::Pull { "pull_request" } else { "issue" },
                        "state": source.state.as_str(),
                        "title": source.title,
                    });
                }
                value
            }
        })
        .collect();
    Ok(pager.page_of(items))
}
