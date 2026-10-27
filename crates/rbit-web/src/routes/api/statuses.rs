//! REST API: Commit statuses, shaped like GitHub's commit status API:
//! `POST /repos/{owner}/{repo}/statuses/{sha}`, `GET /repos/{owner}/{repo}/commits/{ref}/statuses`
//! and `GET /repos/{owner}/{repo}/commits/{ref}/status` (the combined status).
//!
//! Milestone 3 workstream S (commit statuses and required checks) owns this file and
//! `openapi/statuses.json`, the `OpenAPI` fragment documenting exactly these routes
//! (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Creating needs a token with the `write` scope and write access; reading needs read
//! access. Repositories the caller may not see are 404, unknown references too.
//!
//! The two `GET` endpoints sit below `/commits/{*reference}` (a reference may contain
//! `/`), which the router cannot share with `/commits/{ref}/…`: they are served by
//! [`commit_or_statuses`], the handler of that wildcard, and listed in the table with
//! `route_in_wildcard` so the `OpenAPI` document stays checked against the table.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{TokenScope, status::CommitStatus};
use rbit_services::{RepoContext, ServiceError, statuses::NewStatus, statuses::hooks};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Get, Post},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::{self as api_json, ApiJson},
    pagination::{Paged, Pager},
    repos, route, route_in_wildcard,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    vec![
        route(Post, "/repos/{owner}/{repo}/statuses/{sha}", create),
        route_in_wildcard(Get, "/repos/{owner}/{repo}/commits/{ref}/statuses"),
        route_in_wildcard(Get, "/repos/{owner}/{repo}/commits/{ref}/status"),
    ]
}

fn base_url(state: &AppState) -> &str {
    state.config.server.base_url()
}

fn no_commit() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "No commit found for that revision")
}

/// `ServiceError::NotFound` from resolving a reference is "no such commit".
fn resolved<T>(result: Result<T, ServiceError>) -> ApiResult<T> {
    result.map_err(|error| match error {
        ServiceError::NotFound => no_commit(),
        other => other.into(),
    })
}

/// `statuses` as the API returns them, with their creators.
async fn statuses_json(state: &AppState, ctx: &RepoContext, statuses: &[CommitStatus]) -> ApiResult<Vec<Value>> {
    let creators = state.services.statuses.creators(statuses).await?;
    Ok(statuses
        .iter()
        .map(|status| {
            let creator = creators.iter().find(|account| Some(account.id) == status.creator);
            hooks::status(status, creator, &ctx.repo, base_url(state))
        })
        .collect())
}

/// The body of `POST …/statuses/{sha}`.
#[derive(Debug, Deserialize)]
pub struct StatusBody {
    state: String,
    target_url: Option<String>,
    description: Option<String>,
    context: Option<String>,
}

/// Validation messages name what they are about; attribute them to that field.
fn field_error(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::Invalid(message) if message.starts_with("state") => "state",
        ServiceError::Invalid(message) if message.starts_with("target_url") => "target_url",
        ServiceError::Invalid(message) if message.starts_with("description") => "description",
        ServiceError::Invalid(message) if message.starts_with("context") || message.contains("maximum number") => "context",
        _ => "sha",
    };
    ApiError::service(error, field)
}

/// `POST /repos/{owner}/{repo}/statuses/{sha}`: `201 Created`.
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, sha)): Path<(String, String, String)>,
    ApiJson(body): ApiJson<StatusBody>,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &owner, &name).await?;
    let input = NewStatus { state: body.state, target_url: body.target_url, description: body.description, context: body.context };
    // Actions: task tokens. A workflow's status is rbit Actions', not a person's.
    let status = if let Some(grant) = caller.task() {
        state.services.statuses.create_for_task(grant, &ctx, &sha, &input).await
    } else {
        let user = caller.require(TokenScope::Write)?;
        if !ctx.access.can_write() {
            return Err(ApiError::forbidden("You must have write access to this repository to create statuses"));
        }
        state.services.statuses.create(user, &ctx, &sha, &input).await
    }
    .map_err(field_error)?;
    let value = statuses_json(&state, &ctx, std::slice::from_ref(&status)).await?.remove(0);
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    Ok((StatusCode::CREATED, [(header::LOCATION, location)], Json(value)).into_response())
}

/// `GET /repos/{owner}/{repo}/commits/{*reference}`: the commit (see `repos::commit`),
/// or, for a reference ending in `/statuses` or `/status`, its statuses or combined status.
pub async fn commit_or_statuses(
    State(state): State<AppState>,
    caller: Caller,
    pager: Result<Pager, ApiError>,
    Path((owner, name, reference)): Path<(String, String, String)>,
) -> ApiResult<Response> {
    if let Some(reference) = reference.strip_suffix("/statuses").filter(|reference| !reference.is_empty()) {
        return Ok(list(&state, &caller, pager?, &owner, &name, reference).await?.into_response());
    }
    if let Some(reference) = reference.strip_suffix("/status").filter(|reference| !reference.is_empty()) {
        return Ok(combined(&state, &caller, &pager?, &owner, &name, reference).await?.into_response());
    }
    Ok(repos::commit(State(state), caller, Path((owner, name, reference))).await?.into_response())
}

/// `GET /repos/{owner}/{repo}/commits/{ref}/statuses`: every status, newest first.
async fn list(state: &AppState, caller: &Caller, pager: Pager, owner: &str, name: &str, reference: &str) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(state, owner, name).await?;
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u64::try_from(pager.offset()).unwrap_or(u64::MAX);
    let page = resolved(state.services.statuses.list(&ctx, reference, offset, limit).await)?;
    let items = statuses_json(state, &ctx, &page.statuses).await?;
    Ok(pager.paged(items, Some(page.total), false))
}

/// `GET /repos/{owner}/{repo}/commits/{ref}/status`: the combined state and the latest
/// status of each context (paginated with `page`/`per_page`; `total_count` counts all).
async fn combined(state: &AppState, caller: &Caller, pager: &Pager, owner: &str, name: &str, reference: &str) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(state, owner, name).await?;
    let combined = resolved(state.services.statuses.combined(&ctx, reference).await)?;
    let shown: Vec<CommitStatus> = combined.statuses.iter().skip(pager.offset()).take(pager.limit()).cloned().collect();
    let base = base_url(state);
    let api = format!("{base}/api/v1/repos/{}", ctx.repo.full_name());
    let fork = state.services.forks.is_fork(ctx.repo.repo.id).await?;
    Ok(Json(json!({
        "state": combined.state.as_str(),
        "statuses": statuses_json(state, &ctx, &shown).await?,
        "sha": combined.sha,
        "total_count": combined.statuses.len(),
        "repository": api_json::repository(&ctx.repo, None, fork, base),
        "commit_url": format!("{api}/commits/{}", combined.sha),
        "url": format!("{api}/commits/{}/status", combined.sha),
    })))
}

#[cfg(test)]
mod tests {
    use std::{path::Path as FsPath, process::Command};

    use rbit_core::{Visibility, membership::Permission};
    use rbit_services::CreateRepository;

    use super::{super::test_support::Fixture, *};

    fn git(dir: &FsPath, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// `main` with one commit and `release/1.0` (a branch with `/`) on it; returns the id.
    fn push_commit(disk_path: &FsPath) -> String {
        let work = tempfile::tempdir().unwrap();
        git(work.path(), &["init", "-q"]);
        std::fs::write(work.path().join("README.md"), "# Demo\n").unwrap();
        git(work.path(), &["add", "."]);
        git(work.path(), &["commit", "-q", "-m", "First"]);
        git(work.path(), &["branch", "release/1.0"]);
        git(work.path(), &["push", "-q", disk_path.to_str().unwrap(), "main", "release/1.0"]);
        git(work.path(), &["rev-parse", "HEAD"])
    }

    #[tokio::test]
    async fn statuses_are_created_listed_and_combined() {
        let fixture = Fixture::new(|_| {}).await;
        let alice = fixture.user("alice").await;
        let rita = fixture.user("rita").await;
        let olga = fixture.user("olga").await;
        let input = CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Private };
        fixture.services.repositories.create(&alice, input).await.unwrap();
        let repo = fixture.services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
        fixture.services.collaborators.add(&alice, &repo, "rita", Permission::Read).await.unwrap();
        let sha = push_commit(&repo.disk_path);
        let write = fixture.token(&alice, TokenScope::Write).await;
        let read = fixture.token(&alice, TokenScope::Read).await;
        let ritas = fixture.token(&rita, TokenScope::Write).await;
        let olgas = fixture.token(&olga, TokenScope::Write).await;
        let create = format!("/api/v1/repos/alice/demo/statuses/{sha}");

        let body = json!({ "state": "pending", "context": "ci/build", "description": "Queued", "target_url": "https://ci.example/1" });
        for (token, expected) in [
            (None, StatusCode::NOT_FOUND),
            (Some(&olgas), StatusCode::NOT_FOUND),
            (Some(&read), StatusCode::FORBIDDEN),
            (Some(&ritas), StatusCode::FORBIDDEN),
        ] {
            let (response, json) = fixture.send("POST", &create, token.map(String::as_str), Some(body.clone())).await;
            assert_eq!(response.status(), expected, "{json}");
        }
        let (response, json) = fixture.send("POST", &create, Some(&write), Some(body)).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{json}");
        assert_eq!(response.headers()[header::LOCATION], format!("https://git.example.com/api/v1/repos/alice/demo/statuses/{sha}"));
        assert_eq!(
            (json["state"].as_str(), json["context"].as_str(), json["description"].as_str()),
            (Some("pending"), Some("ci/build"), Some("Queued"))
        );
        assert_eq!((json["target_url"].as_str(), json["creator"]["login"].as_str()), (Some("https://ci.example/1"), Some("alice")));
        assert!(json["id"].is_i64() && json["created_at"].is_string());

        for (body, field) in [
            (json!({ "state": "ok" }), "state"),
            (json!({ "state": "success", "target_url": "ftp://x" }), "target_url"),
            (json!({ "state": "success", "description": "x".repeat(141) }), "description"),
            (json!({ "state": "success", "context": "x".repeat(256) }), "context"),
        ] {
            let (response, json) = fixture.send("POST", &create, Some(&write), Some(body)).await;
            assert_eq!((response.status(), json["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(field)), "{json}");
        }
        let bad_sha = "/api/v1/repos/alice/demo/statuses/abc";
        let (response, json) = fixture.send("POST", bad_sha, Some(&write), Some(json!({ "state": "success" }))).await;
        assert_eq!((response.status(), json["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("sha")));
        let (response, _) = fixture.send("POST", &create, Some(&write), Some(json!({ "context": "x" }))).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "state is required");
        let (response, _) = fixture.send("POST", &create, Some(&write), Some(json!({ "state": "success" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "context defaults");

        // Listing and the combined status, by a branch whose name contains `/`.
        let list = "/api/v1/repos/alice/demo/commits/release/1.0/statuses?per_page=1";
        let (response, json) = fixture.send("GET", list, Some(&read), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-total-count"], "2");
        assert!(response.headers()[header::LINK].to_str().unwrap().contains("rel=\"next\""));
        assert_eq!((json.as_array().map(Vec::len), json[0]["context"].as_str()), (Some(1), Some("default")), "newest first");
        let (response, json) = fixture.send("GET", "/api/v1/repos/alice/demo/commits/release/1.0/status", Some(&read), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            (json["state"].as_str(), json["sha"].as_str(), json["total_count"].as_u64()),
            (Some("pending"), Some(sha.as_str()), Some(2))
        );
        let contexts: Vec<_> = json["statuses"].as_array().unwrap().iter().map(|s| s["context"].as_str().unwrap()).collect();
        assert_eq!(contexts, ["ci/build", "default"]);
        assert_eq!(json["url"], format!("https://git.example.com/api/v1/repos/alice/demo/commits/{sha}/status"));
        assert_eq!(json["repository"]["full_name"], "alice/demo");
        let (_, json) = fixture.send("GET", "/api/v1/repos/alice/demo/commits/main/status?per_page=1&page=2", Some(&read), None).await;
        assert_eq!((json["statuses"].as_array().map(Vec::len), json["total_count"].as_u64()), (Some(1), Some(2)));

        // The plain commit endpoint behind the same wildcard still works; unknown refs are 404.
        let (response, json) = fixture.send("GET", "/api/v1/repos/alice/demo/commits/release/1.0", Some(&read), None).await;
        assert_eq!((response.status(), json["sha"].as_str()), (StatusCode::OK, Some(sha.as_str())));
        for path in ["/api/v1/repos/alice/demo/commits/nope/status", "/api/v1/repos/alice/demo/commits/nope/statuses"] {
            let (response, json) = fixture.send("GET", path, Some(&read), None).await;
            assert_eq!((response.status(), json["message"].as_str()), (StatusCode::NOT_FOUND, Some("No commit found for that revision")));
        }
        let (response, _) = fixture.send("GET", "/api/v1/repos/alice/demo/commits/main/status", Some(&olgas), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "hidden from outsiders");
        let (response, _) = fixture.send("GET", "/api/v1/repos/alice/demo/commits/main/statuses?page=x", Some(&read), None).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
