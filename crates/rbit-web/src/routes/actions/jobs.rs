//! A job of a run: its steps with their logs, one step's longer log, and the raw log.
//!
//! The pages need a reader of a repository with Actions on, like the run page. A job that
//! is not finished refreshes its section every 3 s with htmx; a reload does the same. The
//! raw log is the task's whole log as `text/plain`, sent gzipped as stored when the client
//! accepts it and decompressed otherwise, inline and sandboxed like a raw file.
//!
//! Workstream U (interface and secrets) owns this file.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use rbit_core::actions::{JobId, Step, TaskId};
use rbit_services::{
    ActionsService, RepoContext,
    actions::logs::{LogRange, STEP_VIEW_BYTES, STEP_VIEW_LINES},
};
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    routes::repo::RepoPage,
    views::{
        actions::{
            ActionsUrls, JOB_PAGE_STEP_BYTES, JOB_PAGE_STEP_LINES, JobPage, RunSummary, StateLook, StepLog, StepLogPage, StepView, open_one,
        },
        layout::Page,
    },
};

fn actions(state: &AppState) -> WebResult<&ActionsService> {
    state.services.actions.as_ref().ok_or(WebError::NotFound)
}

fn ids(run: &str, job: &str) -> WebResult<(i64, JobId)> {
    let run = run.parse().ok().filter(|number: &i64| *number > 0).ok_or(WebError::NotFound)?;
    let job = job.parse().map(JobId).map_err(|_| WebError::NotFound)?;
    Ok((run, job))
}

/// The lines of `step` of `task`, within the limits, or `None` before it started.
async fn step_log(
    actions: &ActionsService,
    repo: &RepoContext,
    task: TaskId,
    step: &Step,
    lines: usize,
    bytes: usize,
) -> WebResult<Option<StepLog>> {
    if step.started_at.is_none() {
        return Ok(None);
    }
    let window = actions.step_log(repo, task, step.number, LogRange::Tail, lines, bytes).await?;
    Ok(Some(StepLog::new(&window, u64::try_from(step.log_index).unwrap_or_default())))
}

/// `GET /{owner}/{repo}/actions/runs/{run}/jobs/{job}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, run, job)): Path<(String, String, String, String)>,
) -> WebResult<JobPage> {
    let actions = actions(&state)?;
    let (number, job) = ids(&run, &job)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let detail = actions.job_detail(&loaded.ctx, job).await?;
    if detail.run.number != number {
        return Err(WebError::NotFound);
    }
    let urls = ActionsUrls::new(&loaded.urls.home());
    let job_url = urls.job(number, detail.job.id.0);
    let mut steps = Vec::with_capacity(detail.steps.len());
    if let Some(task) = detail.job.task_id {
        for step in &detail.steps {
            let log = step_log(actions, &loaded.ctx, task, step, JOB_PAGE_STEP_LINES, JOB_PAGE_STEP_BYTES).await?;
            steps.push(StepView::new(step, log, format!("{job_url}/logs?step={}", step.number)));
        }
    }
    open_one(&mut steps);
    let people = actions.run_detail(&loaded.ctx, number).await?.people;
    let finished = detail.job.status.is_done();
    let can_rerun = loaded.ctx.can_write_content() && finished && detail.run.status.is_done();
    Ok(JobPage {
        page: page.titled(format!("{} · {} #{} · {}", detail.job.name, detail.run.workflow_name, number, loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "actions"),
        run: RunSummary::new(&detail.run, &people, &urls, loaded.urls.commit(&detail.run.sha)),
        run_url: urls.run(number),
        name: detail.job.name.clone(),
        look: StateLook::of(detail.job.status),
        duration: crate::views::actions::job_duration(&detail.job),
        labels: detail.job.runs_on.join(", "),
        active: !finished,
        self_url: job_url.clone(),
        steps,
        raw_url: detail.job.task_id.map(|_| format!("{job_url}/logs/raw")),
        rerun_url: can_rerun.then(|| format!("{job_url}/rerun")),
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StepQuery {
    step: Option<i64>,
}

/// `GET /{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs?step=n`: one step's log, up to
/// `STEP_VIEW_LINES` of its last lines. Without `step`, the job page.
pub async fn logs(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, run, job)): Path<(String, String, String, String)>,
    Query(query): Query<StepQuery>,
) -> WebResult<Response> {
    let actions = actions(&state)?;
    let (number, job) = ids(&run, &job)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let detail = actions.job_detail(&loaded.ctx, job).await?;
    let job_url = ActionsUrls::new(&loaded.urls.home()).job(number, detail.job.id.0);
    if detail.run.number != number {
        return Err(WebError::NotFound);
    }
    let Some(wanted) = query.step else {
        return Ok(axum::response::Redirect::to(&job_url).into_response());
    };
    let (Some(task), Some(step)) = (detail.job.task_id, detail.steps.iter().find(|step| step.number == wanted)) else {
        return Err(WebError::NotFound);
    };
    let log = step_log(actions, &loaded.ctx, task, step, STEP_VIEW_LINES, STEP_VIEW_BYTES).await?;
    let view = StepView::new(step, log, String::new());
    Ok(StepLogPage {
        page: page.titled(format!("{} · {} · {}", step.name, detail.job.name, loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "actions"),
        job_name: detail.job.name.clone(),
        active: !step.status.is_done(),
        self_url: format!("{job_url}/logs?step={wanted}"),
        raw_url: format!("{job_url}/logs/raw"),
        job_url,
        step: view,
    }
    .into_response())
}

/// `GET /{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs/raw`
pub async fn raw(
    State(state): State<AppState>,
    viewer: Viewer,
    headers: HeaderMap,
    Path((owner, repo, run, job)): Path<(String, String, String, String)>,
) -> WebResult<Response> {
    let actions = actions(&state)?;
    let (number, job) = ids(&run, &job)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let detail = actions.job_detail(&loaded.ctx, job).await?;
    if detail.run.number != number {
        return Err(WebError::NotFound);
    }
    let task = detail.job.task_id.ok_or(WebError::NotFound)?;
    let Some(log) = actions.raw_log(&loaded.ctx, task).await? else {
        return Err(WebError::NotFound);
    };
    let accepts_gzip = headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|coding| coding.split(';').next().is_some_and(|name| name.trim().eq_ignore_ascii_case("gzip")));
    let mut response = if log.gzip && !accepts_gzip {
        let decoder = async_compression::tokio::bufread::GzipDecoder::new(tokio::io::BufReader::new(log.file));
        Body::from_stream(ReaderStream::new(decoder)).into_response()
    } else {
        let mut response = Body::from_stream(ReaderStream::new(log.file)).into_response();
        if log.gzip {
            response.headers_mut().insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        }
        response
    };
    let file_name = format!("{}-run-{number}-{}.log", loaded.ctx.repo.repo.name, detail.job.key);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    if let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{}\"", file_name.replace(['"', '\\', '\r', '\n'], "_"))) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}
