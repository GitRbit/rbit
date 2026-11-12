//! Issues as an agent's task queue (agent plan step 5.4): taking an issue, and finding one
//! by its words before filing another.

use super::{
    ApiError, ApiResult, AppState, Caller, Deserialize, IssuePath, IssueState, Json, Path, Query, RepoPath, SearchFilter, State,
    StatusCode, TokenScope, Value, issue_value, json, open_thread, reopen,
};

/// This file's endpoints, appended to the API table beside the issues area's own.
pub(crate) fn routes() -> Vec<super::ApiRoute> {
    let repo = |suffix: &str| format!("/repos/{{owner}}/{{repo}}{suffix}");
    vec![super::route(super::Post, repo("/issues/{number}/take"), take), super::route(super::Get, repo("/agent/issues"), search)]
}

/// `POST /repos/{owner}/{repo}/issues/{number}/take`: assigns the issue to the caller only
/// if nobody holds it, as one conditional write; `409` with code `conflict` naming the
/// holder otherwise. Taking an issue one already holds succeeds.
pub(super) async fn take(State(state): State<AppState>, caller: Caller, Path(path): Path<IssuePath>) -> ApiResult<Json<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    if let Some(holder) = state.services.issues.take(user, &ctx).await.map_err(|error| ApiError::service(error, "assignees"))? {
        return Err(ApiError::new(StatusCode::CONFLICT, format!("issue #{} is held by {holder}", path.number)));
    }
    let ctx = reopen(&state, &caller, &ctx).await?;
    Ok(Json(issue_value(&state, &ctx).await?))
}

/// The query of an issue search.
#[derive(Debug, Deserialize)]
pub(super) struct SearchQuery {
    q: String,
    #[serde(default)]
    state: Option<String>,
}

/// Issues a search answers with.
const SEARCH_LIMIT: u32 = 20;

/// `GET /repos/{owner}/{repo}/agent/issues?q=<words>[&state=open|closed|all]`: issues and
/// pull requests whose title or body has every word, best first, one short row each, at
/// most 20 (`schema: rbit.issue-search/1`).
pub(super) async fn search(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let words: Vec<String> = query.q.split_whitespace().take(10).map(str::to_owned).collect();
    if words.is_empty() {
        return Err(ApiError::invalid("q", "q must have at least one word"));
    }
    let mut filter = SearchFilter::new(ctx.repo.repo.id);
    filter.text = words;
    filter.state = match query.state.as_deref().unwrap_or("all") {
        "open" => Some(IssueState::Open),
        "closed" => Some(IssueState::Closed),
        "all" => None,
        _ => return Err(ApiError::invalid("state", "state must be open, closed or all")),
    };
    let found = state.services.issues.list(&ctx, filter, 1, SEARCH_LIMIT).await?;
    let items: Vec<Value> = found
        .issues
        .iter()
        .map(|issue| {
            let metadata = found.metadata.get(&issue.id);
            let labels: Vec<&str> =
                metadata.map(|metadata| metadata.labels.iter().map(|label| label.name.as_str()).collect()).unwrap_or_default();
            let assignees: Vec<&str> =
                metadata.map(|metadata| metadata.assignees.iter().map(|account| account.name.as_str()).collect()).unwrap_or_default();
            json!({
                "number": issue.number.get(),
                "title": issue.title.as_str(),
                "state": issue.state.as_str(),
                "pull_request": issue.kind == super::IssueKind::Pull,
                "labels": labels,
                "assignees": assignees,
                "updated_at": super::timestamp(issue.updated_at),
            })
        })
        .collect();
    Ok(Json(json!({ "schema": "rbit.issue-search/1", "total": found.total, "items": items })))
}
