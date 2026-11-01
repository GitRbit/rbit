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
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use rbit_core::actions::{JobId, Step, TaskId};
use rbit_services::{
    ActionsService, RepoContext,
    actions::logs::{LogRange, RawLog},
};
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

fn actions(state: &AppState) -> WebResult<ActionsService> {
    state.services.actions.snapshot().ok_or(WebError::NotFound)
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

fn preview_steps(steps: &[Step]) -> Vec<&Step> {
    let mut shown: Vec<_> = steps.iter().take(32).collect();
    let important: Vec<_> = [rbit_core::actions::Status::Running, rbit_core::actions::Status::Failure]
        .into_iter()
        .filter_map(|status| steps.iter().find(|step| step.status == status))
        .collect();
    for step in &important {
        if !shown.iter().any(|shown| shown.number == step.number)
            && let Some(slot) = shown.iter().rposition(|shown| !important.iter().any(|pinned| pinned.number == shown.number))
        {
            shown[slot] = step;
        }
    }
    shown.sort_by_key(|step| step.number);
    shown
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
    let shown = preview_steps(&detail.steps);
    let omitted_steps = detail.steps.len().saturating_sub(shown.len());
    let bytes = JOB_PAGE_STEP_BYTES / shown.len().max(1);
    let lines = JOB_PAGE_STEP_LINES / shown.len().max(1);
    let mut steps = Vec::with_capacity(shown.len());
    if let Some(task) = detail.job.task_id {
        for step in shown {
            let log = step_log(&actions, &loaded.ctx, task, step, lines, bytes).await?;
            steps.push(StepView::new(
                step,
                log,
                format!("/{owner}/{repo}/-/plugins/rbit.actions/logs?run={number}&job={}&step={}", job.0, step.number),
            ));
        }
    }
    open_one(&mut steps);
    let people = actions.run_detail(&loaded.ctx, number).await?.people;
    let finished = detail.job.status.is_done();
    let can_rerun = loaded.ctx.can_write_content() && finished && detail.run.status.is_done();
    Ok(JobPage {
        omitted_steps,
        job_id: detail.job.id.0,
        page: page.titled(format!(
            "{} · {} #{} · {}",
            crate::views::actions::preview_label(&detail.job.name),
            crate::views::actions::preview_label(&detail.run.workflow_name),
            number,
            loaded.ctx.repo.full_name()
        )),
        header: loaded.header(&state, "actions"),
        run: RunSummary::new(&detail.run, &people, &urls, loaded.urls.commit(&detail.run.sha)),
        run_url: urls.run(number),
        name: crate::views::actions::preview_label(&detail.job.name),
        look: StateLook::of(detail.job.status),
        duration: crate::views::actions::job_duration(&detail.job),
        labels: crate::views::actions::preview_label(&detail.job.runs_on.join(", ")),
        active: !finished,
        self_url: job_url.clone(),
        steps,
        raw_url: detail.job.task_id.map(|_| format!("/{owner}/{repo}/actions/runs/{number}/jobs/{}/logs/raw", job.0)),
        rerun_url: can_rerun.then(|| urls.run_action(number, "rerun-job")),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn step_page(
    state: &AppState,
    viewer: &Viewer,
    page: Page,
    owner: &str,
    repo: &str,
    run: &str,
    job: &str,
    wanted: i64,
) -> WebResult<StepLogPage> {
    let actions = actions(state)?;
    let (number, job) = ids(run, job)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    let detail = actions.job_detail(&loaded.ctx, job).await?;
    let job_url = ActionsUrls::new(&loaded.urls.home()).job(number, detail.job.id.0);
    if detail.run.number != number {
        return Err(WebError::NotFound);
    }
    let (Some(task), Some(step)) = (detail.job.task_id, detail.steps.iter().find(|step| step.number == wanted)) else {
        return Err(WebError::NotFound);
    };
    let log = step_log(&actions, &loaded.ctx, task, step, JOB_PAGE_STEP_LINES, JOB_PAGE_STEP_BYTES).await?;
    let view = StepView::new(step, log, String::new());
    Ok(StepLogPage {
        page: page.titled(format!(
            "{} · {} · {}",
            crate::views::actions::preview_label(&step.name),
            crate::views::actions::preview_label(&detail.job.name),
            loaded.ctx.repo.full_name()
        )),
        header: loaded.header(state, "actions"),
        job_name: crate::views::actions::preview_label(&detail.job.name),
        active: !step.status.is_done(),
        self_url: format!("/{owner}/{repo}/-/plugins/rbit.actions/logs?run={number}&job={}&step={wanted}", job.0),
        raw_url: format!("/{owner}/{repo}/actions/runs/{number}/jobs/{}/logs/raw", job.0),
        job_url,
        step: view,
    })
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
    let file_name = format!("{}-run-{number}-{}.log", loaded.ctx.repo.repo.name, detail.job.key);
    Ok(raw_log_response(log, &headers, &file_name))
}

// Actions: REST API.
/// A raw log as `text/plain`, inline and sandboxed: gzipped as stored when the request
/// accepts it, decompressed otherwise. The REST API's job log answers the same.
pub(crate) fn raw_log_response(log: RawLog, request: &HeaderMap, file_name: &str) -> Response {
    let accepts_gzip = request
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
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    if let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{}\"", file_name.replace(['"', '\\', '\r', '\n'], "_"))) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response
}
