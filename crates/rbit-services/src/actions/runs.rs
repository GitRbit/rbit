//! Creating runs from planned workflows (one transaction: the run and its jobs, then
//! `pending` statuses), concurrency groups, cancel, re-run, approval and listings.
//!
//! * [`ActionsService::start_run`] turns one workflow's answer to an event (a
//!   [`Candidate`]) into a run. A workflow that failed to parse or plan becomes a
//!   *workflow error*: a failed run without jobs whose error tells its author why nothing
//!   ran. The caps of §11.8 apply here: an event starts at most `max_runs_per_event` runs
//!   and `max_jobs_per_event` jobs ([`Allowance`]), and a repository may have at most
//!   `max_waiting_jobs_per_repository` jobs waiting — runs beyond that become workflow
//!   errors saying so.
//! * Concurrency (§11.6) is applied when a run becomes runnable: on creation for trusted
//!   runs, on approval for untrusted ones, which therefore cancel nothing while they wait.
//!   Untrusted runs' groups are namespaced `pr-<number>/`, so they only ever meet runs of
//!   the same pull request. With `cancel-in-progress` the group's other runs are cancelled;
//!   without it the new run is held while another run of the group is active, and a run that
//!   was only held is replaced by the newer one, as on GitHub. Group decisions are made one
//!   at a time (`Inner::groups`), so two runs created at once cannot both start.
//! * People act on runs through [`ActionsService::cancel`], [`ActionsService::rerun`] and
//!   [`ActionsService::approve`]: writers only, and only people — the caller is a `User`,
//!   so a task token can never cancel, re-run or approve (§11.5). Approvals (and trusting an
//!   author with them) are audited. Re-running an untrusted run is not approving it: it
//!   waits again unless its author would skip the wait today.
//! * [`ActionsService::settled`] follows every transition of a run's jobs: the scheduler's
//!   version moves when jobs became waiting, the jobs' commit statuses are reported
//!   ([`super::statuses`]), and a finished run lets the next held run of its group start. The runner protocol calls it after finishing tasks and sweeping.
//!
//! Workstream T (triggers and runs) owns this file.

use rbit_core::{
    Account, AccountId, IssueNumber, RepoId, Timestamp, User,
    actions::{PermissionLevel, Run, RunEvent, RunId, RunTrust, Status, TokenPermissions},
    repository::OwnedRepository,
};
pub use rbit_storage::sqlite::actions_runs::Rerun;
use rbit_storage::sqlite::actions_runs::{NewJob, NewRun, RunStart, Settled};
use rbit_workflow::{
    PlannedJob,
    model::{Permission, Permissions},
};

use super::ActionsService;
use crate::{
    RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// Longest stored workflow error, in bytes (the column's CHECK).
pub const MAX_ERROR_BYTES: usize = 2048;
/// Longest stored name, title, path or concurrency group, in characters.
const MAX_TEXT_CHARS: usize = 1024;
/// Longest runner label a job may ask for, in characters.
const MAX_JOB_LABEL_CHARS: usize = 255;

/// A workflow planned for one event: its jobs and its concurrency group.
#[derive(Debug, Clone)]
pub struct Plan {
    pub jobs: Vec<PlannedJob>,
    /// The evaluated `concurrency.group` and `cancel-in-progress`.
    pub concurrency: Option<(String, bool)>,
}

/// One workflow file's answer to an event: a plan, or why it has none.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// `.forgejo/workflows/ci.yml`.
    pub path: String,
    /// The workflow's `name`, or its path.
    pub name: String,
    /// The plan, or the error shown on the workflow-error run.
    pub outcome: Result<Plan, String>,
}

/// The event every run it starts shares.
#[derive(Debug, Clone)]
pub struct Trigger<'a> {
    pub repo: &'a OwnedRepository,
    pub event: RunEvent,
    /// The full ref.
    pub reference: &'a str,
    pub sha: &'a str,
    /// The commit subject, pull request title or "Scheduled".
    pub title: &'a str,
    pub actor: Option<AccountId>,
    pub trigger_actor: Option<AccountId>,
    pub pull_number: Option<i64>,
    pub is_fork: bool,
    pub trust: RunTrust,
    /// The event payload as stored (see `context::fit_payload`).
    pub payload: &'a str,
    /// The checked dispatch inputs (a JSON object).
    pub inputs: Option<&'a str>,
}

/// What one event may still start (§11.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowance {
    pub runs: u32,
    pub jobs: u32,
}

impl ActionsService {
    /// The allowance of one event, from `[actions] max_runs_per_event` and
    /// `max_jobs_per_event`.
    pub fn allowance(&self) -> Allowance {
        Allowance { runs: self.config().max_runs_per_event, jobs: self.config().max_jobs_per_event }
    }

    /// Creates the run `candidate` asks for, within `allowance`; `None` once the event
    /// started as many runs as it may.
    pub async fn start_run(
        &self,
        trigger: &Trigger<'_>,
        candidate: Candidate,
        allowance: &mut Allowance,
    ) -> Result<Option<Run>, ServiceError> {
        if allowance.runs == 0 {
            tracing::warn!(repo = %trigger.repo.full_name(), workflow = %candidate.path, "an event may start at most {} runs; skipped", self.config().max_runs_per_event);
            return Ok(None);
        }
        allowance.runs -= 1;
        let plan = match candidate.outcome {
            Ok(plan) => plan,
            Err(error) => return self.record_error(trigger, &candidate.path, &candidate.name, &error).await.map(Some),
        };
        let jobs = u32::try_from(plan.jobs.len()).unwrap_or(u32::MAX);
        if jobs > allowance.jobs {
            let max = self.config().max_jobs_per_event;
            let error = format!("{}: this event already started its share of jobs; one event may start at most {max}", candidate.path);
            return self.record_error(trigger, &candidate.path, &candidate.name, &error).await.map(Some);
        }
        allowance.jobs -= jobs;
        if let Some(label) =
            plan.jobs.iter().flat_map(|job| &job.runs_on).find(|label| label.is_empty() || label.chars().count() > MAX_JOB_LABEL_CHARS)
        {
            let error = format!(
                "{}: `runs-on` label `{}` must be 1 to {MAX_JOB_LABEL_CHARS} characters",
                candidate.path,
                truncate_chars(label, 64)
            );
            return self.record_error(trigger, &candidate.path, &candidate.name, &error).await.map(Some);
        }
        let group = plan.concurrency.as_ref().map(|(group, cancel)| (namespaced_group(trigger, group), *cancel));
        let awaiting = matches!(trigger.trust, RunTrust::Untrusted { awaits_approval: true });
        let decisions = self.inner.groups.lock().await;
        let start = if awaiting {
            RunStart::AwaitingApproval
        } else {
            let waiting = self.ctx().database.actions_runs().count_waiting(trigger.repo.repo.id).await?;
            let starting = u64::try_from(plan.jobs.iter().filter(|job| job.needs.is_empty()).count()).unwrap_or(u64::MAX);
            let max = u64::from(self.config().max_waiting_jobs_per_repository);
            if waiting + starting > max {
                let error =
                    format!("{}: the repository already has {waiting} jobs waiting for a runner; at most {max} may wait", candidate.path);
                return self.record_error(trigger, &candidate.path, &candidate.name, &error).await.map(Some);
            }
            match &group {
                Some((group, cancel)) => self.enter_group(trigger.repo.repo.id, group, *cancel, None).await?,
                None => RunStart::Runnable,
            }
        };
        let rows = JobRows::new(&plan.jobs);
        let jobs = rows.jobs(&plan.jobs);
        let (path, name, title) = (
            truncate_chars(&candidate.path, MAX_TEXT_CHARS),
            truncate_chars(&candidate.name, MAX_TEXT_CHARS),
            truncate_chars(trigger.title, MAX_TEXT_CHARS),
        );
        let run = self
            .ctx()
            .database
            .actions_runs()
            .create_run(&NewRun {
                concurrency_group: group.as_ref().map(|(group, _)| group.as_str()),
                cancel_in_progress: group.as_ref().is_some_and(|(_, cancel)| *cancel),
                start,
                jobs: &jobs,
                error: None,
                ..new_run(trigger, &path, &name, &title)
            })
            .await?;
        drop(decisions);
        tracing::info!(repo = %trigger.repo.full_name(), run = run.id.0, number = run.number, workflow = %run.workflow_path, event = %run.event, status = %run.status, "workflow run created");
        if start == RunStart::Runnable {
            self.bump_tasks_version();
        }
        self.report_statuses(run.id).await?;
        Ok(Some(run))
    }

    /// Records a workflow error: a failed run without jobs that says why nothing ran.
    async fn record_error(&self, trigger: &Trigger<'_>, path: &str, name: &str, error: &str) -> Result<Run, ServiceError> {
        let (path, name, title) =
            (truncate_chars(path, MAX_TEXT_CHARS), truncate_chars(name, MAX_TEXT_CHARS), truncate_chars(trigger.title, MAX_TEXT_CHARS));
        let error = truncate_bytes(error, MAX_ERROR_BYTES);
        let run = self
            .ctx()
            .database
            .actions_runs()
            .create_run(&NewRun { error: Some(&error), ..new_run(trigger, &path, &name, &title) })
            .await?;
        tracing::info!(repo = %trigger.repo.full_name(), run = run.id.0, workflow = %run.workflow_path, "workflow error recorded");
        // Actions: integration (I).
        self.announce(run.id).await?;
        Ok(run)
    }

    /// How a run of `group` that is about to become runnable starts (§11.6), with the
    /// group's decisions already serialised by the caller: with `cancel_in_progress` every
    /// other unfinished run of the group is cancelled; otherwise runs that were only held
    /// are cancelled (the newer run replaces them) and the run is held while another is
    /// active. Runs awaiting approval never hold a group. `own` is the run itself, when it
    /// already exists.
    async fn enter_group(&self, repo: RepoId, group: &str, cancel_in_progress: bool, own: Option<RunId>) -> Result<RunStart, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let mut busy = false;
        for other in store.unfinished_in_group(repo, group).await? {
            if Some(other.id) == own || (other.status == Status::AwaitingApproval && !cancel_in_progress) {
                continue;
            }
            if cancel_in_progress || other.status == Status::Blocked {
                if let Some(settled) = store.cancel_run(repo, other.id, Timestamp::now()).await? {
                    tracing::info!(repo = repo.0, run = other.id.0, group, "workflow run cancelled by its concurrency group");
                    self.followed_up(&settled).await?;
                }
            } else {
                busy = true;
            }
        }
        Ok(if busy { RunStart::HeldForConcurrency } else { RunStart::Runnable })
    }

    /// Follows up a transition of a run's jobs (see the module documentation). Call it
    /// after every store transition that returns [`Settled`].
    pub async fn settled(&self, settled: &Settled) -> Result<(), ServiceError> {
        self.followed_up(settled).await?;
        if !settled.run_status.is_done() {
            return Ok(());
        }
        let store = self.ctx().database.actions_runs();
        let Some(run) = store.run(settled.run_id).await? else { return Ok(()) };
        if let Some(group) = &run.concurrency_group {
            self.release_next(run.repo_id, group).await?;
        }
        Ok(())
    }

    /// What every transition needs, without the group: the scheduler's version and the
    /// jobs' commit statuses.
    async fn followed_up(&self, settled: &Settled) -> Result<(), ServiceError> {
        if !settled.now_waiting.is_empty() {
            self.bump_tasks_version();
        }
        self.report_statuses(settled.run_id).await
    }

    /// Starts the oldest held run of `group` once no other run of it is active.
    async fn release_next(&self, repo: RepoId, group: &str) -> Result<(), ServiceError> {
        let _decisions = self.inner.groups.lock().await;
        let store = self.ctx().database.actions_runs();
        let unfinished = store.unfinished_in_group(repo, group).await?;
        if unfinished.iter().any(|run| !matches!(run.status, Status::Blocked | Status::AwaitingApproval)) {
            return Ok(());
        }
        let Some(next) = unfinished.iter().find(|run| run.status == Status::Blocked) else { return Ok(()) };
        if let Some(settled) = store.release_run(repo, next.id, Timestamp::now()).await? {
            tracing::info!(repo = repo.0, run = next.id.0, group, "held workflow run released");
            self.followed_up(&settled).await?;
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ people acting on runs

impl ActionsService {
    /// Cancels run `id` of `repo` for `actor`, a writer: jobs not yet running at once,
    /// running ones in the database (their runners learn it on their next update).
    pub async fn cancel(&self, actor: &User, repo: &RepoContext, id: RunId) -> Result<Run, ServiceError> {
        require_write(repo)?;
        let store = self.ctx().database.actions_runs();
        let repo_id = repo.repo.repo.id;
        store.find_run(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        let Some(settled) = store.cancel_run(repo_id, id, Timestamp::now()).await? else {
            return Err(ServiceError::Invalid("the run has already finished".to_owned()));
        };
        tracing::info!(repo = %repo.repo.full_name(), run = id.0, by = %actor.name(), "workflow run cancelled");
        self.settled(&settled).await?;
        store.find_run(repo_id, id).await?.ok_or(ServiceError::NotFound)
    }

    /// Starts finished run `id` again for `actor`, a writer: every job, the failed ones, or
    /// one job — each with the jobs that need it. The run keeps its number and gets a new
    /// attempt. An untrusted run stays read-only and waits for approval again unless its
    /// author would skip the wait today: re-running is not approving.
    pub async fn rerun(&self, actor: &User, repo: &RepoContext, id: RunId, which: Rerun) -> Result<Run, ServiceError> {
        require_write(repo)?;
        let store = self.ctx().database.actions_runs();
        let repo_id = repo.repo.repo.id;
        let run = store.find_run(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        if let Rerun::Job(job) = which
            && store.find_job(repo_id, job).await?.is_none_or(|job| job.run_id != id)
        {
            return Err(ServiceError::NotFound);
        }
        if run.is_workflow_error() {
            return Err(ServiceError::Invalid("a workflow error has no jobs to run again".to_owned()));
        }
        if !run.status.is_done() {
            return Err(ServiceError::Invalid("only a finished run can be run again".to_owned()));
        }
        let awaits = self.awaits_approval(&repo.repo, &run).await?;
        let decisions = self.inner.groups.lock().await;
        let start = match (&run.concurrency_group, awaits) {
            (_, true) => RunStart::AwaitingApproval,
            (Some(group), false) => self.enter_group(repo_id, group, run.cancel_in_progress, Some(id)).await?,
            (None, false) => RunStart::Runnable,
        };
        let settled = store.rerun(repo_id, id, which, start, Timestamp::now()).await?;
        drop(decisions);
        let Some(settled) = settled else {
            return Err(ServiceError::Invalid("no job of the run matches".to_owned()));
        };
        tracing::info!(repo = %repo.repo.full_name(), run = id.0, by = %actor.name(), ?which, "workflow run started again");
        self.followed_up(&settled).await?;
        store.find_run(repo_id, id).await?.ok_or(ServiceError::NotFound)
    }

    /// Approves run `id` of `repo`, which awaits approval, for `actor`, a writer (audited).
    /// Its concurrency group applies now. The run stays read-only, without secrets or
    /// variables. With `trust_author`, the pull request's author is trusted in the
    /// repository from now on: their runs no longer wait (also audited).
    pub async fn approve(&self, actor: &User, repo: &RepoContext, id: RunId, trust_author: bool) -> Result<Run, ServiceError> {
        require_write(repo)?;
        let store = self.ctx().database.actions_runs();
        let repo_id = repo.repo.repo.id;
        let run = store.find_run(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        if run.status != Status::AwaitingApproval {
            return Err(ServiceError::Invalid("the run does not wait for approval".to_owned()));
        }
        let decisions = self.inner.groups.lock().await;
        let hold = match &run.concurrency_group {
            Some(group) => self.enter_group(repo_id, group, run.cancel_in_progress, Some(id)).await? == RunStart::HeldForConcurrency,
            None => false,
        };
        let settled = store.approve_run(repo_id, id, actor.id(), hold, Timestamp::now()).await?;
        drop(decisions);
        let Some(settled) = settled else {
            return Err(ServiceError::Invalid("the run does not wait for approval".to_owned()));
        };
        self.followed_up(&settled).await?;
        let full_name = repo.repo.full_name();
        let detail = format!("run #{} of {} at {}", run.number, run.workflow_path, run.sha);
        self.audit().record(AuditEntry::new(AuditAction::ActionsRunApproved).by(actor).on(full_name.clone()).detail(detail)).await;
        if trust_author && let Some(author) = self.pull_author(repo_id, &run).await? {
            let settings = self.ctx().database.actions_settings();
            if settings.trust(repo_id, author.id, actor.id(), Timestamp::now()).await? {
                let entry = AuditEntry::new(AuditAction::ActionsUserTrusted).by(actor).on(full_name).detail(author.name.to_string());
                self.audit().record(entry).await;
            }
        }
        store.find_run(repo_id, id).await?.ok_or(ServiceError::NotFound)
    }

    /// Whether an untrusted run waits for approval when it starts again: the rule of its
    /// creation (`trust::classify_pull_request`), with today's standing.
    async fn awaits_approval(&self, repo: &OwnedRepository, run: &Run) -> Result<bool, ServiceError> {
        if run.trusted {
            return Ok(false);
        }
        let author = self.pull_author(repo.repo.id, run).await?.map(|author| author.id);
        let trust = self.classify_pull_request(repo, run.actor, author).await?;
        Ok(matches!(trust, RunTrust::Untrusted { awaits_approval: true }))
    }

    /// The author of the pull request a run belongs to, if any.
    async fn pull_author(&self, repo: RepoId, run: &Run) -> Result<Option<Account>, ServiceError> {
        let Some(number) = run.pull_number.and_then(|number| u32::try_from(number).ok()).and_then(IssueNumber::new) else {
            return Ok(None);
        };
        let database = &self.ctx().database;
        let Some(author) = database.issues().find_by_number(repo, number).await?.and_then(|issue| issue.author) else { return Ok(None) };
        Ok(database.accounts().find_accounts(&[author]).await?.pop())
    }

    pub(super) fn audit(&self) -> AuditService {
        AuditService::from_store(self.ctx().database.audit())
    }
}

/// Acting on runs needs write access; readers learn the run exists, others do not.
fn require_write(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_write() {
        Ok(())
    } else if repo.access.can_read() {
        Err(ServiceError::Forbidden)
    } else {
        Err(ServiceError::NotFound)
    }
}

/// The run row every run of `trigger` shares; callers set the jobs, the group and the start.
fn new_run<'a>(trigger: &Trigger<'a>, path: &'a str, name: &'a str, title: &'a str) -> NewRun<'a> {
    NewRun {
        repo: trigger.repo.repo.id,
        workflow_path: path,
        workflow_name: name,
        title,
        event: trigger.event,
        reference: trigger.reference,
        sha: trigger.sha,
        actor: trigger.actor,
        trigger_actor: trigger.trigger_actor,
        pull_number: trigger.pull_number,
        is_fork: trigger.is_fork,
        trusted: trigger.trust.is_trusted(),
        concurrency_group: None,
        cancel_in_progress: false,
        event_payload: trigger.payload,
        dispatch_inputs: trigger.inputs,
        start: RunStart::Runnable,
        jobs: &[],
        error: None,
        now: Timestamp::now(),
    }
}

/// An untrusted run's group lives under its pull request (`pr-<number>/<group>`), so it
/// can never cancel or hold a run of anyone else.
fn namespaced_group(trigger: &Trigger<'_>, group: &str) -> String {
    let group = match (trigger.trust, trigger.pull_number) {
        (RunTrust::Untrusted { .. }, Some(number)) => format!("pr-{number}/{group}"),
        (RunTrust::Untrusted { .. }, None) => format!("untrusted/{group}"),
        (RunTrust::Trusted, _) => group.to_owned(),
    };
    truncate_chars(&group, MAX_TEXT_CHARS)
}

/// The owned values a run's [`NewJob`]s borrow.
struct JobRows {
    names: Vec<String>,
    matrices: Vec<Option<String>>,
    permissions: Vec<Option<String>>,
}

impl JobRows {
    fn new(jobs: &[PlannedJob]) -> Self {
        Self {
            names: jobs
                .iter()
                .map(|job| if job.name.trim().is_empty() { job.job_id.clone() } else { truncate_chars(&job.name, MAX_TEXT_CHARS) })
                .collect(),
            matrices: jobs.iter().map(PlannedJob::matrix_json).collect(),
            permissions: jobs.iter().map(|job| job.permissions.map(permissions_json)).collect(),
        }
    }

    fn jobs<'a>(&'a self, jobs: &'a [PlannedJob]) -> Vec<NewJob<'a>> {
        jobs.iter()
            .enumerate()
            .map(|(index, job)| NewJob {
                key: &job.job_id,
                name: &self.names[index],
                matrix: self.matrices[index].as_deref(),
                needs: &job.needs,
                runs_on: &job.runs_on,
                permissions: self.permissions[index].as_deref(),
                payload: job.payload.as_bytes(),
            })
            .collect()
    }
}

/// A job's `permissions:` as stored: [`TokenPermissions`] in JSON, which the scheduler
/// reads back to lower the token (`rbit_core::actions::token_permissions`).
pub fn permissions_json(permissions: Permissions) -> String {
    let level = |permission: Permission| match permission {
        Permission::None => PermissionLevel::None,
        Permission::Read => PermissionLevel::Read,
        Permission::Write => PermissionLevel::Write,
    };
    let token = TokenPermissions {
        contents: level(permissions.contents),
        issues: level(permissions.issues),
        pull_requests: level(permissions.pull_requests),
    };
    serde_json::to_string(&token).unwrap_or_else(|_| "{}".to_owned())
}

/// At most `max` characters of `text`.
pub fn truncate_chars(text: &str, max: usize) -> String {
    text.char_indices().nth(max).map_or(text, |(end, _)| &text[..end]).to_owned()
}

/// At most `max` bytes of `text`, cut on a character boundary.
pub fn truncate_bytes(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_cut_on_character_boundaries() {
        assert_eq!(truncate_chars("héllo", 2), "hé");
        assert_eq!(truncate_chars("hi", 10), "hi");
        assert_eq!(truncate_bytes("héllo", 2), "h");
        assert_eq!(truncate_bytes("héllo", 3), "hé");
        assert_eq!(truncate_bytes("hi", 10), "hi");
    }

    #[test]
    fn permissions_are_stored_as_token_permissions() {
        let declared = Permissions { contents: Permission::Read, issues: Permission::None, pull_requests: Permission::Write };
        let stored = permissions_json(declared);
        assert_eq!(stored, r#"{"contents":"read","issues":"none","pull_requests":"write"}"#);
        let read: TokenPermissions = serde_json::from_str(&stored).unwrap();
        assert_eq!(read.pull_requests, PermissionLevel::Write);
    }
}
