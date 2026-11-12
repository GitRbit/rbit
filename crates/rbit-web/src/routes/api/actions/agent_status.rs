//! rbit's own "where am I" and "tell me when it changes" for agents.
//!
//! * `GET /repos/{owner}/{repo}/agent/status?branch=` (`rbit.status/1`): the branch's head,
//!   where it stands against its base, the CI of exactly that commit (with the failed jobs
//!   and how they ended), the base's CI, the open pull request, the issue, and the next
//!   command worth running.
//! * `GET /repos/{owner}/{repo}/agent/watch?sha=&branch=&cursor=&timeout=` (`rbit.watch/1`):
//!   a long-poll. It answers at once when the CI of `sha` differs from what `cursor` saw (a
//!   job failed, a run finished, …), when `branch` no longer points at `sha`
//!   (`head_changed`, naming the newer commit), or after `timeout` seconds (at most 30) with
//!   `changed: false`. Each answer carries the `cursor` for the next call. The caller's
//!   rights are checked again before every answer.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rbit_services::{
    RepoContext, ServiceError,
    actions::{bisect::Recorded, status::CommitCi},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{actions, agent_logs};
use crate::{
    AppState,
    routes::api::{
        auth::Caller,
        error::{ApiError, ApiResult},
    },
};

/// The longest a watch waits before answering `changed: false`.
const MAX_WAIT_SECONDS: u64 = 30;
/// How often a waiting watch looks again.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// The path of these endpoints.
#[derive(Debug, Deserialize)]
pub struct RepoPath {
    owner: String,
    repo: String,
}

impl RepoPath {
    async fn open(&self, state: &AppState, caller: &Caller) -> ApiResult<RepoContext> {
        caller.open_repo(state, &self.owner, &self.repo).await
    }
}

/// The CI of a commit as both endpoints show it.
fn ci_json(ci: &CommitCi) -> Value {
    let [success, failure, running, queued] = ci.counts;
    let runs: Vec<Value> = ci
        .runs
        .iter()
        .map(|run| {
            let status = run.status.to_string();
            json!({ "id": run.id.0, "number": run.number, "workflow": run.workflow_name, "status": status, "error": run.error })
        })
        .collect();
    json!({
        "state": ci.state,
        "reason": ci.reason,
        "runs": runs,
        "jobs": { "success": success, "failure": failure, "running": running, "queued": queued },
        "failed": ci.failed.iter().map(agent_logs::failed_job).collect::<Vec<_>>(),
        "superseded_by": ci.superseded_by,
        "waits_for": ci.waits_for,
        "queue": ci.queue.map(|place| json!({
            "position": place.ahead + 1,
            "expected_start_seconds": place.mean_seconds.map(|mean| mean.saturating_mul(place.ahead)),
        })),
    })
}

/// A fingerprint of what a watcher cares about: the state, the job counts, each run's
/// status and the failed jobs.
fn cursor(ci: &CommitCi) -> String {
    let mut hasher = DefaultHasher::new();
    ci.state.hash(&mut hasher);
    ci.counts.hash(&mut hasher);
    for run in &ci.runs {
        (run.id.0, run.status.to_string()).hash(&mut hasher);
    }
    for failed in &ci.failed {
        failed.job.id.0.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// The command an agent most likely wants next.
fn next(state: &str, sha: &str, failed_run: Option<i64>, pull: Option<(i64, bool)>) -> Option<String> {
    match (state, failed_run, pull) {
        ("failure" | "indeterminate", Some(run), _) => Some(format!("rbit run failure {run}")),
        ("queued" | "running" | "not_started", ..) => Some(format!("rbit run watch --sha {sha}")),
        ("success", _, Some((number, false))) => Some(format!("rbit pr merge {number} --head {sha}")),
        ("success", _, None) => Some("rbit pr create --title …".to_owned()),
        ("superseded", ..) => Some("rbit status".to_owned()),
        _ => None,
    }
}

/// The query of `status`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StatusQuery {
    branch: Option<String>,
}

/// `GET /repos/{owner}/{repo}/agent/status`
pub async fn status(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    Query(query): Query<StatusQuery>,
) -> ApiResult<Json<Value>> {
    let ctx = path.open(&state, &caller).await?;
    let asked = query.branch.as_deref().filter(|branch| !branch.is_empty());
    let status = match actions(&state)?.branch_status(&ctx, asked).await {
        Err(ServiceError::NotFound) => {
            let branch = asked.unwrap_or(&ctx.repo.repo.default_branch);
            return Err(ApiError::new(StatusCode::NOT_FOUND, format!("branch `{branch}` does not resolve to a commit")));
        }
        result => result?,
    };
    let failed_run = status.ci.failed.first().map(|failed| failed.job.run_id.0);
    let pull = status.pull.as_ref().map(|pull| (pull.number, pull.draft));
    // A job no runner can take waits for ever: watching it is no next step.
    let next = if status.ci.waits_for.is_some() { None } else { next(status.ci.state, &status.head, failed_run, pull) };
    let divergence = status
        .divergence
        .as_ref()
        .map(|divergence| json!({ "merge_base": divergence.merge_base, "ahead": divergence.ahead, "behind": divergence.behind }));
    let pull = status.pull.as_ref().map(|pull| json!({ "number": pull.number, "draft": pull.draft, "base": pull.base, "head_sha": pull.head_sha, "up_to_date_with_head": pull.head_sha == status.head }));
    let mut ci = ci_json(&status.ci);
    ci["for_sha"] = json!(status.head);
    Ok(Json(json!({
        "schema": "rbit.status/1",
        "repo": ctx.repo.full_name(),
        "branch": status.branch,
        "head": { "sha": status.head, "base": status.base, "divergence": divergence },
        "ci": ci,
        "base": { "branch": status.base, "sha": status.base_sha, "ci": status.base_ci },
        "pr": pull,
        "task": status.issue.map(|issue| json!({ "issue": issue })),
        "next": next,
    })))
}

/// The query of `watch`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WatchQuery {
    sha: String,
    branch: Option<String>,
    cursor: Option<String>,
    timeout: Option<u64>,
}

/// `GET /repos/{owner}/{repo}/agent/watch`
pub async fn watch(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    Query(query): Query<WatchQuery>,
) -> ApiResult<Json<Value>> {
    let sha = query.sha.trim().to_ascii_lowercase();
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::invalid("sha", "give the full id of the commit to watch"));
    }
    let deadline = Instant::now() + Duration::from_secs(query.timeout.unwrap_or(MAX_WAIT_SECONDS).min(MAX_WAIT_SECONDS));
    let actions = actions(&state)?;
    loop {
        // Every look opens the repository with the caller's rights again.
        let ctx = path.open(&state, &caller).await?;
        let ci = actions.commit_ci(&ctx, &sha).await?;
        let seen = cursor(&ci);
        let moved = match query.branch.as_deref().filter(|branch| !branch.is_empty()) {
            Some(branch) => ctx.reader.commit_id(format!("refs/heads/{branch}")).await.ok().filter(|head| *head != sha),
            None => None,
        };
        let changed = query.cursor.as_deref() != Some(seen.as_str());
        if changed || moved.is_some() || Instant::now() >= deadline {
            let done = matches!(ci.state, "success" | "failure" | "cancelled" | "superseded" | "indeterminate" | "none_expected");
            let mut body = ci_json(&ci);
            body["schema"] = json!("rbit.watch/1");
            body["sha"] = json!(sha);
            body["cursor"] = json!(seen);
            body["changed"] = json!(changed);
            body["done"] = json!(done);
            if let Some(newer) = moved {
                body["head_changed"] = json!({ "branch": query.branch, "sha": newer, "next": format!("rbit run watch --sha {newer}") });
            }
            return Ok(Json(body));
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// The query of a bisection.
#[derive(Debug, Deserialize)]
pub struct BisectQuery {
    good: String,
    bad: String,
    /// One workflow's results only (its path, `.rbit/workflows/ci.yml`).
    #[serde(default)]
    workflow: Option<String>,
}

/// Untested commits a bisection lists by id.
const BISECT_LISTED: usize = 50;

/// `GET …/agent/bisect?good=<rev>&bad=<rev>[&workflow=<path>]`: what CI already recorded
/// between two commits, narrowed to the commits left to test (agent plan step 4.9).
pub async fn bisect(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    Query(query): Query<BisectQuery>,
) -> ApiResult<Json<Value>> {
    let ctx = path.open(&state, &caller).await?;
    let workflow = query.workflow.as_deref().filter(|workflow| !workflow.is_empty());
    let found = actions(&state)?.bisect(&ctx, &query.good, &query.bad, workflow).await?;
    let count = |wanted: Recorded| found.commits.iter().filter(|(_, recorded)| *recorded == wanted).count();
    Ok(Json(json!({
        "schema": "rbit.bisect/1",
        "commits": found.commits.len(),
        "passed": count(Recorded::Passed),
        "failed": count(Recorded::Failed),
        "untested": count(Recorded::Untested),
        "truncated": found.truncated,
        "last_good": found.last_good,
        "first_bad": found.first_bad,
        "culprit": found.next.is_none().then_some(&found.first_bad),
        "next": found.next,
        "remaining": found.remaining.len(),
        "remaining_shas": found.remaining.iter().take(BISECT_LISTED).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_command_follows_the_state() {
        let sha = "a".repeat(40);
        assert_eq!(next("failure", &sha, Some(12), None).as_deref(), Some("rbit run failure 12"));
        assert_eq!(next("running", &sha, None, None), Some(format!("rbit run watch --sha {sha}")));
        assert_eq!(next("success", &sha, None, Some((4, false))), Some(format!("rbit pr merge 4 --head {sha}")));
        assert_eq!(next("success", &sha, None, Some((4, true))), None, "a draft is not merged");
        assert_eq!(next("none_expected", &sha, None, None), None);
    }

    #[test]
    fn a_cursor_moves_when_what_a_watcher_cares_about_moves() {
        let ci = |state: &'static str, counts: [usize; 4]| CommitCi {
            state,
            reason: None,
            runs: Vec::new(),
            counts,
            failed: Vec::new(),
            superseded_by: None,
            queue: None,
            waits_for: None,
        };
        assert_eq!(cursor(&ci("running", [1, 0, 1, 0])), cursor(&ci("running", [1, 0, 1, 0])));
        assert_ne!(cursor(&ci("running", [1, 0, 1, 0])), cursor(&ci("running", [2, 0, 0, 0])));
        assert_ne!(cursor(&ci("running", [1, 0, 1, 0])), cursor(&ci("failure", [1, 0, 1, 0])));
    }
}
