//! REST API: Releases and their attached files: `/repos/{owner}/{repo}/releases…`.
//!
//! Milestone 3 workstream A (API continuation) owns this file and `openapi/releases.json`,
//! the `OpenAPI` fragment documenting exactly these routes (merged into the served
//! document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.
//!
//! Shapes follow GitHub's (the same release object webhooks send). Everything goes
//! through [`rbit_services::ReleaseService`], so its rules hold: readers see published
//! releases, drafts exist only for writers (404 for everyone else), writers create,
//! change and delete releases and files. Scopes: `read` for every `GET`, `write` for
//! everything else — deleting a release or a file is a writer's action, like deleting a
//! label.
//!
//! Files are uploaded like on GitHub's `upload_url`: `POST …/releases/{id}/assets?name=`
//! with the file as the raw request body. That route and the download
//! (`GET …/releases/assets/{id}` with `Accept: application/octet-stream`) are
//! [streaming](super::ApiRoute::streaming): no 1 MiB body limit, no 30 s timeout, no
//! compression. The upload streams to disk under `[releases] max_asset_bytes` (413 once
//! it is exceeded, at once when `Content-Length` says so) and
//! `[releases] upload_timeout_seconds` (408). Documented deviations: no
//! `target_commitish` in responses (rbit does not record it), file `label`s are accepted
//! and ignored, and files cannot be renamed.

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::TryStreamExt;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{
    TokenScope,
    release::{ReleaseAssetId, ReleaseId},
};
use rbit_services::{
    RepoContext, ServiceError,
    releases::{ReleaseDetail, ReleaseInput, human_bytes},
    webhooks::payloads,
};
use serde::Deserialize;
use serde_json::Value;
use tokio_util::io::{ReaderStream, StreamReader};

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
    let releases = |suffix: &str| format!("/repos/{{owner}}/{{repo}}/releases{suffix}");
    vec![
        route(Get, releases(""), list),
        route(Post, releases(""), create),
        route(Get, releases("/latest"), latest),
        route(Get, releases("/tags/{*tag}"), by_tag),
        route(Get, releases("/{release_id}"), show),
        route(Patch, releases("/{release_id}"), update),
        route(Delete, releases("/{release_id}"), delete),
        route(Get, releases("/{release_id}/assets"), assets),
        route(Post, releases("/{release_id}/assets"), upload).streaming(),
        route(Get, releases("/assets/{asset_id}"), asset).streaming(),
        route(Delete, releases("/assets/{asset_id}"), delete_asset),
    ]
}

/// The path parameters of every endpoint.
#[derive(Debug, Deserialize)]
pub struct ReleasePath {
    owner: String,
    repo: String,
    release_id: Option<String>,
    asset_id: Option<String>,
    tag: Option<String>,
}

/// A positive numeric id from the path; anything else is 404.
fn parse_id(id: Option<&str>) -> ApiResult<i64> {
    id.and_then(|id| id.parse::<i64>().ok()).filter(|id| *id > 0).ok_or_else(ApiError::not_found)
}

impl ReleasePath {
    fn release(&self) -> ApiResult<ReleaseId> {
        parse_id(self.release_id.as_deref()).map(ReleaseId)
    }

    fn asset(&self) -> ApiResult<ReleaseAssetId> {
        parse_id(self.asset_id.as_deref()).map(ReleaseAssetId)
    }
}

/// The JSON of `releases` (one query for their authors and uploaders).
async fn values(state: &AppState, ctx: &RepoContext, releases: &[ReleaseDetail]) -> ApiResult<Vec<Value>> {
    let people = state.services.releases.people(releases).await?;
    let base = state.config.server.base_url();
    Ok(releases
        .iter()
        .map(|detail| {
            let author = detail.release.author.as_ref().and_then(|author| people.iter().find(|account| account.id == author.id));
            payloads::release(&detail.release, &detail.assets, author, &people, &ctx.repo, base)
        })
        .collect())
}

async fn value(state: &AppState, ctx: &RepoContext, release: &ReleaseDetail) -> ApiResult<Value> {
    Ok(values(state, ctx, std::slice::from_ref(release)).await?.pop().unwrap_or_default())
}

/// The JSON of file `id` of `release` (as it appears in the release's `assets`).
async fn asset_value(state: &AppState, ctx: &RepoContext, release: &ReleaseDetail, id: ReleaseAssetId) -> ApiResult<Value> {
    let release = value(state, ctx, release).await?;
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    assets.into_iter().find(|asset| asset["id"] == id.0).ok_or_else(ApiError::not_found)
}

/// Writers only (403 for readers), with the `write` scope.
fn require_writer(ctx: &RepoContext) -> ApiResult<()> {
    if ctx.access.can_write() { Ok(()) } else { Err(ApiError::forbidden("You need write access to this repository")) }
}

/// `201 Created` with `Location` set to the body's `url`.
fn created(body: Value) -> Response {
    let location = body["url"].as_str().unwrap_or_default().to_owned();
    (StatusCode::CREATED, [(header::LOCATION, location)], Json(body)).into_response()
}

/// Validation messages say what they are about; attribute them to the closest field.
fn field_error(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::Invalid(message) if message.contains("branch or commit") => "target_commitish",
        ServiceError::Invalid(message) if message.contains("title") => "name",
        ServiceError::Invalid(message) if message.contains("notes") => "body",
        _ => "tag_name",
    };
    ApiError::service(error, field)
}

// ------------------------------------------------------------------ releases

/// `GET /repos/{owner}/{repo}/releases`
pub async fn list(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<ReleasePath>) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u32::try_from(pager.offset()).unwrap_or(u32::MAX);
    let (releases, total) = state.services.releases.page(&ctx, limit, offset).await?;
    Ok(pager.paged(values(&state, &ctx, &releases).await?, Some(total), false))
}

/// The body of `POST` and `PATCH`; absent fields keep their value (or the default of a
/// new release).
#[derive(Debug, Default, Deserialize)]
pub struct ReleaseBody {
    tag_name: Option<String>,
    target_commitish: Option<String>,
    name: Option<String>,
    body: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
}

/// `POST /repos/{owner}/{repo}/releases`: `201 Created`. A missing tag is created at
/// `target_commitish`, by default the default branch (as on GitHub).
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ReleasePath>,
    ApiJson(body): ApiJson<ReleaseBody>,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    require_writer(&ctx)?;
    let tag_name =
        body.tag_name.filter(|tag| !tag.trim().is_empty()).ok_or_else(|| ApiError::invalid("tag_name", "tag_name is required"))?;
    let input = ReleaseInput {
        tag_name,
        name: body.name.unwrap_or_default(),
        notes: body.body.unwrap_or_default(),
        is_draft: body.draft.unwrap_or(false),
        is_prerelease: body.prerelease.unwrap_or(false),
        target: body.target_commitish.filter(|target| !target.trim().is_empty()).unwrap_or_else(|| ctx.repo.repo.default_branch.clone()),
    };
    let releases = &state.services.releases;
    let release = releases.create(&ctx, user, input).await.map_err(field_error)?;
    let detail = releases.by_id(&ctx, release.id).await?;
    Ok(created(value(&state, &ctx, &detail).await?))
}

/// `GET /repos/{owner}/{repo}/releases/latest`: full releases before pre-releases, never
/// drafts; 404 when there is none.
pub async fn latest(State(state): State<AppState>, caller: Caller, Path(path): Path<ReleasePath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let releases = &state.services.releases;
    let release = releases.latest(&ctx).await?.ok_or_else(ApiError::not_found)?;
    let detail = releases.by_id(&ctx, release.id).await?;
    Ok(Json(value(&state, &ctx, &detail).await?))
}

/// `GET /repos/{owner}/{repo}/releases/tags/{tag}`; the tag may contain `/`.
pub async fn by_tag(State(state): State<AppState>, caller: Caller, Path(path): Path<ReleasePath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let detail = state.services.releases.by_tag(&ctx, path.tag.as_deref().unwrap_or_default()).await?;
    Ok(Json(value(&state, &ctx, &detail).await?))
}

/// `GET /repos/{owner}/{repo}/releases/{release_id}`
pub async fn show(State(state): State<AppState>, caller: Caller, Path(path): Path<ReleasePath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let detail = state.services.releases.by_id(&ctx, path.release()?).await?;
    Ok(Json(value(&state, &ctx, &detail).await?))
}

/// `PATCH /repos/{owner}/{repo}/releases/{release_id}`: absent fields keep their value;
/// publishing a draft sends the `release` webhook (once).
pub async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ReleasePath>,
    ApiJson(body): ApiJson<ReleaseBody>,
) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    let releases = &state.services.releases;
    let current = releases.by_id(&ctx, path.release()?).await?.release;
    require_writer(&ctx)?;
    let input = ReleaseInput {
        tag_name: body.tag_name.unwrap_or(current.tag_name),
        name: body.name.unwrap_or(current.name),
        notes: body.body.unwrap_or(current.notes),
        is_draft: body.draft.unwrap_or(current.is_draft),
        is_prerelease: body.prerelease.unwrap_or(current.is_prerelease),
        target: String::new(),
    };
    releases.update(&ctx, user, current.id, input).await.map_err(field_error)?;
    let detail = releases.by_id(&ctx, current.id).await?;
    Ok(Json(value(&state, &ctx, &detail).await?))
}

/// `DELETE /repos/{owner}/{repo}/releases/{release_id}`: `204`; its files go too, the tag
/// stays.
pub async fn delete(State(state): State<AppState>, caller: Caller, Path(path): Path<ReleasePath>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    caller.require(TokenScope::Write)?;
    let releases = &state.services.releases;
    let release = releases.by_id(&ctx, path.release()?).await?.release;
    require_writer(&ctx)?;
    releases.delete(&ctx, release.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ attached files

/// `GET /repos/{owner}/{repo}/releases/{release_id}/assets`, by name.
pub async fn assets(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<ReleasePath>) -> ApiResult<Paged<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let detail = state.services.releases.by_id(&ctx, path.release()?).await?;
    let release = value(&state, &ctx, &detail).await?;
    Ok(pager.page_of(release["assets"].as_array().cloned().unwrap_or_default()))
}

/// The query of an upload; GitHub's `label` is accepted and ignored.
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    name: Option<String>,
}

/// The declared `Content-Length`, if any.
fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers.get(header::CONTENT_LENGTH)?.to_str().ok()?.trim().parse().ok()
}

/// `POST /repos/{owner}/{repo}/releases/{release_id}/assets?name=`: the request body is
/// the file, streamed to disk (see the module docs for the limits); `201 Created`.
pub async fn upload(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ReleasePath>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Write)?;
    let releases = &state.services.releases;
    let release = releases.by_id(&ctx, path.release()?).await?.release;
    require_writer(&ctx)?;
    let name = query.name.filter(|name| !name.is_empty()).ok_or_else(|| ApiError::invalid("name", "name is required"))?;
    let config = &state.config.releases;
    let too_large =
        || ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, format!("Release files can be at most {}", human_bytes(config.max_asset_bytes)));
    if content_length(&headers).is_some_and(|length| length > config.max_asset_bytes) {
        return Err(too_large());
    }
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).unwrap_or_default();

    // Count what arrives, to tell "too large" from other refusals.
    let received = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&received);
    let stream = body
        .into_data_stream()
        .inspect_ok(move |chunk| {
            counter.fetch_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX), Ordering::Relaxed);
        })
        .map_err(io::Error::other);
    let upload = releases.upload_asset(&ctx, user, release.id, &name, content_type, StreamReader::new(stream));
    let timeout = Duration::from_secs(config.upload_timeout_seconds);
    let Ok(uploaded) = tokio::time::timeout(timeout, upload).await else {
        return Err(ApiError::new(StatusCode::REQUEST_TIMEOUT, format!("The upload took longer than {} seconds", timeout.as_secs())));
    };
    let asset = match uploaded {
        Ok(asset) => asset,
        Err(ServiceError::Invalid(_)) if received.load(Ordering::Relaxed) > config.max_asset_bytes => return Err(too_large()),
        Err(error) => return Err(ApiError::service(error, "name")),
    };
    let (detail, asset) = releases.asset(&ctx, asset.id).await?;
    Ok(created(asset_value(&state, &ctx, &detail, asset.id).await?))
}

/// Whether `Accept` asks for the file itself.
fn wants_bytes(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|media| media.split(';').next().is_some_and(|media| media.trim().eq_ignore_ascii_case("application/octet-stream")))
}

/// `GET /repos/{owner}/{repo}/releases/assets/{asset_id}`: the file's JSON, or with
/// `Accept: application/octet-stream` the file itself (streamed, counted as a download).
pub async fn asset(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ReleasePath>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let releases = &state.services.releases;
    let id = path.asset()?;
    if !wants_bytes(&headers) {
        let (detail, _) = releases.asset(&ctx, id).await?;
        return Ok(Json(asset_value(&state, &ctx, &detail, id).await?).into_response());
    }
    let download = releases.open_asset_by_id(&ctx, id).await?;
    let asset = &download.asset;
    let disposition = format!("attachment; filename*=UTF-8''{}", utf8_percent_encode(&asset.name, NON_ALPHANUMERIC));
    // Percent-encoded names and hex digests are always valid header values.
    let header_value = |value: String| {
        HeaderValue::from_str(&value).map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "An unexpected error occurred."))
    };
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")),
        (header::CONTENT_DISPOSITION, header_value(disposition)?),
        (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'")),
        (header::CONTENT_LENGTH, HeaderValue::from(asset.size)),
        (header::ETAG, header_value(format!("\"{}\"", asset.sha256_hex()))?),
    ];
    Ok((headers, Body::from_stream(ReaderStream::new(download.file))).into_response())
}

/// `DELETE /repos/{owner}/{repo}/releases/assets/{asset_id}`: `204`.
pub async fn delete_asset(State(state): State<AppState>, caller: Caller, Path(path): Path<ReleasePath>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    caller.require(TokenScope::Write)?;
    let releases = &state.services.releases;
    let (detail, asset) = releases.asset(&ctx, path.asset()?).await?;
    require_writer(&ctx)?;
    releases.delete_asset(&ctx, detail.release.id, asset.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::{path::Path as FsPath, process::Command};

    use rbit_core::{Visibility, membership::Permission};
    use serde_json::json;

    use super::{super::test_support::Fixture, *};

    fn git(dir: &FsPath, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// Pushes `main` with one commit and the tag `v1` into `alice/{name}`.
    async fn push_history(fixture: &Fixture, owner: &rbit_core::User, name: &str) {
        let ctx = fixture.services.repositories.open(Some(owner), owner.name().as_str(), name).await.unwrap();
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join("README.md"), "hello\n").unwrap();
        git(work.path(), &["init", "--quiet", "--initial-branch=main"]);
        git(work.path(), &["add", "README.md"]);
        git(work.path(), &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "initial"]);
        git(work.path(), &["tag", "v1"]);
        git(work.path(), &["push", "--quiet", ctx.disk_path.to_str().unwrap(), "main", "v1"]);
    }

    #[test]
    fn accept_headers_select_the_bytes() {
        let accept = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT, HeaderValue::from_static(value));
            wants_bytes(&headers)
        };
        assert!(accept("application/octet-stream"));
        assert!(accept("application/json, Application/Octet-Stream;q=0.9"));
        assert!(!accept("application/json"));
        assert!(!accept("*/*"));
        assert!(!wants_bytes(&HeaderMap::new()));
    }

    #[tokio::test]
    #[expect(clippy::too_many_lines, reason = "one walk through the release endpoints")]
    async fn releases_and_files_through_the_api() {
        let fixture = Fixture::new(|config| config.releases.max_asset_bytes = 16).await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        let carol = fixture.user("carol").await;
        fixture.repo(&alice, "demo", Visibility::Public).await;
        fixture.repo(&alice, "vault", Visibility::Private).await;
        push_history(&fixture, &alice, "demo").await;
        let ctx = fixture.services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
        fixture.services.collaborators.add(&alice, &ctx, "bob", Permission::Read).await.unwrap();
        let write = fixture.token(&alice, TokenScope::Write).await;
        let read = fixture.token(&alice, TokenScope::Read).await;
        let bobs = fixture.token(&bob, TokenScope::Admin).await;
        let carols = fixture.token(&carol, TokenScope::Admin).await;
        let api = "/api/v1/repos/alice/demo/releases";

        // A release of an existing tag, and one whose tag is created from the default branch.
        let (response, v1) =
            fixture.send("POST", api, Some(&write), Some(json!({ "tag_name": "v1", "name": "One", "body": "Notes" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{v1}");
        let id = v1["id"].as_i64().unwrap();
        let url = format!("https://git.example.com{api}/{id}");
        assert_eq!((response.headers()[header::LOCATION].to_str().unwrap(), v1["url"].as_str()), (url.as_str(), Some(url.as_str())));
        assert_eq!((v1["tag_name"].as_str(), v1["name"].as_str(), v1["body"].as_str()), (Some("v1"), Some("One"), Some("Notes")));
        assert_eq!((v1["draft"].as_bool(), v1["author"]["login"].as_str()), (Some(false), Some("alice")));
        assert_eq!(v1["upload_url"], format!("{url}/assets{{?name,label}}"));
        let (response, v2) =
            fixture.send("POST", api, Some(&write), Some(json!({ "tag_name": "v2", "draft": true, "prerelease": true }))).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{v2}");
        assert_eq!((v2["name"].as_str(), v2["published_at"].clone()), (Some("v2"), Value::Null), "the title defaults to the tag");
        let draft = format!("{api}/{}", v2["id"]);

        for (body, status, field) in [
            (json!({}), StatusCode::UNPROCESSABLE_ENTITY, "tag_name"),
            (json!({ "tag_name": "v3", "target_commitish": "nope" }), StatusCode::UNPROCESSABLE_ENTITY, "target_commitish"),
            (json!({ "tag_name": "bad..tag" }), StatusCode::UNPROCESSABLE_ENTITY, "tag_name"),
            (json!({ "tag_name": "v3", "name": "x".repeat(300) }), StatusCode::UNPROCESSABLE_ENTITY, "name"),
            (json!({ "tag_name": "v1" }), StatusCode::CONFLICT, ""),
        ] {
            let (response, error) = fixture.send("POST", api, Some(&write), Some(body.clone())).await;
            assert_eq!(response.status(), status, "{body}: {error}");
            if !field.is_empty() {
                assert_eq!(error["errors"][0]["field"], field, "{body}: {error}");
            }
        }

        // Reading: drafts for writers only; latest skips drafts; by tag.
        let (response, list) = fixture.send("GET", api, Some(&write), None).await;
        assert_eq!((response.status(), response.headers()["x-total-count"].to_str().unwrap()), (StatusCode::OK, "2"));
        assert_eq!(list[0]["tag_name"], "v2", "drafts first");
        let (response, list) = fixture.send("GET", api, Some(&read), None).await;
        assert_eq!(
            (response.headers()["x-total-count"].to_str().unwrap(), list[0]["tag_name"].as_str()),
            ("1", Some("v1")),
            "a read token sees what readers see"
        );
        assert_eq!(fixture.send("GET", api, None, None).await.0.status(), StatusCode::OK, "public");
        assert_eq!(fixture.send("GET", &draft, Some(&bobs), None).await.0.status(), StatusCode::NOT_FOUND, "drafts are hidden");
        assert_eq!(fixture.send("GET", &draft, Some(&write), None).await.0.status(), StatusCode::OK);
        assert_eq!(fixture.send("GET", &format!("{api}/latest"), None, None).await.1["tag_name"], "v1");
        assert_eq!(fixture.send("GET", &format!("{api}/tags/v1"), None, None).await.1["id"], id);
        assert_eq!(fixture.send("GET", &format!("{api}/tags/v2"), None, None).await.0.status(), StatusCode::NOT_FOUND);
        for path in [format!("{api}/999"), format!("{api}/x"), "/api/v1/repos/alice/vault/releases".to_owned()] {
            assert_eq!(fixture.send("GET", &path, Some(&carols), None).await.0.status(), StatusCode::NOT_FOUND, "{path}");
        }
        assert_eq!(
            fixture.send("GET", "/api/v1/repos/alice/vault/releases/latest", Some(&write), None).await.0.status(),
            StatusCode::NOT_FOUND
        );

        // Changing: absent fields stay; publishing the draft.
        let (response, changed) = fixture.send("PATCH", &draft, Some(&write), Some(json!({ "draft": false, "body": "Now" }))).await;
        assert_eq!(response.status(), StatusCode::OK, "{changed}");
        assert_eq!(
            (changed["draft"].as_bool(), changed["prerelease"].as_bool(), changed["body"].as_str()),
            (Some(false), Some(true), Some("Now"))
        );
        assert!(changed["published_at"].is_string());
        let (response, error) = fixture.send("PATCH", &draft, Some(&write), Some(json!({ "tag_name": "v9" }))).await;
        assert_eq!((response.status(), error["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("tag_name")));
        assert_eq!(fixture.send("GET", &format!("{api}/latest"), None, None).await.1["tag_name"], "v1", "full releases first");

        // Scopes and rights.
        assert_eq!(fixture.send("POST", api, Some(&read), Some(json!({ "tag_name": "v1" }))).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("PATCH", &draft, Some(&bobs), Some(json!({}))).await.0.status(), StatusCode::FORBIDDEN, "a reader");
        assert_eq!(fixture.send("DELETE", &draft, Some(&bobs), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("POST", api, Some(&carols), Some(json!({ "tag_name": "v1" }))).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("POST", api, None, Some(json!({ "tag_name": "v1" }))).await.0.status(), StatusCode::UNAUTHORIZED);

        // Files: raw bodies, limits, conflicts.
        let upload = |name: &str| format!("{api}/{id}/assets?name={name}&label=ignored");
        let (response, file) =
            fixture.raw("POST", &upload("app.zip"), Some(&write), Some("application/zip"), Some(b"0123456789".to_vec())).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{file:?}");
        let file: Value = serde_json::from_slice(&file).unwrap();
        assert_eq!(
            (file["name"].as_str(), file["size"].as_u64(), file["content_type"].as_str()),
            (Some("app.zip"), Some(10), Some("application/zip"))
        );
        assert_eq!((file["uploader"]["login"].as_str(), file["label"].clone()), (Some("alice"), Value::Null));
        let file_url = format!("https://git.example.com{api}/assets/{}", file["id"]);
        assert_eq!(
            (file["url"].as_str(), response.headers()[header::LOCATION].to_str().unwrap()),
            (Some(file_url.as_str()), file_url.as_str())
        );
        let (response, _) = fixture.raw("POST", &upload("big.bin"), Some(&write), None, Some(vec![b'x'; 17])).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE, "Content-Length over the limit");
        let (response, _) = fixture.raw("POST", &upload("app.zip"), Some(&write), None, Some(b"x".to_vec())).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        for bad in [format!("{api}/{id}/assets"), upload(".env"), upload("a%2Fb")] {
            let (response, error) = fixture.send("POST", &bad, Some(&write), None).await;
            assert_eq!(
                (response.status(), error["errors"][0]["field"].as_str()),
                (StatusCode::UNPROCESSABLE_ENTITY, Some("name")),
                "{bad}"
            );
        }
        assert_eq!(fixture.raw("POST", &upload("x.bin"), Some(&read), None, Some(b"x".to_vec())).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.raw("POST", &upload("x.bin"), Some(&bobs), None, Some(b"x".to_vec())).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            fixture.raw("POST", &format!("{api}/999/assets?name=x"), Some(&write), None, Some(b"x".to_vec())).await.0.status(),
            StatusCode::NOT_FOUND
        );

        // Listing, metadata and the bytes.
        let (response, files) = fixture.send("GET", &format!("{api}/{id}/assets"), None, None).await;
        assert_eq!((response.status(), files.as_array().map(Vec::len)), (StatusCode::OK, Some(1)));
        let one = format!("{api}/assets/{}", file["id"]);
        let (response, metadata) = fixture.send("GET", &one, None, None).await;
        assert_eq!((response.status(), metadata["name"].as_str()), (StatusCode::OK, Some("app.zip")));
        let (response, bytes) = fixture.request("GET", &one, None, &[("accept", "application/octet-stream")], None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(&bytes[..], b"0123456789");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/octet-stream");
        assert_eq!(response.headers()[header::CONTENT_DISPOSITION], "attachment; filename*=UTF-8''app%2Ezip");
        assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(response.headers().get(header::CONTENT_ENCODING).is_none(), "streamed, not compressed");
        assert_eq!(fixture.send("GET", &one, None, None).await.1["download_count"], 1);
        assert_eq!(fixture.send("GET", &format!("{api}/assets/999"), None, None).await.0.status(), StatusCode::NOT_FOUND);

        // Deleting files and releases.
        assert_eq!(fixture.send("DELETE", &one, Some(&bobs), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("DELETE", &one, Some(&write), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("GET", &one, None, None).await.0.status(), StatusCode::NOT_FOUND);
        assert_eq!(fixture.send("DELETE", &draft, Some(&read), None).await.0.status(), StatusCode::FORBIDDEN);
        assert_eq!(fixture.send("DELETE", &draft, Some(&write), None).await.0.status(), StatusCode::NO_CONTENT);
        assert_eq!(fixture.send("GET", &draft, Some(&write), None).await.0.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn streamed_uploads_stop_at_the_limit() {
        let fixture = Fixture::new(|config| config.releases.max_asset_bytes = 1024).await;
        let alice = fixture.user("alice").await;
        fixture.repo(&alice, "demo", Visibility::Public).await;
        push_history(&fixture, &alice, "demo").await;
        let write = fixture.token(&alice, TokenScope::Write).await;
        let (_, release) = fixture.send("POST", "/api/v1/repos/alice/demo/releases", Some(&write), Some(json!({ "tag_name": "v1" }))).await;
        let path = format!("/api/v1/repos/alice/demo/releases/{}/assets?name=big.bin", release["id"]);

        // Chunked (no Content-Length): refused once more than the limit has arrived.
        let chunks = futures_util::stream::iter((0..8).map(|_| Ok::<_, io::Error>(axum::body::Bytes::from(vec![b'x'; 512]))));
        let request = axum::http::Request::builder()
            .method("POST")
            .uri(&path)
            .header(header::AUTHORIZATION, format!("Bearer {write}"))
            .body(Body::from_stream(chunks))
            .unwrap();
        let response = fixture.oneshot(request).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let (_, files) = fixture.send("GET", &format!("/api/v1/repos/alice/demo/releases/{}/assets", release["id"]), None, None).await;
        assert_eq!(files, json!([]), "nothing was kept");

        // Exactly the limit is fine.
        let (response, _) = fixture.raw("POST", &path, Some(&write), None, Some(vec![b'x'; 1024])).await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}
