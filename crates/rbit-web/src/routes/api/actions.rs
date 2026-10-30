//! REST API: rbit Actions, a GitHub-shaped subset (§4.10 of `docs/design/actions.md`):
//! runs, jobs, job logs, artifacts and workflows under `/repos/{owner}/{repo}/actions`, and
//! secrets, variables and runners of a repository, an organisation (`/orgs/{org}/actions`)
//! or the caller (`/user/actions`).
//!
//! The integration workstream (I) owns this file and `openapi/actions.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document only when
//! Actions is enabled).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`. Served only when `[actions] enabled` is on
//! ([`super::actions_routes`]); otherwise none of these paths exists.
//!
//! * Runs, jobs and workflows exist only for readers of a repository with Actions on, as
//!   the pages: 404 otherwise. Runs and jobs are named by their database ids, as on GitHub
//!   (`run_number` is the `#n` of the pages), workflows by their file name (`ci.yml`).
//! * Scopes: `read` for every `GET`; `write` and write access to cancel, re-run, approve,
//!   dispatch and delete artifacts; `admin` with repository administrator or organisation
//!   owner rights for secrets, variables, runners and registration tokens (a user manages
//!   their own). Callers who may see the repository or organisation but lack the scope or
//!   the rights get 403. Instance runners are managed on the administration pages only.
//! * A run and a job are the JSON of the `workflow_run` and `workflow_job` webhooks
//!   ([`rbit_services::actions::hooks`]), so the API and webhooks never disagree. Lists
//!   are wrapped like GitHub's (`{"total_count": n, "workflow_runs": […]}`), paginated with
//!   `page`/`per_page`, `Link` and `X-Total-Count`.
//! * Secret values are never returned. `PUT …/secrets/{name}` takes the plain `value` over
//!   TLS instead of GitHub's libsodium sealed box (`encrypted_value` and `key_id`), so there
//!   is no `…/secrets/public-key`.
//! * Secrets and variables can be managed before a repository turns Actions on.
//! * A job's log (`…/jobs/{job_id}/logs`) is the raw log itself, `text/plain`, not GitHub's
//!   redirect to it. An artifact downloads as GitHub's zip (`…/artifacts/{artifact_id}/zip`,
//!   its `archive_download_url`), made while it is sent, or as a tar.gz (`…/tar.gz`), both
//!   with the files at the top; any other format is 404.
//! * `actions/cache` entries are listed and deleted as on GitHub (`…/actions/caches`,
//!   `…/actions/cache/usage`); deleting needs `write` and write access.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{
    RepoId, Timestamp, TokenScope, User,
    actions::{
        Artifact, ArtifactId, CacheEntry, CacheEntryId, JobId, RunEvent, RunId, Runner, RunnerId, SecretScope, SecretVisibility, Status,
        Variable,
    },
};
use rbit_services::{
    ActionsService, OrgContext, RepoContext, ServiceError,
    actions::{
        browse::{RunQuery, WorkflowState},
        caches::{CacheOrder, scope_ref},
        dispatch::DispatchRequest,
        hooks,
        runners::{ManagedScope, RegistrationOptions},
        runs::Rerun,
        secrets::{ActionsTarget, ListedSecret},
    },
    webhooks::payloads::timestamp,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{
    ApiMethod::{Delete, Get, Patch, Post, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::ApiJson,
    pagination::{Paged, Pager},
    route,
};
use crate::AppState;

// Actions: settings API.
mod permissions;

/// Where secrets and variables are served: a repository, an organisation, the caller.
const SETTINGS_PREFIXES: [&str; 3] = ["/repos/{owner}/{repo}/actions", "/orgs/{org}/actions", "/user/actions"];

/// This area's endpoints, appended to [`super::table`] when Actions is enabled.
pub(super) fn routes() -> Vec<ApiRoute> {
    let repo = |suffix: &str| format!("/repos/{{owner}}/{{repo}}/actions{suffix}");
    let mut routes = vec![
        route(Get, repo("/runs"), list_runs),
        route(Get, repo("/runs/{run_id}"), show_run),
        route(Get, repo("/runs/{run_id}/jobs"), run_jobs),
        // Actions: run logs.
        route(Get, repo("/runs/{run_id}/logs"), run_logs),
        route(Post, repo("/runs/{run_id}/cancel"), cancel),
        route(Post, repo("/runs/{run_id}/rerun"), rerun),
        route(Post, repo("/runs/{run_id}/rerun-failed-jobs"), rerun_failed),
        route(Post, repo("/runs/{run_id}/approve"), approve),
        route(Get, repo("/jobs/{job_id}"), show_job),
        route(Get, repo("/jobs/{job_id}/logs"), job_logs),
        route(Post, repo("/jobs/{job_id}/rerun"), rerun_job),
        route(Get, repo("/runs/{run_id}/artifacts"), run_artifacts),
        route(Get, repo("/artifacts"), list_artifacts),
        route(Get, repo("/artifacts/{artifact_id}"), show_artifact),
        route(Delete, repo("/artifacts/{artifact_id}"), delete_artifact),
        route(Get, repo("/artifacts/{artifact_id}/{archive_format}"), download_artifact),
        // Actions: cache REST API.
        route(Get, repo("/caches"), list_caches),
        route(Delete, repo("/caches"), delete_caches_by_key),
        route(Delete, repo("/caches/{cache_id}"), delete_cache),
        route(Get, repo("/cache/usage"), cache_usage),
        route(Get, repo("/workflows"), workflows),
        // Actions: disabled workflows.
        route(Get, repo("/workflows/{workflow_id}"), show_workflow),
        route(Put, repo("/workflows/{workflow_id}/disable"), disable_workflow),
        route(Put, repo("/workflows/{workflow_id}/enable"), enable_workflow),
        route(Get, repo("/workflows/{workflow_id}/runs"), workflow_runs),
        route(Post, repo("/workflows/{workflow_id}/dispatches"), dispatch),
        // Actions: workflow_run and repository_dispatch.
        route(Post, "/repos/{owner}/{repo}/dispatches".to_owned(), repository_dispatch),
    ];
    for prefix in SETTINGS_PREFIXES {
        let path = |suffix: &str| format!("{prefix}{suffix}");
        routes.extend([
            route(Get, path("/secrets"), list_secrets),
            route(Get, path("/secrets/{name}"), show_secret),
            route(Put, path("/secrets/{name}"), put_secret),
            route(Delete, path("/secrets/{name}"), delete_secret),
            route(Get, path("/variables"), list_variables),
            route(Post, path("/variables"), create_variable),
            route(Get, path("/variables/{name}"), show_variable),
            route(Patch, path("/variables/{name}"), update_variable),
            route(Delete, path("/variables/{name}"), delete_variable),
            route(Get, path("/runners"), list_runners),
            route(Post, path("/runners/registration-token"), create_registration_token),
            route(Get, path("/runners/{runner_id}"), show_runner),
            route(Delete, path("/runners/{runner_id}"), delete_runner),
        ]);
    }
    // Actions: settings API.
    routes.extend(permissions::routes(&SETTINGS_PREFIXES));
    routes
}

/// The path parameters of every endpoint.
#[derive(Debug, Deserialize)]
pub struct ActionsPath {
    owner: Option<String>,
    repo: Option<String>,
    org: Option<String>,
    run_id: Option<String>,
    job_id: Option<String>,
    workflow_id: Option<String>,
    name: Option<String>,
    artifact_id: Option<String>,
    archive_format: Option<String>,
    runner_id: Option<String>,
    // Actions: cache REST API.
    cache_id: Option<String>,
    // Actions: settings API.
    repository_id: Option<String>,
}

/// A positive numeric id from the path; anything else is 404.
fn parse_id(id: Option<&str>) -> ApiResult<i64> {
    id.and_then(|id| id.parse::<i64>().ok()).filter(|id| *id > 0).ok_or_else(ApiError::not_found)
}

impl ActionsPath {
    fn run(&self) -> ApiResult<RunId> {
        parse_id(self.run_id.as_deref()).map(RunId)
    }

    fn job(&self) -> ApiResult<JobId> {
        parse_id(self.job_id.as_deref()).map(JobId)
    }

    fn artifact(&self) -> ApiResult<ArtifactId> {
        parse_id(self.artifact_id.as_deref()).map(ArtifactId)
    }

    // Actions: cache REST API.
    fn cache(&self) -> ApiResult<CacheEntryId> {
        parse_id(self.cache_id.as_deref()).map(CacheEntryId)
    }

    fn runner(&self) -> ApiResult<RunnerId> {
        parse_id(self.runner_id.as_deref()).map(RunnerId)
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or_default()
    }

    /// The repository as the caller sees it (404 if hidden).
    async fn repo(&self, state: &AppState, caller: &Caller) -> ApiResult<RepoContext> {
        match (&self.owner, &self.repo) {
            (Some(owner), Some(repo)) => caller.open_repo(state, owner, repo).await,
            _ => Err(ApiError::not_found()),
        }
    }
}

/// The Actions service; it exists whenever these routes do.
fn actions(state: &AppState) -> ApiResult<ActionsService> {
    state.services.actions.snapshot().ok_or_else(ApiError::not_found)
}

fn base_url(state: &AppState) -> &str {
    state.config.server.base_url()
}

/// GitHub's wrapped list: `{"total_count": n, key: […]}` with the page's `Link` and
/// `X-Total-Count`.
fn wrapped(key: &str, page: Paged<Value>) -> Response {
    let total = page.total.unwrap_or_default();
    let mut body = Map::new();
    body.insert("total_count".to_owned(), total.into());
    body.insert(key.to_owned(), Value::Array(page.items));
    let mut response = Json(Value::Object(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(HeaderName::from_static("x-total-count"), HeaderValue::from(total));
    if let Some(link) = page.link.and_then(|link| HeaderValue::from_str(&link).ok()) {
        headers.insert(header::LINK, link);
    }
    response
}

/// `{}` with `status`, as GitHub answers the actions on runs.
fn accepted(status: StatusCode) -> Response {
    (status, Json(json!({}))).into_response()
}

/// A token with the `write` scope and write access (403 otherwise).
fn require_writer<'a>(caller: &'a Caller, ctx: &RepoContext) -> ApiResult<&'a User> {
    let user = caller.require(TokenScope::Write)?;
    if ctx.access.can_write() { Ok(user) } else { Err(ApiError::forbidden("You must have write access to this repository")) }
}

/// A run that cannot do this now (already finished, not finished, not awaiting approval)
/// is a 409, as on GitHub.
fn state_error(error: ServiceError) -> ApiError {
    match error {
        ServiceError::Invalid(message) => ApiError::new(StatusCode::CONFLICT, message),
        other => other.into(),
    }
}

// ------------------------------------------------------------------ runs and jobs

/// The filters of `GET …/actions/runs`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunsQuery {
    branch: String,
    event: String,
    status: String,
    actor: String,
}

/// The run statuses a GitHub `status` filter stands for; `None` for an unknown one. Valid
/// values rbit never produces (`neutral`, `stale`, `timed_out`) stand for none.
fn github_statuses(status: &str) -> Option<&'static [Status]> {
    Some(match status {
        "queued" | "requested" | "pending" => &[Status::Waiting, Status::Blocked],
        "waiting" | "action_required" => &[Status::AwaitingApproval],
        "in_progress" => &[Status::Running],
        "completed" => &[Status::Success, Status::Failure, Status::Cancelled, Status::Skipped],
        "success" => &[Status::Success],
        "failure" => &[Status::Failure],
        "cancelled" => &[Status::Cancelled],
        "skipped" => &[Status::Skipped],
        "neutral" | "stale" | "timed_out" => &[],
        _ => return None,
    })
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// `GET /repos/{owner}/{repo}/actions/runs`: newest first.
pub async fn list_runs(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
    Query(filters): Query<RunsQuery>,
) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    runs_page(&state, &ctx, pager, &filters, None).await
}

/// A page of `ctx`'s runs matching `filters`, of workflow `workflow` when given.
async fn runs_page(
    state: &AppState,
    ctx: &RepoContext,
    pager: Pager,
    filters: &RunsQuery,
    workflow: Option<String>,
) -> ApiResult<Response> {
    let statuses = match non_empty(&filters.status) {
        Some(status) => github_statuses(&status).ok_or_else(|| ApiError::invalid("status", format!("unknown status `{status}`")))?,
        None => &[],
    };
    let event = non_empty(&filters.event);
    let matches_nothing = (!filters.status.trim().is_empty() && statuses.is_empty())
        || event.as_deref().is_some_and(|event| RunEvent::parse(event).is_none());
    let query = RunQuery {
        workflow,
        branch: non_empty(&filters.branch),
        event: event.as_deref().and_then(RunEvent::parse),
        actor: non_empty(&filters.actor),
        ..RunQuery::default()
    };
    let actions = actions(state)?;
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u32::try_from(pager.offset()).unwrap_or(u32::MAX);
    // An event or status no run can have still needs the repository to have Actions on.
    let page = actions.page_runs(ctx, &query, statuses, offset, if matches_nothing { 0 } else { limit }).await?;
    let total = if matches_nothing { 0 } else { page.total };
    let base = base_url(state);
    let items = page.runs.iter().map(|run| hooks::workflow_run(run, &ctx.repo, &page.people, base)).collect();
    Ok(wrapped("workflow_runs", pager.paged(items, Some(total), false)))
}

/// `GET /repos/{owner}/{repo}/actions/runs/{run_id}`
pub async fn show_run(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let ctx = path.repo(&state, &caller).await?;
    let detail = actions(&state)?.run_by_id(&ctx, path.run()?).await?;
    Ok(Json(hooks::workflow_run(&detail.run, &ctx.repo, &detail.people, base_url(&state))))
}

/// `GET /repos/{owner}/{repo}/actions/runs/{run_id}/jobs`: in the workflow's order.
pub async fn run_jobs(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    let detail = actions.run_by_id(&ctx, path.run()?).await?;
    let shown: Vec<_> = detail.jobs.iter().skip(pager.offset()).take(pager.limit()).cloned().collect();
    let steps = actions.steps(&shown).await?;
    let runners = actions.runner_names(&shown).await?;
    let base = base_url(&state);
    let runner = |job: &rbit_core::actions::Job| job.runner_id.and_then(|id| runners.get(&id)).map(String::as_str);
    let items = shown.iter().map(|job| hooks::workflow_job(job, &detail.run, &ctx.repo, &steps, runner(job), base)).collect();
    let total = u64::try_from(detail.jobs.len()).unwrap_or(u64::MAX);
    Ok(wrapped("jobs", pager.paged(items, Some(total), false)))
}

/// `GET /repos/{owner}/{repo}/actions/jobs/{job_id}`
pub async fn show_job(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let ctx = path.repo(&state, &caller).await?;
    let detail = actions(&state)?.job_detail(&ctx, path.job()?).await?;
    let runners = actions(&state)?.runner_names(std::slice::from_ref(&detail.job)).await?;
    let runner = detail.job.runner_id.and_then(|id| runners.get(&id)).map(String::as_str);
    Ok(Json(hooks::workflow_job(&detail.job, &detail.run, &ctx.repo, &detail.steps, runner, base_url(&state))))
}

// Actions: REST API.
/// `GET /repos/{owner}/{repo}/actions/jobs/{job_id}/logs`: the job's current attempt's raw
/// log as `text/plain`, as the job page's "Raw log"; 404 before it started or once expired.
pub async fn job_logs(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Path(path): Path<ActionsPath>,
) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    let detail = actions.job_detail(&ctx, path.job()?).await?;
    let task = detail.job.task_id.ok_or_else(ApiError::not_found)?;
    let log = actions.raw_log(&ctx, task).await?.ok_or_else(ApiError::not_found)?;
    let file_name = format!("{}-run-{}-{}.log", ctx.repo.repo.name, detail.run.number, detail.job.key);
    Ok(crate::routes::actions::jobs::raw_log_response(log, &headers, &file_name))
}

// Actions: run logs.
/// `GET /repos/{owner}/{repo}/actions/runs/{run_id}/logs`: a zip with each job's log as
/// `<n>_<job name>.txt`, sent itself rather than through GitHub's redirect.
pub async fn run_logs(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    let run = actions.run_by_id(&ctx, path.run()?).await?.run;
    let files = actions.run_log_files(&ctx, run.id).await?;
    if files.is_empty() {
        return Err(ApiError::not_found());
    }
    let mtime = u64::try_from(run.finished_at.unwrap_or(run.created_at).0).unwrap_or_default();
    // One job's log is read at a time, straight into the response.
    let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<axum::body::Bytes>>(8);
    let lease = actions.admit().map_err(|_| ApiError::not_found())?;
    let cancellation = actions.cancellation_token();
    let writer = crate::routes::actions::ChunkWriter::new(sender.clone(), cancellation.clone());
    tokio::spawn(async move {
        let _lease = lease;
        let mut zip = rbit_services::actions::downloads::ZipWriter::new(writer, mtime);
        for (name, task) in files {
            let log = match actions.raw_log(&ctx, task).await {
                Ok(Some(log)) => log,
                Ok(None) => continue,
                Err(error) => return fail_archive(&sender, error.to_string(), &cancellation).await,
            };
            let (file, gzip) = (log.file.into_std().await, log.gzip);
            let added = tokio::task::spawn_blocking(move || {
                let added = rbit_services::actions::logs::add_raw_log(&mut zip, &name, file, gzip);
                (zip, added)
            })
            .await;
            zip = match added {
                Ok((zip, Ok(()))) => zip,
                Ok((_, Err(error))) => return fail_archive(&sender, error.to_string(), &cancellation).await,
                Err(error) => return fail_archive(&sender, error.to_string(), &cancellation).await,
            };
        }
        if let Ok(Err(error)) = tokio::task::spawn_blocking(move || zip.finish().map(drop)).await {
            fail_archive(&sender, error.to_string(), &cancellation).await;
        }
    });
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move { receiver.recv().await.map(|item| (item, receiver)) });
    let name = format!("attachment; filename=\"logs_{}.zip\"", run.id.0);
    let mut response = axum::body::Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    if let Ok(value) = HeaderValue::from_str(&name) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

/// Ends a log archive being sent with `error`, which the client sees as a broken download.
async fn fail_archive(
    sender: &tokio::sync::mpsc::Sender<std::io::Result<axum::body::Bytes>>,
    error: String,
    cancellation: &tokio_util::sync::CancellationToken,
) {
    tracing::warn!(%error, "cannot send a run's log archive");
    tokio::select! {
        () = cancellation.cancelled() => {},
        _ = sender.send(Err(std::io::Error::other(error))) => {},
    }
}

/// `POST /repos/{owner}/{repo}/actions/runs/{run_id}/cancel`: `202 Accepted`; running jobs
/// stop when their runners next report.
pub async fn cancel(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    let run = actions.run_by_id(&ctx, path.run()?).await?.run;
    let user = require_writer(&caller, &ctx)?;
    actions.cancel(user, &ctx, run.id).await.map_err(state_error)?;
    Ok(accepted(StatusCode::ACCEPTED))
}

/// Re-runs `which` jobs of run `run` (found in the repository first).
async fn rerun_jobs(state: &AppState, caller: &Caller, path: &ActionsPath, run: RunId, which: Rerun) -> ApiResult<Response> {
    let ctx = path.repo(state, caller).await?;
    let actions = actions(state)?;
    let run = actions.run_by_id(&ctx, run).await?.run;
    let user = require_writer(caller, &ctx)?;
    actions.rerun(user, &ctx, run.id, which).await.map_err(state_error)?;
    Ok(accepted(StatusCode::CREATED))
}

/// `POST /repos/{owner}/{repo}/actions/runs/{run_id}/rerun`: `201 Created`; every job
/// runs again as a new attempt.
pub async fn rerun(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    rerun_jobs(&state, &caller, &path, path.run()?, Rerun::All).await
}

/// `POST /repos/{owner}/{repo}/actions/runs/{run_id}/rerun-failed-jobs`: failed and
/// cancelled jobs and the jobs that need them.
pub async fn rerun_failed(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    rerun_jobs(&state, &caller, &path, path.run()?, Rerun::Failed).await
}

/// `POST /repos/{owner}/{repo}/actions/jobs/{job_id}/rerun`: the job and the jobs that need it.
pub async fn rerun_job(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let job = actions(&state)?.job_detail(&ctx, path.job()?).await?.job;
    rerun_jobs(&state, &caller, &path, job.run_id, Rerun::Job(job.id)).await
}

/// `POST /repos/{owner}/{repo}/actions/runs/{run_id}/approve`: `201 Created`; a pull
/// request run waiting for approval starts (still read-only, without secrets).
pub async fn approve(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    let run = actions.run_by_id(&ctx, path.run()?).await?.run;
    let user = require_writer(&caller, &ctx)?;
    actions.approve(user, &ctx, run.id, false).await.map_err(state_error)?;
    Ok(accepted(StatusCode::CREATED))
}

// ------------------------------------------------------------------ workflows

/// `GET /repos/{owner}/{repo}/actions/workflows`: the workflows that have runs, by path.
pub async fn workflows(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let known = actions(&state)?.workflow_states(&ctx).await?;
    let base = base_url(&state);
    let items = known.iter().map(|workflow| workflow_json(workflow, &ctx, base)).collect();
    Ok(wrapped("workflows", pager.page_of(items)))
}

// Actions: disabled workflows.
/// A workflow, shaped like GitHub's; its id is its file name (`ci.yml`), which GitHub's
/// endpoints also take.
fn workflow_json(workflow: &WorkflowState, ctx: &RepoContext, base: &str) -> Value {
    let file = hooks::workflow_file(&workflow.path);
    let full_name = ctx.repo.full_name();
    json!({
        "id": file,
        "name": workflow.name,
        "path": workflow.path,
        "state": if workflow.disabled { "disabled_manually" } else { "active" },
        "url": format!("{base}/api/v1/repos/{full_name}/actions/workflows/{file}"),
        "html_url": format!("{base}/{full_name}/actions/workflows/{file}"),
        "badge_url": format!("{base}/{full_name}/actions/workflows/{file}/badge.svg"),
    })
}

/// `GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}`: by file name or path.
pub async fn show_workflow(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let ctx = path.repo(&state, &caller).await?;
    let workflow = actions(&state)?.workflow_state(&ctx, path.workflow_id.as_deref().unwrap_or_default()).await?;
    Ok(Json(workflow_json(&workflow, &ctx, base_url(&state))))
}

/// `PUT /repos/{owner}/{repo}/actions/workflows/{workflow_id}/disable`: `204`; it starts no
/// run until enabled.
pub async fn disable_workflow(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    switch_workflow(&state, &caller, &path, true).await
}

/// `PUT /repos/{owner}/{repo}/actions/workflows/{workflow_id}/enable`: `204`.
pub async fn enable_workflow(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    switch_workflow(&state, &caller, &path, false).await
}

async fn switch_workflow(state: &AppState, caller: &Caller, path: &ActionsPath, disabled: bool) -> ApiResult<StatusCode> {
    let ctx = path.repo(state, caller).await?;
    let actions = actions(state)?;
    let workflow = path.workflow_id.as_deref().unwrap_or_default();
    actions.workflow_state(&ctx, workflow).await?;
    let user = require_writer(caller, &ctx)?;
    actions.set_workflow_disabled(user, &ctx, workflow, disabled).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}/runs`: as `…/actions/runs`,
/// for one workflow.
pub async fn workflow_runs(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
    Query(filters): Query<RunsQuery>,
) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let workflow = actions(&state)?.workflow_state(&ctx, path.workflow_id.as_deref().unwrap_or_default()).await?;
    runs_page(&state, &ctx, pager, &filters, Some(workflow.path)).await
}

/// The body of `POST …/workflows/{workflow_id}/dispatches`.
#[derive(Debug, Deserialize)]
pub struct DispatchBody {
    #[serde(rename = "ref")]
    reference: Option<String>,
    #[serde(default)]
    inputs: BTreeMap<String, Value>,
}

/// An input as the workflow reads it: strings as given, booleans and numbers spelled out.
fn input_value(name: &str, value: &Value) -> ApiResult<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Bool(_) | Value::Number(_) => Ok(value.to_string()),
        _ => Err(ApiError::invalid("inputs", format!("the input `{name}` must be a string, a boolean or a number"))),
    }
}

/// Dispatch messages say what they are about; attribute them to that field.
fn dispatch_error(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::Invalid(message) if message.contains("input") => "inputs",
        ServiceError::Invalid(message) if message.starts_with("the ref") => "ref",
        _ => "workflow_id",
    };
    ApiError::service(error, field)
}

/// `POST /repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches`: `204`; the
/// workflow (`ci.yml` or its path) runs at `ref`, a branch or tag of the repository.
pub async fn dispatch(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<DispatchBody>,
) -> ApiResult<StatusCode> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    if !ctx.access.can_read() || !actions.enabled_for(ctx.repo.repo.id).await? {
        return Err(ApiError::not_found());
    }
    let user = require_writer(&caller, &ctx)?;
    let reference =
        body.reference.filter(|reference| !reference.trim().is_empty()).ok_or_else(|| ApiError::invalid("ref", "ref is required"))?;
    let inputs = body.inputs.iter().map(|(name, value)| Ok((name.clone(), input_value(name, value)?))).collect::<ApiResult<_>>()?;
    let request = DispatchRequest { workflow: path.workflow_id.clone().unwrap_or_default(), reference, inputs };
    actions.dispatch(user, &ctx, &request).await.map_err(dispatch_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// Actions: workflow_run and repository_dispatch.
#[derive(Debug, Deserialize)]
pub struct RepositoryDispatchBody {
    event_type: Option<String>,
    client_payload: Option<Value>,
}

/// `POST /repos/{owner}/{repo}/dispatches`: `204`; the default branch's workflows that run
/// on `repository_dispatch` of `event_type` start, with `client_payload` in their event.
pub async fn repository_dispatch(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<RepositoryDispatchBody>,
) -> ApiResult<StatusCode> {
    let ctx = path.repo(&state, &caller).await?;
    let user = require_writer(&caller, &ctx)?;
    let event_type = body.event_type.ok_or_else(|| ApiError::invalid("event_type", "event_type is required"))?;
    actions(&state)?
        .repository_dispatch(user, &ctx, &event_type, body.client_payload)
        .await
        .map_err(|error| ApiError::service(error, error_field(&event_type).unwrap_or("client_payload")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Which field a refused dispatch is about: its `event_type` when that is out of range.
fn error_field(event_type: &str) -> Option<&'static str> {
    let length = event_type.trim().chars().count();
    (length == 0 || length > rbit_services::actions::chains::MAX_DISPATCH_EVENT_TYPE_CHARS).then_some("event_type")
}

// ------------------------------------------------------------------ artifacts

// Actions: REST API.
/// An artifact, shaped like GitHub's (`size_in_bytes`, `archive_download_url`, `digest`).
fn artifact_json(artifact: &Artifact, ctx: &RepoContext, base: &str) -> Value {
    let url = format!("{base}/api/v1/repos/{}/actions/artifacts/{}", ctx.repo.full_name(), artifact.id.0);
    let digest = artifact.sha256.iter().fold(String::with_capacity(64), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    json!({
        "id": artifact.id.0,
        "name": artifact.name,
        "size_in_bytes": artifact.size,
        "url": url,
        "archive_download_url": format!("{url}/zip"),
        "expired": false,
        "digest": format!("sha256:{digest}"),
        "created_at": timestamp(artifact.created_at),
        "updated_at": timestamp(artifact.created_at),
        "expires_at": timestamp(artifact.expires_at),
        "workflow_run": { "id": artifact.run_id.0, "repository_id": artifact.repo_id.0 },
    })
}

/// `GET /repos/{owner}/{repo}/actions/runs/{run_id}/artifacts`: the newest attempt's
/// artifact of each name, by name.
pub async fn run_artifacts(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    let run = actions.run_by_id(&ctx, path.run()?).await?.run;
    let base = base_url(&state);
    let items = actions.run_artifacts(&ctx, run.id).await?.iter().map(|artifact| artifact_json(artifact, &ctx, base)).collect();
    Ok(wrapped("artifacts", pager.page_of(items)))
}

/// The filter of `GET …/actions/artifacts`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ArtifactsQuery {
    name: String,
}

/// `GET /repos/{owner}/{repo}/actions/artifacts`: newest first; `name` keeps one name.
pub async fn list_artifacts(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
    Query(filter): Query<ArtifactsQuery>,
) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u32::try_from(pager.offset()).unwrap_or(u32::MAX);
    let name = non_empty(&filter.name);
    let (artifacts, total) = actions(&state)?.repo_artifacts(&ctx, name.as_deref(), offset, limit).await?;
    let base = base_url(&state);
    let items = artifacts.iter().map(|artifact| artifact_json(artifact, &ctx, base)).collect();
    Ok(wrapped("artifacts", pager.paged(items, Some(total), false)))
}

/// `GET /repos/{owner}/{repo}/actions/artifacts/{artifact_id}`
pub async fn show_artifact(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let ctx = path.repo(&state, &caller).await?;
    let artifact = actions(&state)?.artifact(&ctx, path.artifact()?).await?;
    Ok(Json(artifact_json(&artifact, &ctx, base_url(&state))))
}

/// `GET /repos/{owner}/{repo}/actions/artifacts/{artifact_id}/{archive_format}`: the file
/// itself, as a `zip` (GitHub's format) or the `tar.gz` it is stored as.
pub async fn download_artifact(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let id = path.artifact()?;
    let format = path
        .archive_format
        .as_deref()
        .and_then(crate::routes::actions::ArtifactFormat::parse)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Artifacts download as …/zip or …/tar.gz"))?;
    let actions = actions(&state)?;
    let (artifact, file) = actions.open_artifact(&ctx, id).await?;
    Ok(crate::routes::actions::artifact_download(&actions, &artifact, file, format))
}

/// `DELETE /repos/{owner}/{repo}/actions/artifacts/{artifact_id}`: `204`; the file goes too.
pub async fn delete_artifact(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    let ctx = path.repo(&state, &caller).await?;
    let actions = actions(&state)?;
    let artifact = actions.artifact(&ctx, path.artifact()?).await?;
    let user = require_writer(&caller, &ctx)?;
    actions.delete_artifact(user, &ctx, artifact.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ caches

// Actions: cache REST API.
/// A cache entry, shaped like GitHub's.
fn cache_json(entry: &CacheEntry) -> Value {
    json!({
        "id": entry.id.0,
        "ref": scope_ref(&entry.scope),
        "key": entry.key,
        "version": entry.version,
        "last_accessed_at": timestamp(entry.used_at),
        "created_at": timestamp(entry.created_at),
        "size_in_bytes": entry.size,
    })
}

/// The filter and order of `GET …/actions/caches`, and the filter of `DELETE` by key.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CachesQuery {
    key: String,
    #[serde(rename = "ref")]
    reference: String,
    sort: String,
    direction: String,
}

impl CachesQuery {
    fn order(&self) -> ApiResult<CacheOrder> {
        let ascending = match self.direction.as_str() {
            "" | "desc" => false,
            "asc" => true,
            _ => return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "direction is asc or desc")),
        };
        Ok(match (self.sort.as_str(), ascending) {
            ("" | "last_accessed_at", false) => CacheOrder::UsedDesc,
            ("" | "last_accessed_at", true) => CacheOrder::UsedAsc,
            ("created_at", false) => CacheOrder::CreatedDesc,
            ("created_at", true) => CacheOrder::CreatedAsc,
            ("size_in_bytes", false) => CacheOrder::SizeDesc,
            ("size_in_bytes", true) => CacheOrder::SizeAsc,
            _ => return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "sort is created_at, last_accessed_at or size_in_bytes")),
        })
    }
}

/// `GET /repos/{owner}/{repo}/actions/caches`: least recently used last by default; `key`
/// keeps keys starting with it, `ref` one branch's or pull request's.
pub async fn list_caches(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
    Query(query): Query<CachesQuery>,
) -> ApiResult<Response> {
    let ctx = path.repo(&state, &caller).await?;
    let order = query.order()?;
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u32::try_from(pager.offset()).unwrap_or(u32::MAX);
    let (key, reference) = (non_empty(&query.key), non_empty(&query.reference));
    let (entries, total) = actions(&state)?.repo_caches(&ctx, key.as_deref(), reference.as_deref(), order, offset, limit).await?;
    Ok(wrapped("actions_caches", pager.paged(entries.iter().map(cache_json).collect(), Some(total), false)))
}

/// `DELETE /repos/{owner}/{repo}/actions/caches?key=…[&ref=…]`: the entries deleted.
pub async fn delete_caches_by_key(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    Query(query): Query<CachesQuery>,
) -> ApiResult<Json<Value>> {
    let ctx = path.repo(&state, &caller).await?;
    let user = require_writer(&caller, &ctx)?;
    let key = non_empty(&query.key).ok_or_else(|| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "key is required"))?;
    let reference = non_empty(&query.reference);
    let deleted = actions(&state)?.delete_caches_by_key(user, &ctx, &key, reference.as_deref()).await?;
    if deleted.is_empty() {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({ "total_count": deleted.len(), "actions_caches": deleted.iter().map(cache_json).collect::<Vec<_>>() })))
}

/// `DELETE /repos/{owner}/{repo}/actions/caches/{cache_id}`: `204`; the file goes too.
pub async fn delete_cache(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    let ctx = path.repo(&state, &caller).await?;
    let id = path.cache()?;
    let user = require_writer(&caller, &ctx)?;
    actions(&state)?.delete_cache(user, &ctx, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /repos/{owner}/{repo}/actions/cache/usage`
pub async fn cache_usage(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let ctx = path.repo(&state, &caller).await?;
    let (count, bytes) = actions(&state)?.cache_usage(&ctx).await?;
    Ok(Json(json!({
        "full_name": ctx.repo.full_name(),
        "active_caches_size_in_bytes": bytes,
        "active_caches_count": count,
    })))
}

// ------------------------------------------------------------------ runners

// Actions: REST API.
/// A runner, shaped like GitHub's (`status`, `busy`, `labels`), with what rbit adds.
fn runner_json(runner: &Runner, busy: bool, now: Timestamp) -> Value {
    let labels: Vec<Value> = runner.labels.iter().map(|label| json!({ "name": label.as_str(), "type": "custom" })).collect();
    json!({
        "id": runner.id.0,
        "name": runner.name,
        "os": runner.capabilities.os,
        "arch": runner.capabilities.arch,
        "status": if runner.is_online(now) { "online" } else { "offline" },
        "busy": busy,
        "disabled": runner.disabled,
        "ephemeral": runner.ephemeral,
        "version": runner.version,
        "labels": labels,
        "last_seen_at": runner.last_online.map_or(Value::Null, timestamp),
        "created_at": timestamp(runner.created_at),
    })
}

/// `runners` as the API shows them, with whether each is running a task.
async fn runners_json(actions: &ActionsService, runners: &[Runner]) -> ApiResult<Vec<Value>> {
    let now = Timestamp::now();
    let mut items = Vec::with_capacity(runners.len());
    for runner in runners {
        items.push(runner_json(runner, actions.runner_busy(runner.id).await?, now));
    }
    Ok(items)
}

/// `GET …/actions/runners`: the runners of exactly this repository, organisation or user,
/// oldest first.
pub async fn list_runners(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    let actions = actions(&state)?;
    let runners = actions.runners(user, owner.managed()).await?;
    let total = u64::try_from(runners.len()).unwrap_or(u64::MAX);
    let shown: Vec<_> = runners.into_iter().skip(pager.offset()).take(pager.limit()).collect();
    let items = runners_json(&actions, &shown).await?;
    Ok(wrapped("runners", pager.paged(items, Some(total), false)))
}

/// Runner `id` of exactly this repository, organisation or user.
async fn find_runner(actions: &ActionsService, user: &User, owner: &Owner, id: RunnerId) -> ApiResult<Runner> {
    let runners = actions.runners(user, owner.managed()).await?;
    runners.into_iter().find(|runner| runner.id == id).ok_or_else(ApiError::not_found)
}

/// `GET …/actions/runners/{runner_id}`
pub async fn show_runner(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    let actions = actions(&state)?;
    let runner = find_runner(&actions, user, &owner, path.runner()?).await?;
    Ok(Json(runner_json(&runner, actions.runner_busy(runner.id).await?, Timestamp::now())))
}

/// `DELETE …/actions/runners/{runner_id}`: `204`; its credential stops working at once and
/// the tasks it was running fail.
pub async fn delete_runner(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    actions(&state)?.remove_runner(user, owner.managed(), path.runner()?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST …/actions/runners/registration-token`: `201`; a single-use token for an hour, as
/// the runners pages make by default, shown this once.
pub async fn create_registration_token(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require(TokenScope::Admin)?;
    let issued = actions(&state)?.create_registration_token(user, owner.managed(), RegistrationOptions::default()).await?;
    let body = json!({ "token": issued.secret.expose(), "expires_at": timestamp(issued.token.expires_at) });
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

// ------------------------------------------------------------------ secrets and variables

/// Whose secrets and variables these are, as the caller sees it.
enum Owner {
    Repository(Box<RepoContext>),
    Organization(OrgContext),
    User(User),
}

impl Owner {
    /// Loads the repository or organisation (404 if hidden) or the caller, then checks the
    /// token's `admin` scope and the caller's rights (403).
    async fn load(state: &AppState, caller: &Caller, path: &ActionsPath) -> ApiResult<Self> {
        let owner = match (&path.owner, &path.repo, &path.org) {
            (Some(owner), Some(repo), _) => Self::Repository(Box::new(caller.open_repo(state, owner, repo).await?)),
            (_, _, Some(org)) => Self::Organization(state.services.orgs.open(caller.user(), org).await?),
            _ => Self::User(caller.require_user()?.clone()),
        };
        caller.require(TokenScope::Admin)?;
        if !owner.target().can_manage() {
            return Err(ApiError::forbidden(match owner {
                Self::Repository(_) => "You must be an administrator of this repository",
                Self::Organization(_) | Self::User(_) => "You must be an owner of this organization",
            }));
        }
        Ok(owner)
    }

    fn target(&self) -> ActionsTarget<'_> {
        match self {
            Self::Repository(ctx) => ActionsTarget::Repository(ctx),
            Self::Organization(ctx) => ActionsTarget::Organization(ctx),
            Self::User(user) => ActionsTarget::User(user),
        }
    }

    // Actions: REST API.
    /// Whose runners these are.
    fn managed(&self) -> ManagedScope<'_> {
        match self {
            Self::Repository(ctx) => ManagedScope::Repository(ctx),
            Self::Organization(ctx) => ManagedScope::Owner(&ctx.org.account),
            Self::User(user) => ManagedScope::Owner(&user.account),
        }
    }

    /// Secrets and variables of an account apply to its repositories: they have a visibility.
    fn is_account(&self) -> bool {
        matches!(self.target().scope(), SecretScope::Owner(_))
    }
}

/// Secret and variable messages say what they are about; attribute them to that field.
fn settings_error(error: ServiceError) -> ApiError {
    let field = match &error {
        ServiceError::Invalid(message) if message.contains(" name ") || message.contains("may be stored here") => "name",
        ServiceError::Invalid(message) if message.contains("visible") => "visibility",
        _ => "value",
    };
    ApiError::service(error, field)
}

/// A secret without its value.
fn secret_json(secret: &ListedSecret, account: bool) -> Value {
    let info = &secret.info;
    let mut value = json!({
        "name": info.name.as_str(),
        "created_at": timestamp(info.created_at),
        "updated_at": timestamp(info.updated_at),
    });
    if account {
        value["visibility"] = info.visibility.as_str().into();
        if info.visibility == SecretVisibility::Selected {
            value["selected_repository_ids"] = secret.repositories.iter().map(|repo| repo.0).collect::<Vec<_>>().into();
        }
    }
    value
}

fn variable_json(variable: &Variable, account: bool) -> Value {
    let mut value = json!({
        "name": variable.name.as_str(),
        "value": variable.value,
        "created_at": timestamp(variable.created_at),
        "updated_at": timestamp(variable.updated_at),
    });
    if account {
        value["visibility"] = "all".into();
    }
    value
}

/// `name` as stored (upper-cased); `None` when it cannot name anything.
fn stored_name(name: &str) -> Option<String> {
    name.parse::<rbit_core::actions::SecretName>().ok().map(|name| name.as_str().to_owned())
}

/// `GET …/actions/secrets`: names and dates, never values.
pub async fn list_secrets(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let secrets = actions(&state)?.list_secrets(owner.target()).await?;
    let items = secrets.iter().map(|secret| secret_json(secret, owner.is_account())).collect();
    Ok(wrapped("secrets", pager.page_of(items)))
}

/// Secret `name` of `owner`, if it exists.
async fn find_secret(state: &AppState, owner: &Owner, name: &str) -> ApiResult<Option<ListedSecret>> {
    let Some(name) = stored_name(name) else { return Ok(None) };
    let secrets = actions(state)?.list_secrets(owner.target()).await?;
    Ok(secrets.into_iter().find(|secret| secret.info.name.as_str() == name))
}

/// `GET …/actions/secrets/{name}`: without its value.
pub async fn show_secret(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let secret = find_secret(&state, &owner, path.name()).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(secret_json(&secret, owner.is_account())))
}

/// The body of `PUT …/actions/secrets/{name}`.
#[derive(Debug, Deserialize)]
pub struct SecretBody {
    value: Option<String>,
    /// GitHub's sealed box, refused with a message saying what to send instead.
    encrypted_value: Option<Value>,
    visibility: Option<String>,
    selected_repository_ids: Option<Vec<i64>>,
}

/// `PUT …/actions/secrets/{name}`: `201 Created` for a new secret, `204` for a new value.
/// An account's secret is visible to all its repositories or to `selected_repository_ids`.
pub async fn put_secret(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<SecretBody>,
) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    let value = match (body.value, body.encrypted_value) {
        (Some(value), _) => value,
        (None, Some(_)) => {
            return Err(ApiError::invalid(
                "value",
                "send the secret as plain `value` over TLS; sealed boxes (encrypted_value) are not supported",
            ));
        }
        (None, None) => return Err(ApiError::invalid("value", "value is required")),
    };
    let existing = find_secret(&state, &owner, path.name()).await?;
    let visibility = match body.visibility.as_deref() {
        None => existing.as_ref().map_or(SecretVisibility::All, |secret| secret.info.visibility),
        Some(given) if owner.is_account() => SecretVisibility::parse(given)
            .ok_or_else(|| ApiError::invalid("visibility", format!("visibility must be all or selected, not `{given}`")))?,
        Some(_) => return Err(ApiError::invalid("visibility", "a repository secret is visible to its repository only")),
    };
    if body.selected_repository_ids.is_some() && visibility != SecretVisibility::Selected {
        return Err(ApiError::invalid("selected_repository_ids", "selected_repository_ids needs the selected visibility"));
    }
    let actions = actions(&state)?;
    let info = actions.put_secret(user, owner.target(), path.name(), &value, visibility).await.map_err(settings_error)?;
    if let Some(ids) = &body.selected_repository_ids {
        let repos: Vec<RepoId> = ids.iter().copied().map(RepoId).collect();
        actions.set_secret_repositories(user, owner.target(), info.name.as_str(), &repos).await.map_err(settings_error)?;
    }
    Ok(if existing.is_some() { StatusCode::NO_CONTENT.into_response() } else { accepted(StatusCode::CREATED) })
}

/// `DELETE …/actions/secrets/{name}`: `204`.
pub async fn delete_secret(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    actions(&state)?.delete_secret(caller.require_user()?, owner.target(), path.name()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Variable `name` of `owner`, if it exists.
async fn find_variable(state: &AppState, owner: &Owner, name: &str) -> ApiResult<Option<Variable>> {
    let Some(name) = stored_name(name) else { return Ok(None) };
    let variables = actions(state)?.list_variables(owner.target()).await?;
    Ok(variables.into_iter().find(|variable| variable.name.as_str() == name))
}

/// `GET …/actions/variables`
pub async fn list_variables(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<ActionsPath>,
) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let variables = actions(&state)?.list_variables(owner.target()).await?;
    let items = variables.iter().map(|variable| variable_json(variable, owner.is_account())).collect();
    Ok(wrapped("variables", pager.page_of(items)))
}

/// The body of `POST …/actions/variables` and `PATCH …/actions/variables/{name}`.
#[derive(Debug, Deserialize)]
pub struct VariableBody {
    name: Option<String>,
    value: Option<String>,
    visibility: Option<String>,
}

impl VariableBody {
    /// Account variables are visible to all their repositories; nothing else is offered.
    fn check_visibility(&self) -> ApiResult<()> {
        match self.visibility.as_deref() {
            None | Some("all") => Ok(()),
            Some(_) => Err(ApiError::invalid("visibility", "variables are visible to every repository of their owner")),
        }
    }
}

/// `POST …/actions/variables`: `201 Created`; 409 when the name is taken.
pub async fn create_variable(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<VariableBody>,
) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    body.check_visibility()?;
    let name = body.name.as_deref().ok_or_else(|| ApiError::invalid("name", "name is required"))?;
    let value = body.value.as_deref().ok_or_else(|| ApiError::invalid("value", "value is required"))?;
    if find_variable(&state, &owner, name).await?.is_some() {
        return Err(ApiError::new(StatusCode::CONFLICT, "A variable with this name already exists"));
    }
    actions(&state)?.put_variable(user, owner.target(), name, value).await.map_err(settings_error)?;
    Ok(accepted(StatusCode::CREATED))
}

/// `GET …/actions/variables/{name}`
pub async fn show_variable(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let variable = find_variable(&state, &owner, path.name()).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(variable_json(&variable, owner.is_account())))
}

/// `PATCH …/actions/variables/{name}`: `204`. Variables cannot be renamed.
pub async fn update_variable(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<ActionsPath>,
    ApiJson(body): ApiJson<VariableBody>,
) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    body.check_visibility()?;
    let current = find_variable(&state, &owner, path.name()).await?.ok_or_else(ApiError::not_found)?;
    if body.name.as_deref().is_some_and(|name| stored_name(name).as_deref() != Some(current.name.as_str())) {
        return Err(ApiError::invalid("name", "variables cannot be renamed; create one with the new name and delete this one"));
    }
    let value = body.value.unwrap_or(current.value);
    actions(&state)?.put_variable(user, owner.target(), current.name.as_str(), &value).await.map_err(settings_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE …/actions/variables/{name}`: `204`.
pub async fn delete_variable(State(state): State<AppState>, caller: Caller, Path(path): Path<ActionsPath>) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    actions(&state)?.delete_variable(caller.require_user()?, owner.target(), path.name()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::{path::Path as FsPath, process::Command};

    use rbit_core::{Timestamp, Visibility, actions::Run, membership::Permission};
    use rbit_services::{CreateOrganization, CreateRepository};
    use rbit_storage::sqlite::actions_runs::{NewJob, NewRun, RunStart};

    use super::{super::test_support::Fixture, *};

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const CI: &str = ".forgejo/workflows/ci.yml";

    async fn fixture() -> Fixture {
        Fixture::new(|config| config.actions.enabled = true).await
    }

    /// The id of `owner/name`, after turning Actions on for it when `on`.
    async fn repo_id(fixture: &Fixture, owner: &str, name: &str, on: bool) -> RepoId {
        let id = fixture.database.repositories().find_by_path(owner, name).await.unwrap().unwrap().repo.id;
        if on {
            fixture.database.actions_settings().set_enabled(id, true, Timestamp::now()).await.unwrap();
        }
        id
    }

    /// What a test run looks like: where it ran, why and for whom.
    struct Spec<'a> {
        workflow: &'a str,
        event: RunEvent,
        reference: &'a str,
        actor: Option<&'a User>,
        start: RunStart,
    }

    impl<'a> Spec<'a> {
        fn push(reference: &'a str, actor: &'a User) -> Self {
            Self { workflow: CI, event: RunEvent::Push, reference, actor: Some(actor), start: RunStart::Runnable }
        }
    }

    /// A run with jobs `build` and `test` (needing `build`).
    async fn run(fixture: &Fixture, repo: RepoId, spec: Spec<'_>) -> Run {
        let (needs, labels) = (vec!["build".to_owned()], vec!["linux".to_owned()]);
        let jobs = [
            NewJob {
                key: "build",
                name: "build",
                matrix: None,
                needs: &[],
                runs_on: &labels,
                permissions: None,
                payload: b"{}",
                condition: false,
                max_parallel: None,
                group: None,
                environment: None,
            },
            NewJob {
                key: "test",
                name: "test",
                matrix: None,
                needs: &needs,
                runs_on: &labels,
                permissions: None,
                payload: b"{}",
                condition: false,
                max_parallel: None,
                group: None,
                environment: None,
            },
        ];
        let actor = spec.actor.map(User::id);
        let new = NewRun {
            repo,
            workflow_path: spec.workflow,
            workflow_name: "CI",
            title: "Fix the build",
            event: spec.event,
            reference: spec.reference,
            sha: SHA,
            actor,
            trigger_actor: actor,
            pull_number: None,
            is_fork: false,
            trusted: spec.start != RunStart::AwaitingApproval,
            concurrency_group: None,
            cancel_in_progress: false,
            event_payload: "{}",
            dispatch_inputs: None,
            start: spec.start,
            jobs: &jobs,
            error: None,
            now: Timestamp::now(),
        };
        fixture.database.actions_runs().create_run(&new).await.unwrap()
    }

    fn ids(json: &Value, key: &str) -> Vec<i64> {
        json[key].as_array().unwrap().iter().map(|item| item["id"].as_i64().unwrap()).collect()
    }

    fn field(json: &Value) -> Option<&str> {
        json["errors"][0]["field"].as_str()
    }

    #[tokio::test]
    #[expect(clippy::too_many_lines, reason = "one repository's runs, read every way")]
    async fn runs_jobs_and_workflows_are_read_with_filters_and_pages() {
        let fixture = fixture().await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        fixture.repo(&alice, "rocket", Visibility::Public).await;
        fixture.repo(&alice, "vault", Visibility::Private).await;
        fixture.repo(&alice, "quiet", Visibility::Public).await;
        let rocket = repo_id(&fixture, "alice", "rocket", true).await;
        let vault = repo_id(&fixture, "alice", "vault", true).await;
        let quiet = repo_id(&fixture, "alice", "quiet", false).await;
        let first = run(&fixture, rocket, Spec::push("refs/heads/main", &alice)).await;
        let scheduled = Spec {
            workflow: ".forgejo/workflows/nightly.yml",
            event: RunEvent::Schedule,
            reference: "refs/heads/main",
            actor: None,
            start: RunStart::Runnable,
        };
        let nightly = run(&fixture, rocket, scheduled).await;
        let third = run(&fixture, rocket, Spec::push("refs/heads/dev", &bob)).await;
        fixture.database.actions_runs().cancel_run(rocket, first.id, Timestamp::now()).await.unwrap();
        let hidden = run(&fixture, vault, Spec::push("refs/heads/main", &alice)).await;
        run(&fixture, quiet, Spec::push("refs/heads/main", &alice)).await;
        let alices = fixture.token(&alice, TokenScope::Read).await;
        let bobs = fixture.token(&bob, TokenScope::Admin).await;
        let runs = "/api/v1/repos/alice/rocket/actions/runs";

        let (response, json) = fixture.send("GET", runs, None, None).await;
        assert_eq!(response.status(), StatusCode::OK, "{json}");
        assert_eq!((json["total_count"].as_u64(), ids(&json, "workflow_runs")), (Some(3), vec![third.id.0, nightly.id.0, first.id.0]));
        assert_eq!(response.headers()["x-total-count"], "3");
        let cancelled = &json["workflow_runs"][2];
        assert_eq!((cancelled["status"].as_str(), cancelled["conclusion"].as_str()), (Some("completed"), Some("cancelled")));
        assert_eq!((cancelled["run_number"].as_i64(), cancelled["workflow_id"].as_str()), (Some(1), Some("ci.yml")));
        assert_eq!((cancelled["actor"]["login"].as_str(), cancelled["head_branch"].as_str()), (Some("alice"), Some("main")));
        let api = "https://git.example.com/api/v1/repos/alice/rocket/actions";
        assert_eq!(cancelled["url"], format!("{api}/runs/{}", first.id.0));
        assert_eq!(cancelled["jobs_url"], format!("{api}/runs/{}/jobs", first.id.0));
        assert_eq!(cancelled["html_url"], "https://git.example.com/alice/rocket/actions/runs/1");
        let unattended = &json["workflow_runs"][1];
        assert_eq!((unattended["actor"].clone(), unattended["status"].as_str()), (Value::Null, Some("queued")));

        // Pages keep the filters in their links; the total counts every match.
        let (response, json) = fixture.send("GET", &format!("{runs}?event=push&per_page=1"), None, None).await;
        assert_eq!((json["total_count"].as_u64(), ids(&json, "workflow_runs")), (Some(2), vec![third.id.0]));
        let link = response.headers()[header::LINK].to_str().unwrap();
        assert!(link.contains("actions/runs?event=push&per_page=1&page=2>; rel=\"next\""), "{link}");
        let (_, json) = fixture.send("GET", &format!("{runs}?event=push&per_page=1&page=2"), None, None).await;
        assert_eq!(ids(&json, "workflow_runs"), [first.id.0]);
        for (filter, expected) in [
            ("event=schedule", vec![nightly.id.0]),
            ("event=release", vec![]),
            ("branch=dev", vec![third.id.0]),
            ("actor=bob", vec![third.id.0]),
            ("actor=nobody", vec![]),
            ("status=completed", vec![first.id.0]),
            ("status=cancelled", vec![first.id.0]),
            ("status=queued", vec![third.id.0, nightly.id.0]),
            ("status=in_progress", vec![]),
            ("status=timed_out", vec![]),
            ("branch=main&status=queued", vec![nightly.id.0]),
        ] {
            let (response, json) = fixture.send("GET", &format!("{runs}?{filter}"), None, None).await;
            assert_eq!(response.status(), StatusCode::OK, "{filter}");
            assert_eq!(ids(&json, "workflow_runs"), expected, "{filter}");
            assert_eq!(json["total_count"].as_u64(), u64::try_from(expected.len()).ok(), "{filter}");
        }
        let (response, json) = fixture.send("GET", &format!("{runs}?status=done"), None, None).await;
        assert_eq!((response.status(), field(&json)), (StatusCode::UNPROCESSABLE_ENTITY, Some("status")));

        // One run, its jobs, one job; ids of another repository are unknown here.
        let (response, json) = fixture.send("GET", &format!("{runs}/{}", nightly.id.0), None, None).await;
        assert_eq!((response.status(), json["id"].as_i64()), (StatusCode::OK, Some(nightly.id.0)));
        assert_eq!((json["path"].as_str(), json["triggering_actor"].clone()), (Some(".forgejo/workflows/nightly.yml"), Value::Null));
        let (response, json) = fixture.send("GET", &format!("{runs}/{}/jobs", third.id.0), None, None).await;
        assert_eq!((response.status(), json["total_count"].as_u64()), (StatusCode::OK, Some(2)));
        let names: Vec<_> = json["jobs"].as_array().unwrap().iter().map(|job| job["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["build", "test"]);
        let build = &json["jobs"][0];
        assert_eq!(
            (build["status"].as_str(), build["conclusion"].clone(), build["steps"].clone()),
            (Some("queued"), Value::Null, json!([]))
        );
        assert_eq!((build["run_id"].as_i64(), build["labels"].clone()), (Some(third.id.0), json!(["linux"])));
        let job_id = build["id"].as_i64().unwrap();
        let (response, json) = fixture.send("GET", &format!("{runs}/{}/jobs?per_page=1&page=2", third.id.0), None, None).await;
        assert_eq!((json["jobs"][0]["name"].as_str(), json["total_count"].as_u64()), (Some("test"), Some(2)));
        assert!(response.headers()[header::LINK].to_str().unwrap().contains("rel=\"prev\""));
        let (response, json) = fixture.send("GET", &format!("/api/v1/repos/alice/rocket/actions/jobs/{job_id}"), None, None).await;
        assert_eq!((response.status(), json["name"].as_str()), (StatusCode::OK, Some("build")));
        assert_eq!(json["url"], format!("{api}/jobs/{job_id}"));
        assert_eq!(json["html_url"], format!("https://git.example.com/alice/rocket/actions/runs/3/jobs/{job_id}"));
        let hidden_job = fixture.database.actions_runs().jobs(hidden.id).await.unwrap()[0].id.0;
        for path in [
            format!("{runs}/{}", hidden.id.0),
            format!("{runs}/{}/jobs", hidden.id.0),
            format!("{runs}/x"),
            format!("{runs}/0"),
            format!("/api/v1/repos/alice/rocket/actions/jobs/{hidden_job}"),
            "/api/v1/repos/alice/rocket/actions/jobs/x".to_owned(),
        ] {
            let (response, _) = fixture.send("GET", &path, None, None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }

        let (response, json) = fixture.send("GET", "/api/v1/repos/alice/rocket/actions/workflows", None, None).await;
        assert_eq!((response.status(), json["total_count"].as_u64()), (StatusCode::OK, Some(2)));
        assert_eq!(
            json["workflows"][0],
            json!({
                "id": "ci.yml",
                "name": "CI",
                "path": CI,
                "state": "active",
                "url": "https://git.example.com/api/v1/repos/alice/rocket/actions/workflows/ci.yml",
                "html_url": "https://git.example.com/alice/rocket/actions/workflows/ci.yml",
                "badge_url": "https://git.example.com/alice/rocket/actions/workflows/ci.yml/badge.svg",
            })
        );
        assert_eq!(json["workflows"][1]["id"], "nightly.yml");

        // A private repository is 404 to outsiders, whatever their scope; a repository with
        // Actions off is 404 to everyone.
        let vault_paths = [
            "/api/v1/repos/alice/vault/actions/runs".to_owned(),
            format!("/api/v1/repos/alice/vault/actions/runs/{}", hidden.id.0),
            format!("/api/v1/repos/alice/vault/actions/runs/{}/jobs", hidden.id.0),
            format!("/api/v1/repos/alice/vault/actions/jobs/{hidden_job}"),
            "/api/v1/repos/alice/vault/actions/workflows".to_owned(),
        ];
        for path in &vault_paths {
            for token in [None, Some(bobs.as_str())] {
                let (response, json) = fixture.send("GET", path, token, None).await;
                assert_eq!((response.status(), json["message"].as_str()), (StatusCode::NOT_FOUND, Some("Not Found")), "{path}");
            }
            let (response, _) = fixture.send("GET", path, Some(&alices), None).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
        }
        for path in [
            "/api/v1/repos/alice/quiet/actions/runs",
            "/api/v1/repos/alice/quiet/actions/workflows",
            "/api/v1/repos/alice/quiet/actions/runs?status=timed_out",
        ] {
            let (response, _) = fixture.send("GET", path, Some(&alices), None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn writers_cancel_rerun_and_approve_runs() {
        let fixture = fixture().await;
        let alice = fixture.user("alice").await;
        let rita = fixture.user("rita").await;
        let olga = fixture.user("olga").await;
        fixture.repo(&alice, "rocket", Visibility::Public).await;
        fixture.repo(&alice, "vault", Visibility::Private).await;
        fixture.repo(&alice, "quiet", Visibility::Public).await;
        let ctx = fixture.services.repositories.open(Some(&alice), "alice", "rocket").await.unwrap();
        fixture.services.collaborators.add(&alice, &ctx, "rita", Permission::Read).await.unwrap();
        let rocket = repo_id(&fixture, "alice", "rocket", true).await;
        let vault = repo_id(&fixture, "alice", "vault", true).await;
        let quiet = repo_id(&fixture, "alice", "quiet", false).await;
        let pushed = run(&fixture, rocket, Spec::push("refs/heads/main", &alice)).await;
        let write = fixture.token(&alice, TokenScope::Write).await;
        let read = fixture.token(&alice, TokenScope::Read).await;
        let ritas = fixture.token(&rita, TokenScope::Write).await;
        let olgas = fixture.token(&olga, TokenScope::Admin).await;
        let path = |suffix: &str| format!("/api/v1/repos/alice/rocket/actions/runs/{}{suffix}", pushed.id.0);
        let status = |json: &Value| (json["status"].as_str().map(str::to_owned), json["conclusion"].as_str().map(str::to_owned));

        for (token, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some(&read), StatusCode::FORBIDDEN),
            (Some(&ritas), StatusCode::FORBIDDEN),
            (Some(&olgas), StatusCode::FORBIDDEN),
        ] {
            for suffix in ["/cancel", "/rerun", "/rerun-failed-jobs", "/approve"] {
                let (response, json) = fixture.send("POST", &path(suffix), token.map(String::as_str), None).await;
                assert_eq!(response.status(), expected, "{suffix}: {json}");
            }
        }
        let (response, json) = fixture.send("POST", &path("/rerun"), Some(&write), None).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "only finished runs run again: {json}");
        let (response, json) = fixture.send("POST", &path("/approve"), Some(&write), None).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{json}");
        let (response, json) = fixture.send("POST", &path("/cancel"), Some(&write), None).await;
        assert_eq!((response.status(), json), (StatusCode::ACCEPTED, json!({})));
        let (response, _) = fixture.send("POST", &path("/cancel"), Some(&write), None).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "already finished");
        let (_, json) = fixture.send("GET", &path(""), None, None).await;
        assert_eq!(status(&json), (Some("completed".into()), Some("cancelled".into())));

        let (response, json) = fixture.send("POST", &path("/rerun-failed-jobs"), Some(&write), None).await;
        assert_eq!((response.status(), json), (StatusCode::CREATED, json!({})), "cancelled jobs count as failed");
        let (_, json) = fixture.send("GET", &path(""), None, None).await;
        assert_eq!((status(&json), json["run_attempt"].as_i64()), ((Some("queued".into()), None), Some(2)));
        fixture.send("POST", &path("/cancel"), Some(&write), None).await;
        let (response, _) = fixture.send("POST", &path("/rerun"), Some(&write), None).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        fixture.send("POST", &path("/cancel"), Some(&write), None).await;
        let jobs = fixture.database.actions_runs().jobs(pushed.id).await.unwrap();
        let test_job = format!("/api/v1/repos/alice/rocket/actions/jobs/{}/rerun", jobs[1].id.0);
        let (response, _) = fixture.send("POST", &test_job, Some(&ritas), None).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let (response, json) = fixture.send("POST", &test_job, Some(&write), None).await;
        assert_eq!((response.status(), json), (StatusCode::CREATED, json!({})));
        let jobs = fixture.database.actions_runs().jobs(pushed.id).await.unwrap();
        assert_eq!((jobs[0].status, jobs[1].status), (Status::Cancelled, Status::Waiting), "only the test job runs again");

        // Approval starts a run that waits for it, once.
        let pull = Spec {
            workflow: CI,
            event: RunEvent::PullRequest,
            reference: "refs/pull/7/head",
            actor: Some(&olga),
            start: RunStart::AwaitingApproval,
        };
        let waiting = run(&fixture, rocket, pull).await;
        let approve = format!("/api/v1/repos/alice/rocket/actions/runs/{}/approve", waiting.id.0);
        let shown = format!("/api/v1/repos/alice/rocket/actions/runs/{}", waiting.id.0);
        let (_, json) = fixture.send("GET", &shown, None, None).await;
        assert_eq!(status(&json), (Some("waiting".into()), None));
        let (response, _) = fixture.send("POST", &approve, Some(&ritas), None).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let (response, json) = fixture.send("POST", &approve, Some(&write), None).await;
        assert_eq!((response.status(), json), (StatusCode::CREATED, json!({})));
        let (response, _) = fixture.send("POST", &approve, Some(&write), None).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let (_, json) = fixture.send("GET", &shown, None, None).await;
        assert_eq!(status(&json), (Some("queued".into()), None));

        // Hidden repositories, repositories with Actions off and runs of other repositories are 404.
        let hidden = run(&fixture, vault, Spec::push("refs/heads/main", &alice)).await;
        let off = run(&fixture, quiet, Spec::push("refs/heads/main", &alice)).await;
        for (path, token) in [
            (format!("/api/v1/repos/alice/vault/actions/runs/{}/cancel", hidden.id.0), &olgas),
            (format!("/api/v1/repos/alice/quiet/actions/runs/{}/cancel", off.id.0), &write),
            (format!("/api/v1/repos/alice/rocket/actions/runs/{}/cancel", hidden.id.0), &write),
            (format!("/api/v1/repos/alice/rocket/actions/runs/{}/rerun", hidden.id.0), &write),
            ("/api/v1/repos/alice/rocket/actions/jobs/999999/rerun".to_owned(), &write),
        ] {
            let (response, _) = fixture.send("POST", &path, Some(token), None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
        assert_eq!(fixture.database.actions_runs().run(off.id).await.unwrap().unwrap().status, Status::Waiting, "nothing was cancelled");
    }

    fn git(dir: &FsPath, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    }

    const MANUAL: &str = "name: Manual
on:
  workflow_dispatch:
    inputs:
      level:
        type: choice
        options: [low, high]
        required: true
      debug:
        type: boolean
jobs:
  a:
    runs-on: linux
    steps:
      - run: echo hi
";

    #[tokio::test]
    async fn workflows_are_dispatched_at_a_ref_with_checked_inputs() {
        let fixture = fixture().await;
        let alice = fixture.user("alice").await;
        let rita = fixture.user("rita").await;
        fixture.repo(&alice, "rocket", Visibility::Public).await;
        fixture.repo(&alice, "quiet", Visibility::Public).await;
        let ctx = fixture.services.repositories.open(Some(&alice), "alice", "rocket").await.unwrap();
        fixture.services.collaborators.add(&alice, &ctx, "rita", Permission::Read).await.unwrap();
        let quiet = fixture.services.repositories.open(Some(&alice), "alice", "quiet").await.unwrap();
        let work = tempfile::tempdir().unwrap();
        git(work.path(), &["init", "-q"]);
        std::fs::create_dir_all(work.path().join(".forgejo/workflows")).unwrap();
        std::fs::write(work.path().join(".forgejo/workflows/manual.yml"), MANUAL).unwrap();
        git(work.path(), &["add", "."]);
        git(work.path(), &["commit", "-q", "-m", "CI"]);
        git(work.path(), &["push", "-q", ctx.disk_path.to_str().unwrap(), "main"]);
        git(work.path(), &["push", "-q", quiet.disk_path.to_str().unwrap(), "main"]);
        repo_id(&fixture, "alice", "rocket", true).await;
        let write = fixture.token(&alice, TokenScope::Write).await;
        let read = fixture.token(&alice, TokenScope::Read).await;
        let ritas = fixture.token(&rita, TokenScope::Write).await;
        let dispatch = "/api/v1/repos/alice/rocket/actions/workflows/manual.yml/dispatches";
        let body = json!({ "ref": "main", "inputs": { "level": "high", "debug": true } });

        for (token, expected) in
            [(None, StatusCode::UNAUTHORIZED), (Some(&read), StatusCode::FORBIDDEN), (Some(&ritas), StatusCode::FORBIDDEN)]
        {
            let (response, json) = fixture.send("POST", dispatch, token.map(String::as_str), Some(body.clone())).await;
            assert_eq!(response.status(), expected, "{json}");
        }
        let (response, json) = fixture.send("POST", dispatch, Some(&write), Some(body)).await;
        assert_eq!((response.status(), json), (StatusCode::NO_CONTENT, Value::Null));
        let (_, json) = fixture.send("GET", "/api/v1/repos/alice/rocket/actions/runs", None, None).await;
        let run = &json["workflow_runs"][0];
        assert_eq!(
            (json["total_count"].as_u64(), run["event"].as_str(), run["name"].as_str()),
            (Some(1), Some("workflow_dispatch"), Some("Manual"))
        );
        assert_eq!((run["head_branch"].as_str(), run["actor"]["login"].as_str()), (Some("main"), Some("alice")));
        let (_, json) = fixture.send("GET", "/api/v1/repos/alice/rocket/actions/workflows", None, None).await;
        assert_eq!(json["workflows"][0]["id"], "manual.yml");

        let other = "/api/v1/repos/alice/rocket/actions/workflows/other.yml/dispatches";
        let readme = "/api/v1/repos/alice/rocket/actions/workflows/README.md/dispatches";
        for (path, body, expected) in [
            (dispatch, json!({ "ref": "nope", "inputs": { "level": "low" } }), "ref"),
            (dispatch, json!({ "ref": "refs/pull/1/head", "inputs": { "level": "low" } }), "ref"),
            (dispatch, json!({ "inputs": { "level": "low" } }), "ref"),
            (dispatch, json!({ "ref": "main" }), "inputs"),
            (dispatch, json!({ "ref": "main", "inputs": { "level": "medium" } }), "inputs"),
            (dispatch, json!({ "ref": "main", "inputs": { "level": "low", "other": "x" } }), "inputs"),
            (dispatch, json!({ "ref": "main", "inputs": { "level": ["low"] } }), "inputs"),
            (other, json!({ "ref": "main" }), "workflow_id"),
            (readme, json!({ "ref": "main" }), "workflow_id"),
        ] {
            let (response, json) = fixture.send("POST", path, Some(&write), Some(body.clone())).await;
            assert_eq!((response.status(), field(&json)), (StatusCode::UNPROCESSABLE_ENTITY, Some(expected)), "{body}: {json}");
        }
        let quiet = "/api/v1/repos/alice/quiet/actions/workflows/manual.yml/dispatches";
        let (response, _) = fixture.send("POST", quiet, Some(&write), Some(json!({ "ref": "main", "inputs": { "level": "low" } }))).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "Actions is off there");
    }

    #[tokio::test]
    async fn secrets_and_variables_need_admin_and_never_show_values() {
        let fixture = fixture().await;
        let alice = fixture.user("alice").await;
        let rita = fixture.user("rita").await;
        let bob = fixture.user("bob").await;
        fixture.repo(&alice, "rocket", Visibility::Public).await;
        fixture.repo(&alice, "vault", Visibility::Private).await;
        let ctx = fixture.services.repositories.open(Some(&alice), "alice", "rocket").await.unwrap();
        fixture.services.collaborators.add(&alice, &ctx, "rita", Permission::Write).await.unwrap();
        let admin = fixture.token(&alice, TokenScope::Admin).await;
        let write = fixture.token(&alice, TokenScope::Write).await;
        let ritas = fixture.token(&rita, TokenScope::Admin).await;
        let bobs = fixture.token(&bob, TokenScope::Admin).await;
        let secret = "/api/v1/repos/alice/rocket/actions/secrets/deploy_key";
        let value = json!({ "value": "s3cr3t-value" });

        for (path, token, expected) in [
            (secret, None, StatusCode::UNAUTHORIZED),
            (secret, Some(&write), StatusCode::FORBIDDEN),
            (secret, Some(&ritas), StatusCode::FORBIDDEN),
            (secret, Some(&bobs), StatusCode::FORBIDDEN),
            ("/api/v1/repos/alice/vault/actions/secrets/deploy_key", Some(&bobs), StatusCode::NOT_FOUND),
            ("/api/v1/repos/alice/nope/actions/secrets/deploy_key", Some(&admin), StatusCode::NOT_FOUND),
        ] {
            let (response, json) = fixture.send("PUT", path, token.map(String::as_str), Some(value.clone())).await;
            assert_eq!(response.status(), expected, "{path}: {json}");
        }
        let (response, json) = fixture.send("PUT", secret, Some(&admin), Some(value.clone())).await;
        assert_eq!((response.status(), json), (StatusCode::CREATED, json!({})));
        let (response, json) = fixture.send("PUT", secret, Some(&admin), Some(json!({ "value": "s3cr3t-value-2" }))).await;
        assert_eq!((response.status(), json), (StatusCode::NO_CONTENT, Value::Null));
        let (response, bytes) = fixture.raw("GET", "/api/v1/repos/alice/rocket/actions/secrets", Some(&admin), None, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!String::from_utf8_lossy(&bytes).contains("s3cr3t"), "values are never returned");
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["total_count"], 1);
        let listed = json["secrets"][0].as_object().unwrap();
        assert_eq!(listed.keys().map(String::as_str).collect::<Vec<_>>(), ["created_at", "name", "updated_at"]);
        assert_eq!(listed["name"], "DEPLOY_KEY");
        let (response, bytes) = fixture.raw("GET", secret, Some(&admin), None, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!String::from_utf8_lossy(&bytes).contains("s3cr3t"));
        for (path, token) in [(secret, &write), ("/api/v1/repos/alice/rocket/actions/secrets", &ritas)] {
            let (response, _) = fixture.send("GET", path, Some(token), None).await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        }
        for (body, expected) in [
            (json!({ "encrypted_value": "c2VhbGVk", "key_id": "1" }), "value"),
            (json!({}), "value"),
            (json!({ "value": "" }), "value"),
            (json!({ "value": "x", "visibility": "selected" }), "visibility"),
            (json!({ "value": "x", "selected_repository_ids": [1] }), "selected_repository_ids"),
        ] {
            let (response, json) = fixture.send("PUT", secret, Some(&admin), Some(body.clone())).await;
            assert_eq!((response.status(), field(&json)), (StatusCode::UNPROCESSABLE_ENTITY, Some(expected)), "{body}: {json}");
        }
        let reserved = "/api/v1/repos/alice/rocket/actions/secrets/GITHUB_TOKEN";
        let (response, json) = fixture.send("PUT", reserved, Some(&admin), Some(value.clone())).await;
        assert_eq!((response.status(), field(&json)), (StatusCode::UNPROCESSABLE_ENTITY, Some("name")));
        let (response, _) = fixture.send("DELETE", secret, Some(&admin), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        for method in ["GET", "DELETE"] {
            let (response, _) = fixture.send(method, secret, Some(&admin), None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
        }
    }

    #[tokio::test]
    async fn variables_are_created_once_changed_and_never_renamed() {
        let fixture = fixture().await;
        let alice = fixture.user("alice").await;
        fixture.repo(&alice, "rocket", Visibility::Public).await;
        let admin = fixture.token(&alice, TokenScope::Admin).await;
        let write = fixture.token(&alice, TokenScope::Write).await;
        let variables = "/api/v1/repos/alice/rocket/actions/variables";
        let region = json!({ "name": "region", "value": "eu" });
        let (response, _) = fixture.send("POST", variables, Some(&write), Some(region.clone())).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let (response, json) = fixture.send("POST", variables, Some(&admin), Some(region.clone())).await;
        assert_eq!((response.status(), json), (StatusCode::CREATED, json!({})));
        let (response, _) = fixture.send("POST", variables, Some(&admin), Some(region)).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let (_, json) = fixture.send("GET", variables, Some(&admin), None).await;
        let listed = &json["variables"][0];
        assert_eq!(
            (json["total_count"].as_u64(), listed["name"].as_str(), listed["value"].as_str()),
            (Some(1), Some("REGION"), Some("eu"))
        );
        assert!(listed.get("visibility").is_none());
        let one = format!("{variables}/Region");
        let (response, _) = fixture.send("PATCH", &one, Some(&admin), Some(json!({ "value": "us" }))).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (response, _) = fixture.send("PATCH", &one, Some(&admin), Some(json!({ "name": "REGION", "value": "ap" }))).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (_, json) = fixture.send("GET", &one, Some(&admin), None).await;
        assert_eq!((json["name"].as_str(), json["value"].as_str()), (Some("REGION"), Some("ap")));
        for (method, path, body, expected) in [
            ("PATCH", one.as_str(), json!({ "name": "zone" }), "name"),
            ("POST", variables, json!({ "name": "zone" }), "value"),
            ("POST", variables, json!({ "value": "x" }), "name"),
            ("POST", variables, json!({ "name": "9lives", "value": "x" }), "name"),
            ("POST", variables, json!({ "name": "zone", "value": "x", "visibility": "selected" }), "visibility"),
        ] {
            let (response, json) = fixture.send(method, path, Some(&admin), Some(body.clone())).await;
            assert_eq!((response.status(), field(&json)), (StatusCode::UNPROCESSABLE_ENTITY, Some(expected)), "{body}: {json}");
        }
        let (response, _) = fixture.send("DELETE", &one, Some(&admin), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        for (method, body) in [("GET", None), ("PATCH", Some(json!({ "value": "x" }))), ("DELETE", None)] {
            let (response, _) = fixture.send(method, &one, Some(&admin), body).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
        }
    }

    #[tokio::test]
    async fn organisations_and_users_have_secrets_and_variables_of_their_own() {
        let fixture = fixture().await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        fixture.repo(&alice, "rocket", Visibility::Public).await;
        let input = CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() };
        fixture.services.orgs.create(&alice, input).await.unwrap();
        let site = CreateRepository { name: "site".into(), description: String::new(), visibility: Visibility::Private };
        let site = fixture.services.repositories.create_in(&alice, "acme", site).await.unwrap().repo.id;
        let rocket = repo_id(&fixture, "alice", "rocket", false).await;
        let admin = fixture.token(&alice, TokenScope::Admin).await;
        let write = fixture.token(&alice, TokenScope::Write).await;
        let bobs = fixture.token(&bob, TokenScope::Admin).await;
        let org = "/api/v1/orgs/acme/actions";
        let npm = format!("{org}/secrets/npm_token");

        let (response, _) = fixture.send("GET", &format!("{org}/secrets"), Some(&bobs), None).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "not an owner");
        let (response, _) = fixture.send("GET", "/api/v1/orgs/nope/actions/secrets", Some(&admin), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let selected = json!({ "value": "token", "visibility": "selected", "selected_repository_ids": [site.0, rocket.0] });
        let (response, json) = fixture.send("PUT", &npm, Some(&admin), Some(selected)).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{json}");
        let (response, json) = fixture.send("PUT", &npm, Some(&admin), Some(json!({ "value": "other" }))).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "{json}");
        let (_, json) = fixture.send("GET", &format!("{org}/secrets/NPM_TOKEN"), Some(&admin), None).await;
        assert_eq!(
            (json["visibility"].as_str(), json["selected_repository_ids"].clone()),
            (Some("selected"), json!([site.0])),
            "the visibility is kept, and only the organisation's repositories are given the secret"
        );
        assert!(json.get("value").is_none());
        let (response, json) = fixture.send("PUT", &npm, Some(&admin), Some(json!({ "value": "x", "visibility": "private" }))).await;
        assert_eq!((response.status(), field(&json)), (StatusCode::UNPROCESSABLE_ENTITY, Some("visibility")));
        let (response, _) = fixture.send("PUT", &npm, Some(&admin), Some(json!({ "value": "x", "visibility": "all" }))).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let (_, json) = fixture.send("GET", &format!("{org}/secrets"), Some(&admin), None).await;
        assert_eq!((json["secrets"][0]["visibility"].as_str(), json["secrets"][0].get("selected_repository_ids")), (Some("all"), None));

        let stage = json!({ "name": "stage", "value": "prod" });
        let (response, _) = fixture.send("POST", &format!("{org}/variables"), Some(&admin), Some(stage)).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let (_, json) = fixture.send("GET", &format!("{org}/variables/stage"), Some(&admin), None).await;
        assert_eq!((json["value"].as_str(), json["visibility"].as_str()), (Some("prod"), Some("all")));

        // The caller's own.
        let user = "/api/v1/user/actions";
        let (response, _) = fixture.send("GET", &format!("{user}/secrets"), None, None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let (response, _) = fixture.send("GET", &format!("{user}/secrets"), Some(&write), None).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let (response, _) = fixture.send("PUT", &format!("{user}/secrets/mine"), Some(&admin), Some(json!({ "value": "x" }))).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let (_, json) = fixture.send("GET", &format!("{user}/secrets"), Some(&admin), None).await;
        let mine = &json["secrets"][0];
        assert_eq!(
            (json["total_count"].as_u64(), mine["name"].as_str(), mine["visibility"].as_str()),
            (Some(1), Some("MINE"), Some("all"))
        );
        let (_, json) = fixture.send("GET", &format!("{user}/secrets"), Some(&bobs), None).await;
        assert_eq!(json["total_count"], 0, "everyone sees their own");
        let (_, json) = fixture.send("GET", &format!("{org}/secrets"), Some(&admin), None).await;
        assert_eq!(json["total_count"], 1, "the user's secrets are not the organisation's");
    }

    /// `path` with representative parameters.
    fn concrete(path: &str) -> String {
        [
            ("{owner}", "alice"),
            ("{repo}", "rocket"),
            ("{org}", "acme"),
            ("{run_id}", "1"),
            ("{job_id}", "1"),
            ("{workflow_id}", "ci.yml"),
            ("{name}", "TOKEN"),
            ("{artifact_id}", "1"),
            ("{archive_format}", "tar.gz"),
            ("{runner_id}", "1"),
        ]
        .into_iter()
        .fold(path.to_owned(), |path, (parameter, value)| path.replace(parameter, value))
    }

    #[tokio::test]
    async fn with_actions_disabled_none_of_these_paths_exists() {
        let fixture = Fixture::new(|_| {}).await;
        assert!(fixture.services.actions.is_none());
        let alice = fixture.user("alice").await;
        fixture.repo(&alice, "rocket", Visibility::Public).await;
        let input = CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() };
        fixture.services.orgs.create(&alice, input).await.unwrap();
        let admin = fixture.token(&alice, TokenScope::Admin).await;
        for entry in routes() {
            let path = format!("/api/v1{}", concrete(&entry.path));
            let method = entry.method.as_str().to_ascii_uppercase();
            let body = matches!(entry.method, Post | Put | Patch).then(|| json!({ "ref": "main", "value": "x", "name": "TOKEN" }));
            let (response, json) = fixture.send(&method, &path, Some(&admin), body).await;
            assert_eq!((response.status(), json["message"].as_str()), (StatusCode::NOT_FOUND, Some("Not Found")), "{method} {path}");
        }
        let (_, json) = fixture.send("GET", "/api/v1/openapi.json", None, None).await;
        assert!(json["paths"].as_object().unwrap().keys().all(|path| !path.contains("/actions")));

        let enabled = super::tests::fixture().await;
        let (_, json) = enabled.send("GET", "/api/v1/openapi.json", None, None).await;
        assert!(json["paths"]["/repos/{owner}/{repo}/actions/runs"]["get"].is_object());
        assert!(json["components"]["schemas"]["WorkflowRun"].is_object());
    }

    #[test]
    fn github_statuses_cover_every_run_status_once() {
        for status in Status::ALL {
            let named = ["queued", "waiting", "in_progress", "completed"]
                .into_iter()
                .filter(|name| github_statuses(name).is_some_and(|statuses| statuses.contains(&status)))
                .count();
            assert_eq!(named, 1, "{status}");
        }
        assert_eq!(github_statuses("timed_out"), Some(&[][..]));
        assert_eq!(github_statuses("Completed"), None);
    }
}
