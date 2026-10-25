//! REST API: Issues, comments, labels, milestones, assignees and the timeline:
//! `/repos/{owner}/{repo}/issues…`, `/labels…`, `/milestones…`, `/assignees`.
//!
//! Milestone 2 workstream I (issues) owns this file and `openapi/issues.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Shapes and filters follow GitHub's. Pull requests appear among issues with a
//! `pull_request` key and share the number sequence. Scopes: `read` for every `GET`,
//! `write` for changes (also for readers who comment), `admin` to delete an issue.
//! Documented deviations: milestones are addressed by id (GitHub numbers them per
//! repository), `sort=comments` is always descending, and label names containing `/`
//! cannot be addressed in a path.

mod assignees;
mod comments;
mod labels;
mod milestones;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{
    Account, AccountId, CommentId, IssueKind, IssueNumber, IssueState, MilestoneId, StateReason, TokenScope, issue_query::IssueSort,
    repository::OwnedRepository,
};
use rbit_services::{
    IssueContext, RepoContext, ServiceError,
    issues::{IssueMetadata, LabelInput, MilestoneInput, NewIssueInput, SearchFilter, json as issue_json},
    timeline::{MAX_TIMELINE_PAGE, StateChange, TimelineEntry},
    webhooks::payloads::{account, timestamp},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Delete, Get, Patch, Post, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::{ApiJson, present},
    pagination::{Paged, Pager},
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    let repo = |suffix: &str| format!("/repos/{{owner}}/{{repo}}{suffix}");
    vec![
        route(Get, repo("/issues"), list_issues),
        route(Post, repo("/issues"), create_issue),
        route(Get, repo("/issues/comments/{id}"), comments::get_comment),
        route(Patch, repo("/issues/comments/{id}"), comments::edit_comment),
        route(Delete, repo("/issues/comments/{id}"), comments::delete_comment),
        route(Get, repo("/issues/{number}"), get_issue),
        route(Patch, repo("/issues/{number}"), update_issue),
        route(Delete, repo("/issues/{number}"), delete_issue),
        route(Put, repo("/issues/{number}/lock"), lock),
        route(Delete, repo("/issues/{number}/lock"), unlock),
        route(Get, repo("/issues/{number}/comments"), comments::list_comments),
        route(Post, repo("/issues/{number}/comments"), comments::create_comment),
        route(Get, repo("/issues/{number}/timeline"), comments::timeline),
        route(Get, repo("/issues/{number}/labels"), labels::issue_labels),
        route(Post, repo("/issues/{number}/labels"), labels::add_labels),
        route(Put, repo("/issues/{number}/labels"), labels::set_labels),
        route(Delete, repo("/issues/{number}/labels"), labels::clear_labels),
        route(Delete, repo("/issues/{number}/labels/{*name}"), labels::remove_label),
        route(Post, repo("/issues/{number}/assignees"), assignees::add_assignees),
        route(Delete, repo("/issues/{number}/assignees"), assignees::remove_assignees),
        route(Get, repo("/assignees"), assignees::assignees),
        route(Get, repo("/labels"), labels::list_labels),
        route(Post, repo("/labels"), labels::create_label),
        route(Get, repo("/labels/{*name}"), labels::get_label),
        route(Patch, repo("/labels/{*name}"), labels::update_label),
        route(Delete, repo("/labels/{*name}"), labels::delete_label),
        route(Get, repo("/milestones"), milestones::list_milestones),
        route(Post, repo("/milestones"), milestones::create_milestone),
        route(Get, repo("/milestones/{id}"), milestones::get_milestone),
        route(Patch, repo("/milestones/{id}"), milestones::update_milestone),
        route(Delete, repo("/milestones/{id}"), milestones::delete_milestone),
    ]
}

// ------------------------------------------------------------------ helpers

#[derive(Debug, Deserialize)]
pub struct RepoPath {
    owner: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
pub struct IssuePath {
    owner: String,
    repo: String,
    number: String,
}

#[derive(Debug, Deserialize)]
pub struct NamedPath {
    owner: String,
    repo: String,
    #[serde(default)]
    number: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct IdPath {
    owner: String,
    repo: String,
    id: String,
}

fn base(state: &AppState) -> &str {
    state.config.server.base_url()
}

fn parse_id(id: &str) -> ApiResult<i64> {
    id.parse::<i64>().ok().filter(|id| *id > 0).ok_or_else(ApiError::not_found)
}

/// Thread `number` of the repository, for the caller; 404 when hidden or unknown.
async fn open_thread(state: &AppState, caller: &Caller, owner: &str, repo: &str, number: &str) -> ApiResult<IssueContext> {
    let ctx = caller.open_repo(state, owner, repo).await?;
    let number = IssueNumber::parse(number).map_err(|_| ApiError::not_found())?;
    Ok(state.services.timeline.open(caller.user(), ctx, number).await?)
}

/// The full issue object of `ctx` (author and metadata loaded).
async fn issue_value(state: &AppState, ctx: &IssueContext) -> ApiResult<Value> {
    let metadata = state.services.issues.metadata(&[ctx.issue.id]).await?.remove(&ctx.issue.id).unwrap_or_default();
    Ok(issue_json::issue_with(&ctx.issue, ctx.author.as_ref(), &metadata, &ctx.repo.repo, base(state)))
}

/// Reopens thread `number` after a change, for an up-to-date response.
async fn reopen(state: &AppState, caller: &Caller, ctx: &IssueContext) -> ApiResult<IssueContext> {
    Ok(state.services.timeline.open(caller.user(), ctx.repo.clone(), ctx.issue.number).await?)
}

fn labels_value(metadata: &IssueMetadata, repo: &OwnedRepository, base_url: &str) -> Vec<Value> {
    metadata.labels.iter().map(|label| issue_json::label(label, repo, base_url)).collect()
}

/// `201 Created` with `Location`.
fn created(location: &str, body: Value) -> Response {
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
}

fn require_writer(ctx: &RepoContext) -> ApiResult<()> {
    if ctx.access.can_write() { Ok(()) } else { Err(ApiError::forbidden("You must be able to push to this repository")) }
}

/// `2026-10-15T12:00:00Z` (or with an offset, or fractional seconds) as Unix seconds.
pub fn parse_rfc3339(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| value.get(range).filter(|s| s.bytes().all(|b| b.is_ascii_digit()))?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    // A numeric day in 1..=31 can still be invalid for its month or leap year.
    time::Date::from_calendar_date(
        i32::try_from(year).ok()?,
        time::Month::try_from(u8::try_from(month).ok()?).ok()?,
        u8::try_from(day).ok()?,
    )
    .ok()?;
    let mut rest = &value[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let (sign, zone) = match rest.as_bytes().first()? {
                b'+' => (1, &rest[1..]),
                b'-' => (-1, &rest[1..]),
                _ => return None,
            };
            let (hours, minutes) = zone.split_once(':')?;
            if hours.len() != 2 || minutes.len() != 2 || !hours.bytes().chain(minutes.bytes()).all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let (hours, minutes) = (hours.parse::<i64>().ok()?, minutes.parse::<i64>().ok()?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

// ------------------------------------------------------------------ issues

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    state: Option<String>,
    labels: Option<String>,
    milestone: Option<String>,
    assignee: Option<String>,
    creator: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
    since: Option<String>,
}

/// An account id for `login`; unknown logins match nothing.
async fn login_id(state: &AppState, login: &str) -> ApiResult<AccountId> {
    let found = state.services.issues.users_named(&[login.to_owned()]).await;
    Ok(found.ok().and_then(|users| users.first().map(|user| user.id)).unwrap_or(AccountId(0)))
}

/// `GET /repos/{owner}/{repo}/issues`: issues and pull requests, filtered like GitHub.
pub async fn list_issues(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<RepoPath>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let mut filter = SearchFilter::new(ctx.repo.repo.id);
    filter.state = match query.state.as_deref().unwrap_or("open") {
        "open" => Some(IssueState::Open),
        "closed" => Some(IssueState::Closed),
        "all" => None,
        _ => return Err(ApiError::invalid("state", "state must be open, closed or all")),
    };
    if let Some(labels) = &query.labels {
        filter.labels = labels.split(',').map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
    }
    match query.milestone.as_deref() {
        None => {}
        Some("*") => filter.any_milestone = true,
        Some("none") => filter.no_milestone = true,
        Some(id) => {
            let id = id.parse::<i64>().map_err(|_| ApiError::invalid("milestone", "milestone must be an id, * or none"))?;
            match state.services.issues.milestone(&ctx, MilestoneId(id)).await {
                Ok(progress) => filter.milestone = Some(progress.milestone.title),
                Err(ServiceError::NotFound) => return Ok(pager.paged(Vec::new(), Some(0), false)),
                Err(error) => return Err(error.into()),
            }
        }
    }
    match query.assignee.as_deref() {
        None => {}
        Some("*") => filter.any_assignee = true,
        Some("none") => filter.no_assignee = true,
        Some(login) => filter.assignee = Some(login_id(&state, login).await?),
    }
    if let Some(login) = &query.creator {
        filter.author = Some(login_id(&state, login).await?);
    }
    let ascending = match query.direction.as_deref() {
        None | Some("desc") => false,
        Some("asc") => true,
        Some(_) => return Err(ApiError::invalid("direction", "direction must be asc or desc")),
    };
    filter.sort = match (query.sort.as_deref().unwrap_or("created"), ascending) {
        ("created", false) => IssueSort::CreatedDesc,
        ("created", true) => IssueSort::CreatedAsc,
        ("updated", false) => IssueSort::UpdatedDesc,
        ("updated", true) => IssueSort::UpdatedAsc,
        ("comments", _) => IssueSort::CommentsDesc,
        _ => return Err(ApiError::invalid("sort", "sort must be created, updated or comments")),
    };
    if let Some(since) = &query.since {
        let at = parse_rfc3339(since).ok_or_else(|| ApiError::invalid("since", "since must be an ISO 8601 timestamp"))?;
        filter.updated_since = Some(rbit_core::Timestamp(at));
    }
    let per_page = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let results = state.services.issues.list(&ctx, filter, pager.page, per_page).await?;
    let base_url = base(&state);
    let items = results
        .issues
        .iter()
        .map(|issue| {
            let author = issue.author.and_then(|id| results.authors.get(&id));
            let metadata = results.metadata.get(&issue.id).cloned().unwrap_or_default();
            issue_json::issue_with(issue, author, &metadata, &ctx.repo, base_url)
        })
        .collect();
    Ok(pager.paged(items, Some(results.total), false))
}

#[derive(Debug, Deserialize)]
pub struct NewIssueBody {
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    assignees: Vec<String>,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default)]
    milestone: Option<i64>,
}

/// `POST /repos/{owner}/{repo}/issues`: labels, assignees and milestone are applied only
/// for writers (silently dropped otherwise, as on GitHub).
pub async fn create_issue(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    ApiJson(body): ApiJson<NewIssueBody>,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    let issues = &state.services.issues;
    let mut input = NewIssueInput { title: body.title, body: body.body.unwrap_or_default(), ..NewIssueInput::default() };
    if ctx.access.can_write() {
        if !body.labels.is_empty() {
            input.labels =
                issues.labels_named(&ctx, &body.labels).await.map_err(|e| ApiError::service(e, "labels"))?.iter().map(|l| l.id).collect();
        }
        let mut logins = body.assignees;
        logins.extend(body.assignee);
        if !logins.is_empty() {
            input.assignees =
                issues.users_named(&logins).await.map_err(|e| ApiError::service(e, "assignees"))?.iter().map(|a| a.id).collect();
        }
        input.milestone = body.milestone.map(MilestoneId);
    }
    let issue = issues.create(user, &ctx, input).await.map_err(|e| ApiError::service(e, "title"))?;
    let thread = state.services.timeline.open(caller.user(), ctx, issue.number).await?;
    let value = issue_value(&state, &thread).await?;
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    Ok(created(&location, value))
}

/// `GET /repos/{owner}/{repo}/issues/{number}`
pub async fn get_issue(State(state): State<AppState>, caller: Caller, Path(path): Path<IssuePath>) -> ApiResult<Json<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    Ok(Json(issue_value(&state, &ctx).await?))
}

#[derive(Debug, Deserialize)]
pub struct IssueChanges {
    title: Option<String>,
    /// A string sets it, `null` empties it.
    #[serde(default, deserialize_with = "present")]
    body: Option<Value>,
    state: Option<String>,
    state_reason: Option<String>,
    labels: Option<Vec<String>>,
    assignees: Option<Vec<String>>,
    /// An id sets it, `null` clears it.
    #[serde(default, deserialize_with = "present")]
    milestone: Option<Value>,
}

/// `PATCH /repos/{owner}/{repo}/issues/{number}`: absent fields keep their value.
pub async fn update_issue(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<IssuePath>,
    ApiJson(changes): ApiJson<IssueChanges>,
) -> ApiResult<Json<Value>> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    let body = match changes.body {
        None => None,
        Some(Value::Null) => Some(String::new()),
        Some(Value::String(text)) => Some(text),
        Some(_) => return Err(ApiError::invalid("body", "body must be a string or null")),
    };
    let timeline = &state.services.timeline;
    let issues = &state.services.issues;
    if changes.title.is_some() || body.is_some() {
        timeline.edit(user, &ctx, changes.title.as_deref(), body.as_deref()).await.map_err(|e| ApiError::service(e, "title"))?;
    }
    if let Some(wanted) = changes.state.as_deref() {
        let change = match (wanted, changes.state_reason.as_deref()) {
            ("open", _) => StateChange::Reopen,
            ("closed", None | Some("completed")) => StateChange::Close(StateReason::Completed),
            ("closed", Some("not_planned")) => StateChange::Close(StateReason::NotPlanned),
            ("closed", Some(_)) => return Err(ApiError::invalid("state_reason", "state_reason must be completed or not_planned")),
            _ => return Err(ApiError::invalid("state", "state must be open or closed")),
        };
        let current = reopen(&state, &caller, &ctx).await?;
        timeline.set_state(user, &current, change).await.map_err(|e| ApiError::service(e, "state"))?;
    }
    if let Some(names) = &changes.labels {
        let labels = issues.labels_named(&ctx.repo, names).await.map_err(|e| ApiError::service(e, "labels"))?;
        let ids: Vec<_> = labels.iter().map(|l| l.id).collect();
        issues.set_labels(user, &ctx, &ids).await.map_err(|e| ApiError::service(e, "labels"))?;
    }
    if let Some(logins) = &changes.assignees {
        let accounts = issues.users_named(logins).await.map_err(|e| ApiError::service(e, "assignees"))?;
        let ids: Vec<AccountId> = accounts.iter().map(|a| a.id).collect();
        issues.set_assignees(user, &ctx, &ids).await.map_err(|e| ApiError::service(e, "assignees"))?;
    }
    if let Some(milestone) = changes.milestone {
        let milestone = match milestone {
            Value::Null => None,
            Value::Number(id) => Some(MilestoneId(id.as_i64().ok_or_else(|| ApiError::invalid("milestone", "milestone must be an id"))?)),
            _ => return Err(ApiError::invalid("milestone", "milestone must be an id or null")),
        };
        issues.set_milestone(user, &ctx, milestone).await.map_err(|e| ApiError::service(e, "milestone"))?;
    }
    let ctx = reopen(&state, &caller, &ctx).await?;
    Ok(Json(issue_value(&state, &ctx).await?))
}

/// `DELETE /repos/{owner}/{repo}/issues/{number}`: repository administrators with the
/// `admin` scope; pull requests cannot be deleted.
pub async fn delete_issue(State(state): State<AppState>, caller: Caller, Path(path): Path<IssuePath>) -> ApiResult<StatusCode> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Admin)?;
    state.services.issues.delete(user, &ctx).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /repos/{owner}/{repo}/issues/{number}/lock`
pub async fn lock(State(state): State<AppState>, caller: Caller, Path(path): Path<IssuePath>) -> ApiResult<StatusCode> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    state.services.timeline.set_locked(user, &ctx, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /repos/{owner}/{repo}/issues/{number}/lock`
pub async fn unlock(State(state): State<AppState>, caller: Caller, Path(path): Path<IssuePath>) -> ApiResult<StatusCode> {
    let ctx = open_thread(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let user = caller.require(TokenScope::Write)?;
    state.services.timeline.set_locked(user, &ctx, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use rbit_core::{Visibility, membership::Permission};

    use super::{super::test_support::Fixture, *};

    #[test]
    fn rfc3339_timestamps() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2025-10-09T08:53:20Z"), Some(1_760_000_000));
        assert_eq!(parse_rfc3339("2025-10-09T10:53:20+02:00"), Some(1_760_000_000));
        assert_eq!(parse_rfc3339("2025-10-09T08:53:20.123Z"), Some(1_760_000_000));
        assert!(parse_rfc3339("2024-02-29T00:00:00Z").is_some());
        for bad in [
            "",
            "2025-10-09",
            "2025-10-09T08:53:20",
            "2025-13-09T08:53:20Z",
            "2025-10-09T08:53:20+0200",
            "2025-10-09 08:53:20Z",
            "x025-10-09T08:53:20Z",
            "2025-02-29T00:00:00Z",
            "2024-02-30T00:00:00Z",
            "2025-04-31T00:00:00Z",
            "2025-10-09T08:53:20+24:00",
            "2025-10-09T08:53:20+00:60",
            "2025-10-09T08:53:20+-1:00",
            "2025-10-09T08:53:20+00:-1",
            "2025-10-09T08:53:20++1:00",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
    }

    #[tokio::test]
    async fn hidden_repositories_are_404_and_scopes_apply() {
        let fixture = Fixture::new(|_| {}).await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        fixture.repo(&alice, "vault", Visibility::Private).await;
        let bobs = fixture.token(&bob, TokenScope::Admin).await;
        let alices_read = fixture.token(&alice, TokenScope::Read).await;
        let alices_write = fixture.token(&alice, TokenScope::Write).await;
        for path in [
            "/api/v1/repos/alice/vault/issues",
            "/api/v1/repos/alice/vault/labels",
            "/api/v1/repos/alice/vault/milestones",
            "/api/v1/repos/alice/vault/issues/1",
        ] {
            let (response, _) = fixture.send("GET", path, Some(&bobs), None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            let (response, _) = fixture.send("GET", path, None, None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "anonymous {path}");
        }
        let (response, _) = fixture.send("POST", "/api/v1/repos/alice/vault/issues", Some(&bobs), Some(json!({ "title": "x" }))).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let (response, _) =
            fixture.send("POST", "/api/v1/repos/alice/vault/issues", Some(&alices_read), Some(json!({ "title": "x" }))).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "the read scope cannot write");
        let (response, json) =
            fixture.send("POST", "/api/v1/repos/alice/vault/issues", Some(&alices_write), Some(json!({ "title": "x" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{json}");
        let (response, _) = fixture.send("DELETE", "/api/v1/repos/alice/vault/issues/1", Some(&alices_write), None).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "deletion needs the admin scope");
        let (response, _) = fixture.send("GET", "/api/v1/repos/alice/vault/issues/abc", Some(&alices_read), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    #[expect(clippy::too_many_lines, reason = "one walk through the issue endpoints")]
    async fn issues_labels_milestones_comments_and_timeline() {
        let fixture = Fixture::new(|config| config.api.max_page_size = 2).await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        fixture.repo(&alice, "demo", Visibility::Public).await;
        let ctx = fixture.services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
        fixture.services.collaborators.add(&alice, &ctx, "bob", Permission::Write).await.unwrap();
        let token = fixture.token(&alice, TokenScope::Admin).await;
        let bobs = fixture.token(&bob, TokenScope::Write).await;
        let api = "/api/v1/repos/alice/demo";
        let send = |method: &'static str, path: String, body: Option<Value>| {
            let token = token.clone();
            let fixture = &fixture;
            async move { fixture.send(method, &path, Some(&token), body).await }
        };

        let (response, label) = send("POST", format!("{api}/labels"), Some(json!({ "name": "good first issue", "color": "7057FF" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{label}");
        assert_eq!(
            (label["color"].as_str(), label["url"].as_str()),
            (Some("7057ff"), Some("https://git.example.com/api/v1/repos/alice/demo/labels/good%20first%20issue"))
        );
        let (response, _) = send("POST", format!("{api}/labels"), Some(json!({ "name": "Good First Issue", "color": "000000" }))).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let (response, _) = send("POST", format!("{api}/labels"), Some(json!({ "name": "bug", "color": "red" }))).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        send("POST", format!("{api}/labels"), Some(json!({ "name": "bug", "color": "d73a4a" }))).await;
        let (response, renamed) = send("PATCH", format!("{api}/labels/bug"), Some(json!({ "new_name": "defect" }))).await;
        assert_eq!(
            (response.status(), renamed["name"].as_str(), renamed["color"].as_str()),
            (StatusCode::OK, Some("defect"), Some("d73a4a"))
        );
        let (response, _) = send("GET", format!("{api}/labels/good%20first%20issue"), None).await;
        assert_eq!(response.status(), StatusCode::OK);

        let (response, milestone) =
            send("POST", format!("{api}/milestones"), Some(json!({ "title": "v1", "due_on": "2026-12-31T00:00:00Z" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{milestone}");
        assert_eq!(milestone["due_on"], "2026-12-31T00:00:00Z");
        let milestone_id = milestone["id"].as_i64().unwrap();

        let new =
            json!({ "title": "Crash", "body": "Boom", "labels": ["good first issue"], "assignees": ["bob"], "milestone": milestone_id });
        let (response, issue) = send("POST", format!("{api}/issues"), Some(new)).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{issue}");
        assert_eq!(response.headers()[header::LOCATION], "https://git.example.com/api/v1/repos/alice/demo/issues/1");
        assert_eq!((issue["labels"][0]["name"].as_str(), issue["assignee"]["login"].as_str()), (Some("good first issue"), Some("bob")));
        assert_eq!(issue["milestone"]["title"], "v1");
        assert_eq!(issue["user"]["login"], "alice");
        assert!(!issue.to_string().contains("@example.com"), "no email addresses");
        for title in ["Second", "Third"] {
            send("POST", format!("{api}/issues"), Some(json!({ "title": title }))).await;
        }

        // Paging with Link headers and filters.
        let (response, page) = send("GET", format!("{api}/issues"), None).await;
        assert_eq!((page.as_array().unwrap().len(), response.headers()["x-total-count"].to_str().unwrap()), (2, "3"));
        let link = response.headers()[header::LINK].to_str().unwrap();
        assert!(link.contains("page=2>; rel=\"next\""), "{link}");
        let (_, page) = send("GET", format!("{api}/issues?page=2"), None).await;
        assert_eq!(page[0]["title"], "Crash", "newest first");
        let (_, filtered) =
            send("GET", format!("{api}/issues?labels=good%20first%20issue&assignee=bob&milestone={milestone_id}"), None).await;
        assert_eq!(filtered.as_array().unwrap().len(), 1);
        let (_, none) = send("GET", format!("{api}/issues?assignee=none&sort=created&direction=asc"), None).await;
        assert_eq!(none[0]["title"], "Second");
        let (_, any) = send("GET", format!("{api}/issues?milestone=*&creator=alice&since=2000-01-01T00:00:00Z"), None).await;
        assert_eq!(any.as_array().unwrap().len(), 1);
        let (response, _) = send("GET", format!("{api}/issues?state=sideways"), None).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // Changing an issue.
        let patch = json!({ "title": "Crash on start", "state": "closed", "state_reason": "not_planned", "labels": ["defect"], "assignees": [], "milestone": null });
        let (response, changed) = send("PATCH", format!("{api}/issues/1"), Some(patch)).await;
        assert_eq!(response.status(), StatusCode::OK, "{changed}");
        assert_eq!((changed["state"].as_str(), changed["state_reason"].as_str()), (Some("closed"), Some("not_planned")));
        assert_eq!((changed["labels"][0]["name"].as_str(), changed["assignees"].as_array().unwrap().len()), (Some("defect"), 0));
        assert_eq!(changed["milestone"], Value::Null);
        let (_, reopened) = send("PATCH", format!("{api}/issues/1"), Some(json!({ "state": "open" }))).await;
        assert_eq!(reopened["state"], "open");

        // Labels on the issue.
        let (_, labels) = send("POST", format!("{api}/issues/1/labels"), Some(json!({ "labels": ["good first issue"] }))).await;
        assert_eq!(labels.as_array().unwrap().len(), 2);
        let (response, labels) = send("DELETE", format!("{api}/issues/1/labels/defect"), None).await;
        assert_eq!((response.status(), labels.as_array().unwrap().len()), (StatusCode::OK, 1));
        let (response, _) = send("DELETE", format!("{api}/issues/1/labels/defect"), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let (_, labels) = send("PUT", format!("{api}/issues/1/labels"), Some(json!({ "labels": ["defect"] }))).await;
        assert_eq!(labels[0]["name"], "defect");
        let (response, _) = send("DELETE", format!("{api}/issues/1/labels"), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Assignees.
        let (_, people) = send("GET", format!("{api}/assignees"), None).await;
        assert_eq!(people.as_array().unwrap().len(), 2);
        let (response, assigned) = send("POST", format!("{api}/issues/1/assignees"), Some(json!({ "assignees": ["bob"] }))).await;
        assert_eq!((response.status(), assigned["assignees"][0]["login"].as_str()), (StatusCode::CREATED, Some("bob")));
        let (_, unassigned) = send("DELETE", format!("{api}/issues/1/assignees"), Some(json!({ "assignees": ["bob"] }))).await;
        assert_eq!(unassigned["assignees"], json!([]));
        let (response, _) = send("POST", format!("{api}/issues/1/assignees"), Some(json!({ "assignees": ["nobody"] }))).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // Comments, by issue and by id.
        let (response, comment) =
            fixture.send("POST", &format!("{api}/issues/1/comments"), Some(&bobs), Some(json!({ "body": "Same here" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{comment}");
        let id = comment["id"].as_i64().unwrap();
        assert_eq!(comment["html_url"], format!("https://git.example.com/alice/demo/issues/1#issuecomment-{id}"));
        let (_, comments) = send("GET", format!("{api}/issues/1/comments"), None).await;
        assert_eq!(comments[0]["body"], "Same here");
        let (response, _) = send("PATCH", format!("{api}/issues/comments/{id}"), Some(json!({ "body": "Not mine" }))).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "only its author edits a comment");
        let (_, edited) =
            fixture.send("PATCH", &format!("{api}/issues/comments/{id}"), Some(&bobs), Some(json!({ "body": "Same here too" }))).await;
        assert_eq!(edited["body"], "Same here too");
        let (_, fetched) = send("GET", format!("{api}/issues/comments/{id}"), None).await;
        assert_eq!(fetched["body"], "Same here too");
        let (_, comments) = send("GET", format!("{api}/issues/1/comments"), None).await;
        assert_eq!(fetched, comments[0], "a comment looks the same fetched alone and in its thread");
        let (response, _) = send("GET", "/api/v1/repos/alice/demo/issues/comments/999".to_owned(), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Lock, timeline, delete.
        let (response, _) = send("PUT", format!("{api}/issues/1/lock"), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (_, issue) = send("GET", format!("{api}/issues/1"), None).await;
        assert_eq!(issue["locked"], true);
        let mut events: Vec<String> = Vec::new();
        for page in 1.. {
            let (response, timeline) = send("GET", format!("{api}/issues/1/timeline?page={page}"), None).await;
            assert_eq!(response.status(), StatusCode::OK);
            let items = timeline.as_array().unwrap();
            if items.is_empty() {
                break;
            }
            events.extend(items.iter().filter_map(|item| item["event"].as_str().map(str::to_owned)));
        }
        let events: Vec<&str> = events.iter().map(String::as_str).collect();
        assert!(events.contains(&"commented") && events.contains(&"labeled") && events.contains(&"locked"), "{events:?}");
        let (response, _) = send("DELETE", format!("{api}/issues/comments/{id}"), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "moderators delete comments");
        let (response, _) = send("DELETE", format!("{api}/issues/1"), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (response, _) = send("GET", format!("{api}/issues/1"), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Milestones and labels go away.
        let (_, list) = send("GET", format!("{api}/milestones?state=all"), None).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        let (_, closed) =
            send("PATCH", format!("{api}/milestones/{milestone_id}"), Some(json!({ "state": "closed", "description": "Done" }))).await;
        assert_eq!((closed["state"].as_str(), closed["description"].as_str()), (Some("closed"), Some("Done")));
        let (response, _) = send("DELETE", format!("{api}/milestones/{milestone_id}"), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (response, _) = send("DELETE", format!("{api}/labels/defect"), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (response, _) = fixture
            .send(
                "DELETE",
                &format!("{api}/labels/good%20first%20issue"),
                Some(&fixture.token(&fixture.user("carol").await, TokenScope::Write).await),
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "readers cannot change labels");
    }

    #[tokio::test]
    async fn notifications_list_and_change_through_the_api() {
        let fixture = Fixture::new(|_| {}).await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        fixture.repo(&alice, "demo", Visibility::Public).await;
        let mut events = fixture.services.events.subscribe();
        let alices = fixture.token(&alice, TokenScope::Write).await;
        let bobs = fixture.token(&bob, TokenScope::Write).await;
        fixture.send("POST", "/api/v1/repos/alice/demo/issues", Some(&alices), Some(json!({ "title": "Hello", "body": "cc @bob" }))).await;
        fixture.deliver(&mut events).await;

        let (response, threads) = fixture.send("GET", "/api/v1/notifications", Some(&bobs), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let thread = &threads[0];
        assert_eq!((thread["reason"].as_str(), thread["unread"].as_bool()), (Some("mention"), Some(true)));
        assert_eq!(thread["subject"]["url"], "https://git.example.com/api/v1/repos/alice/demo/issues/1");
        assert_eq!(thread["repository"]["full_name"], "alice/demo");
        let id = thread["id"].as_str().unwrap().to_owned();
        let (response, _) = fixture.send("PATCH", &format!("/api/v1/notifications/threads/{id}"), Some(&alices), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "only one's own threads");
        let (response, _) = fixture.send("PATCH", &format!("/api/v1/notifications/threads/{id}"), Some(&bobs), None).await;
        assert_eq!(response.status(), StatusCode::RESET_CONTENT);
        let (_, unread) = fixture.send("GET", "/api/v1/notifications", Some(&bobs), None).await;
        assert_eq!(unread, json!([]));
        let (_, all) = fixture.send("GET", "/api/v1/notifications?all=true", Some(&bobs), None).await;
        assert_eq!(all.as_array().unwrap().len(), 1);
        let (response, _) = fixture.send("PUT", "/api/v1/notifications", Some(&bobs), None).await;
        assert_eq!(response.status(), StatusCode::RESET_CONTENT);
        let (response, _) = fixture.send("DELETE", &format!("/api/v1/notifications/threads/{id}/subscription"), Some(&bobs), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (response, _) = fixture.send("GET", "/api/v1/notifications", None, None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let off = Fixture::new(|config| config.issues.notifications = false).await;
        let carol = off.user("carol").await;
        let token = off.token(&carol, TokenScope::Read).await;
        let (response, _) = off.send("GET", "/api/v1/notifications", Some(&token), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
