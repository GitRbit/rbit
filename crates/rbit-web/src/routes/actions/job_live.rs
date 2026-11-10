//! The job page's live tail: the lines a running job wrote after the ones a page has.
//!
//! `GET …/jobs/{job}/logs/live?from=<line>` answers JSON for `assets/actions-log.js`:
//!
//! * `state`: the job's and its steps' states in one string. When it differs from the
//!   page's `data-live-state`, the page reloads the job section (a step started or ended).
//! * `active`: the job is not done; once it is, the page reloads the section a last time
//!   and stops asking.
//! * `target`, `html`, `next`: the lines of the task log from `from` on (never before the
//!   target step's first line), as rows of the step `target` (`views::actions::log_lines`,
//!   with group ends kept as markers), and the line to ask from next time. Without a step
//!   whose log grows there is no target and no lines.
//!
//! The same reader check as the job page applies (a private repository's job is a 404 to
//! anyone who cannot read it), and one answer holds at most the job page's step budget
//! ([`JOB_PAGE_STEP_LINES`], [`JOB_PAGE_STEP_BYTES`]); a page that got a full answer asks
//! again at once.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use rbit_core::actions::{Status, Step};
use rbit_services::actions::logs::LogRange;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

use super::jobs::{actions, ids};
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    routes::repo::RepoPage,
    views::actions::{JOB_PAGE_STEP_BYTES, JOB_PAGE_STEP_LINES, StateLook, log_lines::Console},
};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct LiveQuery {
    from: u64,
}

/// One answer; see the module documentation.
#[derive(Debug, Serialize)]
pub struct Live {
    pub state: String,
    pub active: bool,
    pub target: Option<String>,
    pub html: String,
    pub next: u64,
}

/// The states of a job and its steps as one string, which changes whenever a step starts,
/// ends or is added: the job page carries it as `data-live-state`.
pub fn live_state(job: Status, steps: &[Step]) -> String {
    let mut state = StateLook::of(job).key.to_owned();
    for step in steps {
        let _ = write!(state, ",{}:{}", step.number, StateLook::of(step.status).key);
    }
    state
}

/// The step whose log grows now, and where its log starts.
fn target(steps: &[Step]) -> Option<(String, u64)> {
    let running = steps.iter().find(|step| step.status == Status::Running)?;
    Some((running.number.to_string(), u64::try_from(running.log_index).unwrap_or_default()))
}

/// `GET /{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs/live?from=n`
pub async fn live(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, run, job)): Path<(String, String, String, String)>,
    Query(query): Query<LiveQuery>,
) -> WebResult<Response> {
    let actions = actions(&state)?;
    let (number, job) = ids(&run, &job)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let detail = actions.job_detail(&loaded.ctx, job).await?;
    if detail.run.number != number {
        return Err(WebError::NotFound);
    }
    let mut live = Live {
        state: live_state(detail.job.status, &detail.steps),
        active: !detail.job.status.is_done(),
        target: None,
        html: String::new(),
        next: query.from,
    };
    // A finished job has nothing more to tail: the page reloads it whole.
    let task = detail.job.task_id.filter(|_| live.active);
    if let (Some(task), Some((key, start))) = (task, target(&detail.steps)) {
        let from = query.from.max(start);
        let window = actions.task_log(&loaded.ctx, task, LogRange::From(from), JOB_PAGE_STEP_LINES, JOB_PAGE_STEP_BYTES).await?;
        let first = usize::try_from(from - start).unwrap_or(usize::MAX).saturating_add(1);
        live.html = Console { step: &key, first, running: true, live: true }.render(&window.lines).html;
        live.next = window.next_line.max(from);
        live.target = Some(key);
    }
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(live)).into_response())
}
