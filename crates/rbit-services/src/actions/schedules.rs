//! Scheduled runs: the schedule table, refreshed on default-branch pushes, and the ticker
//! that starts due runs.
//!
//! * The table holds the `schedule` entries of the workflows on the default branch: it is
//!   replaced whenever a push that may start runs moves the default branch, and on demand
//!   ([`ActionsService::refresh_default_branch_schedules`], for when the repository's switch
//!   is turned on). At most `[actions] max_schedules_per_repository` entries are kept, a
//!   repeated expression of one workflow counts once, and expressions firing more often
//!   than every 5 minutes are already workflow errors.
//! * Every [`TICK`] the ticker starts the entries that are due, with the default branch's
//!   head at that moment. An entry is skipped (and rescheduled) while the previous
//!   scheduled run of its workflow still waits for a runner, or while Actions is off for the
//!   repository. A workflow that no longer has the entry refreshes the table; one that fails
//!   to parse or plan records a workflow error, and [`PAUSE_AFTER_FAILURES`] of those in a
//!   row pause the entry until the table is next refreshed.
//! * Scheduled runs have no actor and post no commit statuses.
//! * [`ActionsService::tick`] takes the time it runs at, so tests drive it with any clock.
//!
//! Workstream T (triggers and runs) owns this file.

use std::{collections::BTreeSet, time::Duration};

use rbit_core::{
    RepoId, Timestamp,
    actions::{Run, RunEvent, RunTrust, ScheduleEntry, Status},
    repository::OwnedRepository,
};
use rbit_storage::sqlite::{actions_runs::RunFilter, actions_schedules::NewSchedule};
use rbit_workflow::{Event, GroupContext, cron::Cron};

use super::{
    ActionsService, context,
    runs::{Candidate, Trigger},
    triggers::{discover, plan_workflow, problem},
};
use crate::ServiceError;

/// How often the ticker looks for due entries.
pub const TICK: Duration = Duration::from_secs(60);
/// Consecutive workflow errors that pause an entry.
pub const PAUSE_AFTER_FAILURES: i64 = 3;
/// Entries started by one tick at most; the rest are due on the next.
const DUE_PER_TICK: u32 = 100;
/// When an entry whose expression can no longer be read is looked at again.
const RETRY_UNREADABLE: i64 = 24 * 60 * 60;

/// What firing one entry did.
#[derive(Debug)]
enum Fired {
    Started(Run),
    /// A workflow-error run was recorded.
    Failed(Run),
    /// Not this time: Actions is off, the previous run still waits, or the branch is gone.
    Skipped,
    /// The workflow no longer has the entry; the table was refreshed.
    Gone,
}

impl ActionsService {
    /// The ticker: every [`TICK`] until shutdown.
    pub(super) async fn run_schedules(self) {
        loop {
            tokio::select! {
                () = self.inner.shutdown.cancelled() => return,
                () = tokio::time::sleep(TICK) => {}
            }
            if let Err(error) = self.tick(Timestamp::now()).await {
                tracing::warn!(%error, "cannot start scheduled workflow runs");
            }
        }
    }

    /// Replaces `repo`'s schedule with the entries of its workflows at `sha`, the default
    /// branch's head; their next fire times count from `now`. Returns how many there are.
    pub async fn refresh_schedules(&self, repo: &OwnedRepository, sha: &str, now: Timestamp) -> Result<usize, ServiceError> {
        let reader = self.reader(repo);
        let (limits, sha) = (self.limits(), sha.to_owned());
        let found = self
            .evaluate(move |handle, deadline| {
                let files = discover(handle, &reader, &sha, &limits, deadline)?;
                let mut entries = BTreeSet::new();
                for file in &files {
                    if let Ok(workflow) = &file.parsed {
                        entries.extend(workflow.on.schedule.iter().map(|entry| (file.path.clone(), entry.cron.as_str().to_owned())));
                    }
                }
                Ok::<_, rbit_git::GitError>(entries)
            })
            .await??;
        let max = usize::try_from(self.config().max_schedules_per_repository).unwrap_or(usize::MAX);
        if found.len() > max {
            tracing::warn!(repo = %repo.full_name(), entries = found.len(), "only the first {max} schedule entries of a repository are kept");
        }
        let timed: Vec<(String, String, Timestamp)> = found
            .into_iter()
            .take(max)
            .filter_map(|(path, cron)| {
                let next = Cron::parse(&cron).ok()?.next_after(now.seconds())?;
                Some((path, cron, Timestamp(next)))
            })
            .collect();
        let entries: Vec<NewSchedule<'_>> =
            timed.iter().map(|(path, cron, next_at)| NewSchedule { workflow_path: path, cron, next_at: *next_at }).collect();
        self.ctx().database.actions_schedules().replace(repo.repo.id, &entries).await?;
        // A refreshed entry is no longer paused by earlier failures.
        self.ctx().database.actions_schedules().set_paused(repo.repo.id, false).await?;
        Ok(entries.len())
    }

    /// Refreshes `repo`'s schedule from its default branch now (when Actions is switched on
    /// for it). Without the branch, the schedule is emptied.
    pub async fn refresh_default_branch_schedules(&self, repo: RepoId) -> Result<usize, ServiceError> {
        let Some(owned) = self.ctx().database.admin().find_repository(repo).await? else { return Ok(0) };
        if let Ok(sha) = self.reader(&owned).commit_id(format!("refs/heads/{}", owned.repo.default_branch)).await {
            self.refresh_schedules(&owned, &sha, Timestamp::now()).await
        } else {
            self.ctx().database.actions_schedules().replace(repo, &[]).await?;
            Ok(0)
        }
    }

    /// Starts the entries due at `now` and returns the runs started (workflow errors
    /// included).
    pub async fn tick(&self, now: Timestamp) -> Result<Vec<Run>, ServiceError> {
        let store = self.ctx().database.actions_schedules();
        let mut started = Vec::new();
        for entry in store.due(now, DUE_PER_TICK).await? {
            let Some(next) = Cron::parse(&entry.cron).ok().and_then(|cron| cron.next_after(now.seconds())) else {
                store
                    .record_tick(entry.repo_id, &entry.workflow_path, &entry.cron, Timestamp(now.seconds() + RETRY_UNREADABLE), true, 1)
                    .await?;
                continue;
            };
            let next = Timestamp(next);
            let failed = match self.fire(&entry).await {
                Ok(Fired::Started(run)) => {
                    started.push(run);
                    false
                }
                Ok(Fired::Failed(run)) => {
                    started.push(run);
                    true
                }
                Ok(Fired::Skipped) => false,
                Ok(Fired::Gone) => continue,
                Err(error) => {
                    tracing::warn!(%error, repo = entry.repo_id.0, workflow = %entry.workflow_path, "cannot start a scheduled run");
                    false
                }
            };
            store.record_tick(entry.repo_id, &entry.workflow_path, &entry.cron, next, failed, PAUSE_AFTER_FAILURES).await?;
        }
        Ok(started)
    }

    async fn fire(&self, entry: &ScheduleEntry) -> Result<Fired, ServiceError> {
        let database = &self.ctx().database;
        if !database.actions_settings().is_enabled(entry.repo_id).await? {
            return Ok(Fired::Skipped);
        }
        let Some(owned) = database.admin().find_repository(entry.repo_id).await? else { return Ok(Fired::Skipped) };
        let filter =
            RunFilter { workflow_path: Some(&entry.workflow_path), event: Some(RunEvent::Schedule), limit: 1, ..RunFilter::default() };
        if let Some(previous) = database.actions_runs().list_runs(entry.repo_id, &filter).await?.first()
            && matches!(previous.status, Status::Waiting | Status::Blocked)
        {
            tracing::debug!(repo = %owned.full_name(), workflow = %entry.workflow_path, "the previous scheduled run still waits; skipped");
            return Ok(Fired::Skipped);
        }
        let reference = format!("refs/heads/{}", owned.repo.default_branch);
        let reader = self.reader(&owned);
        let Ok(sha) = reader.commit_id(reference.clone()).await else { return Ok(Fired::Skipped) };

        let (limits, at, path, cron) = (self.limits(), sha.clone(), entry.workflow_path.clone(), entry.cron.clone());
        // Nobody causes a scheduled run; it runs the default branch, so it is trusted.
        let planning = self.planning(&owned, RunEvent::Schedule, &reference, &sha, (None, None), true).await?;
        let group = GroupContext { schedule: entry.cron.clone(), ..planning };
        let candidate = self
            .evaluate(move |handle, deadline| {
                let files = discover(handle, &reader, &at, &limits, deadline).ok()?;
                let file = files.iter().find(|file| file.path == path)?;
                let outcome = match &file.parsed {
                    Err(error) => Err(problem(&file.path, error)),
                    Ok(workflow) if rbit_workflow::matches(workflow, &Event::Schedule { cron: &cron }, &mut None::<Vec<String>>) => {
                        plan_workflow(workflow, &file.name(), &file.path, &group, &limits, deadline)
                            .map_err(|error| problem(&file.path, &error))
                    }
                    Ok(_) => return None,
                };
                Some(Candidate { path: file.path.clone(), name: file.name(), outcome })
            })
            .await?;
        let Some(candidate) = candidate else {
            self.refresh_schedules(&owned, &sha, Timestamp::now()).await?;
            return Ok(Fired::Gone);
        };
        let fork = database.forks().forks_among(&[owned.repo.id]).await?.contains(&owned.repo.id);
        let payload = context::fit_payload(context::schedule_event(&owned, fork, &entry.cron, self.ctx().config.server.base_url()));
        let trigger = Trigger {
            repo: &owned,
            event: RunEvent::Schedule,
            reference: &reference,
            sha: &sha,
            title: "Scheduled",
            actor: None,
            trigger_actor: None,
            pull_number: None,
            is_fork: false,
            trust: RunTrust::Trusted,
            payload: &payload,
            inputs: None,
        };
        let mut allowance = self.allowance();
        Ok(match self.start_run(&trigger, candidate, &mut allowance).await? {
            Some(run) if run.is_workflow_error() => Fired::Failed(run),
            Some(run) => Fired::Started(run),
            None => Fired::Skipped,
        })
    }
}
