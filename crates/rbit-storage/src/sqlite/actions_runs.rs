//! Workflow runs, their jobs, the tasks runners execute and their steps (migration
//! `0048_actions.sql`) — and the run state machine, each transition in one transaction:
//!
//! * [`ActionsRunStore::create_run`] numbers the run and creates its jobs: with
//!   [`RunStart::Runnable`] jobs without `needs` are waiting and the others blocked; a run
//!   held for its concurrency group has every job blocked, an untrusted run awaiting
//!   approval every job awaiting. A run without jobs is a workflow error and has failed.
//! * [`ActionsRunStore::claim`] hands the oldest waiting job a runner may take to it: its
//!   labels are matched in SQL against the job's (`actions_job_labels`), its scope against
//!   the repository, and the job becomes running with a new task in the same transaction.
//! * [`ActionsRunStore::finish_task`], [`ActionsRunStore::cancel_run`],
//!   [`ActionsRunStore::release_run`], [`ActionsRunStore::approve_run`] and
//!   [`ActionsRunStore::rerun`] move jobs, then *settle* the run: every blocked job whose
//!   needs have all finished becomes waiting (whatever their result: the runner's `if:`
//!   decides), and the run's status is recomputed with `rbit_core::actions::aggregate`.
//!
//! Every query a page can reach takes the repository, so an id from another repository
//! finds nothing.
//!
//! The Actions foundation (workstream F) owns this file.

use std::collections::BTreeSet;

use rbit_core::{
    AccountId, RepoId, Timestamp,
    actions::{
        Job, JobId, JobStatus, LogState, Run, RunEvent, RunId, RunStatus, RunnerId, RunnerLabel, RunnerScope, Status, Step, Task, TaskId,
        aggregate,
    },
};
use sqlx::{SqliteConnection, SqlitePool};

use super::actions_rows::{corrupt, labels_json, runner_columns, status, strings, strings_json, timestamp};
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    id: i64,
    repo_id: i64,
    run_number: i64,
    workflow_path: String,
    workflow_name: String,
    title: String,
    event: String,
    r#ref: String,
    sha: String,
    actor_id: Option<i64>,
    trigger_actor_id: Option<i64>,
    pull_number: Option<i64>,
    is_fork: bool,
    trusted: bool,
    status: i64,
    attempt: i64,
    concurrency_group: Option<String>,
    cancel_in_progress: bool,
    error: Option<String>,
    approved_by: Option<i64>,
    approved_at: Option<i64>,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    logs_expired_at: Option<i64>,
    awaiting_review: bool,
}

impl TryFrom<RunRow> for Run {
    type Error = StorageError;

    fn try_from(row: RunRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: RunId(row.id),
            repo_id: RepoId(row.repo_id),
            number: row.run_number,
            workflow_path: row.workflow_path,
            workflow_name: row.workflow_name,
            title: row.title,
            event: RunEvent::parse(&row.event).ok_or_else(|| corrupt("run event", &row.event))?,
            reference: row.r#ref,
            sha: row.sha,
            actor: row.actor_id.map(AccountId),
            trigger_actor: row.trigger_actor_id.map(AccountId),
            pull_number: row.pull_number,
            is_fork: row.is_fork,
            trusted: row.trusted,
            status: status(row.status)?,
            attempt: row.attempt,
            concurrency_group: row.concurrency_group,
            cancel_in_progress: row.cancel_in_progress,
            error: row.error,
            approved_by: row.approved_by.map(AccountId),
            approved_at: timestamp(row.approved_at),
            created_at: Timestamp(row.created_at),
            started_at: timestamp(row.started_at),
            finished_at: timestamp(row.finished_at),
            logs_expired_at: timestamp(row.logs_expired_at),
            awaiting_review: row.awaiting_review,
        })
    }
}

macro_rules! select_run {
    ($clause:expr) => {
        concat!(
            "SELECT id, repo_id, run_number, workflow_path, workflow_name, title, event, ref, sha, actor_id, trigger_actor_id, \
             pull_number, is_fork, trusted, status, attempt, concurrency_group, cancel_in_progress, error, approved_by, approved_at, \
             created_at, started_at, finished_at, logs_expired_at,              EXISTS (SELECT 1 FROM actions_jobs j WHERE j.run_id = actions_runs.id AND j.review = 1 AND j.status = 8)              AND NOT EXISTS (SELECT 1 FROM actions_jobs j WHERE j.run_id = actions_runs.id AND j.status IN (5, 6)) AS awaiting_review              FROM actions_runs ",
            $clause
        )
    };
}

// Actions: integration (I).
/// The `WHERE` clause of a [`RunFilter`] for repository `?1`; [`bind_run_filter!`] binds
/// `?1` to `?8`.
macro_rules! runs_where {
    () => {
        "WHERE repo_id = ?1 AND (?2 IS NULL OR workflow_path = ?2) AND (?3 IS NULL OR event = ?3) AND (?4 IS NULL OR status = ?4) \
         AND (?5 IS NULL OR actor_id = ?5) AND (?6 IS NULL OR ref = ?6) AND (?7 IS NULL OR id < ?7) \
         AND (?8 IS NULL OR status IN (SELECT value FROM json_each(?8)))"
    };
}

/// Binds the parameters of [`runs_where!`] to `$query`.
macro_rules! bind_run_filter {
    ($query:expr, $repo:expr, $filter:expr) => {
        $query
            .bind($repo.0)
            .bind($filter.workflow_path)
            .bind($filter.event.map(RunEvent::as_str))
            .bind($filter.status.map(Status::code))
            .bind($filter.actor.map(|actor| actor.0))
            .bind($filter.reference)
            .bind($filter.before.map(|run| run.0))
            .bind(status_codes($filter.statuses))
    };
}

#[derive(Debug, sqlx::FromRow)]
struct JobRow {
    id: i64,
    run_id: i64,
    repo_id: i64,
    job_key: String,
    name: String,
    matrix: Option<String>,
    needs: String,
    runs_on: String,
    permissions: Option<String>,
    status: i64,
    attempt: i64,
    task_id: Option<i64>,
    runner_id: Option<i64>,
    created_at: i64,
    queued_at: Option<i64>,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    environment: Option<String>,
    environment_url: Option<String>,
    review: bool,
}

impl TryFrom<JobRow> for Job {
    type Error = StorageError;

    fn try_from(row: JobRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: JobId(row.id),
            run_id: RunId(row.run_id),
            repo_id: RepoId(row.repo_id),
            key: row.job_key,
            name: row.name,
            matrix: row.matrix,
            needs: strings(&row.needs)?,
            runs_on: strings(&row.runs_on)?,
            permissions: row.permissions,
            status: status(row.status)?,
            attempt: row.attempt,
            task_id: row.task_id.map(TaskId),
            runner_id: row.runner_id.map(RunnerId),
            created_at: Timestamp(row.created_at),
            queued_at: timestamp(row.queued_at),
            started_at: timestamp(row.started_at),
            finished_at: timestamp(row.finished_at),
            environment: row.environment,
            environment_url: row.environment_url,
            awaiting_review: row.review && row.status == Status::AwaitingApproval.code(),
        })
    }
}

/// Job columns in [`JobRow`] order; `runs_on` is rebuilt from the label table.
macro_rules! select_job {
    ($clause:expr) => {
        concat!(
            "SELECT j.id, j.run_id, j.repo_id, j.job_key, j.name, j.matrix, j.needs, \
             (SELECT COALESCE(json_group_array(l.label), '[]') FROM actions_job_labels l WHERE l.job_id = j.id) AS runs_on, \
             j.permissions, j.status, j.attempt, j.task_id, j.runner_id, j.created_at, j.queued_at, j.started_at, j.finished_at, \
             j.environment, j.environment_url, j.review FROM actions_jobs j ",
            $clause
        )
    };
}

#[derive(Debug, sqlx::FromRow)]
struct TaskRow {
    id: i64,
    job_id: i64,
    attempt: i64,
    runner_id: Option<i64>,
    status: i64,
    started_at: i64,
    stopped_at: Option<i64>,
    heartbeat_at: i64,
    log_lines: i64,
    log_bytes: i64,
    log_state: String,
    log_truncated: bool,
}

impl TryFrom<TaskRow> for Task {
    type Error = StorageError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: TaskId(row.id),
            job_id: JobId(row.job_id),
            attempt: row.attempt,
            runner_id: row.runner_id.map(RunnerId),
            status: status(row.status)?,
            started_at: Timestamp(row.started_at),
            stopped_at: timestamp(row.stopped_at),
            heartbeat_at: Timestamp(row.heartbeat_at),
            log_lines: row.log_lines,
            log_bytes: row.log_bytes,
            log_state: LogState::parse(&row.log_state).ok_or_else(|| corrupt("log state", &row.log_state))?,
            log_truncated: row.log_truncated,
        })
    }
}

macro_rules! select_task {
    ($clause:expr) => {
        concat!(
            "SELECT id, job_id, attempt, runner_id, status, started_at, stopped_at, heartbeat_at, log_lines, log_bytes, log_state, \
             log_truncated FROM actions_tasks ",
            $clause
        )
    };
}

#[derive(Debug, sqlx::FromRow)]
struct StepRow {
    task_id: i64,
    number: i64,
    name: String,
    status: i64,
    started_at: Option<i64>,
    stopped_at: Option<i64>,
    log_index: i64,
    log_length: i64,
}

impl TryFrom<StepRow> for Step {
    type Error = StorageError;

    fn try_from(row: StepRow) -> Result<Self, Self::Error> {
        Ok(Self {
            task_id: TaskId(row.task_id),
            number: row.number,
            name: row.name,
            status: status(row.status)?,
            started_at: timestamp(row.started_at),
            stopped_at: timestamp(row.stopped_at),
            log_index: row.log_index,
            log_length: row.log_length,
        })
    }
}

/// `statuses` as the JSON array `?8` of [`runs_where!`] takes; `None` when empty.
fn status_codes(statuses: &[Status]) -> Option<String> {
    (!statuses.is_empty()).then(|| format!("[{}]", statuses.iter().map(|status| status.code().to_string()).collect::<Vec<_>>().join(",")))
}

/// How a new or re-run run starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStart {
    /// Jobs without `needs` wait for a runner, the others are blocked on their needs.
    Runnable,
    /// Every job is blocked until [`ActionsRunStore::release_run`]: the concurrency group is busy.
    HeldForConcurrency,
    /// Every job awaits approval ([`ActionsRunStore::approve_run`]).
    AwaitingApproval,
}

/// A run to create.
#[derive(Debug, Clone)]
pub struct NewRun<'a> {
    pub repo: RepoId,
    pub workflow_path: &'a str,
    pub workflow_name: &'a str,
    pub title: &'a str,
    pub event: RunEvent,
    /// The full ref.
    pub reference: &'a str,
    pub sha: &'a str,
    pub actor: Option<AccountId>,
    pub trigger_actor: Option<AccountId>,
    pub pull_number: Option<i64>,
    pub is_fork: bool,
    pub trusted: bool,
    pub concurrency_group: Option<&'a str>,
    pub cancel_in_progress: bool,
    /// The GitHub-shaped event payload (JSON, at most 256 KiB).
    pub event_payload: &'a str,
    /// The checked dispatch inputs (a JSON object).
    pub dispatch_inputs: Option<&'a str>,
    pub start: RunStart,
    /// Empty for a workflow error, which needs `error`.
    pub jobs: &'a [NewJob<'a>],
    pub error: Option<&'a str>,
    pub now: Timestamp,
}

/// A job of a new run.
#[derive(Debug, Clone)]
pub struct NewJob<'a> {
    pub key: &'a str,
    pub name: &'a str,
    /// The matrix combination as a JSON object.
    pub matrix: Option<&'a str>,
    pub needs: &'a [String],
    pub runs_on: &'a [String],
    /// The effective `permissions:` as a JSON object.
    pub permissions: Option<&'a str>,
    pub payload: &'a [u8],
    // Actions: join (J).
    /// The job has its own `if:`: it stays blocked until [`ActionsRunStore::decide`].
    pub condition: bool,
    /// `strategy.max-parallel`.
    pub max_parallel: Option<u32>,
    /// Job-level `concurrency.group`.
    pub group: Option<&'a str>,
    // Actions: environments.
    /// The deployment environment, by name: created for the repository if it has none.
    pub environment: Option<&'a str>,
}

/// Filters of the runs list. `before` pages by id (newest first), `offset` by position
/// (the API's pages).
#[derive(Debug, Clone, Default)]
pub struct RunFilter<'a> {
    pub workflow_path: Option<&'a str>,
    pub event: Option<RunEvent>,
    pub status: Option<RunStatus>,
    pub actor: Option<AccountId>,
    /// The full ref.
    pub reference: Option<&'a str>,
    pub before: Option<RunId>,
    pub limit: u32,
    // Actions: integration (I).
    /// One of these statuses, when not empty (a GitHub status such as `completed` stands
    /// for several); applies together with `status`.
    pub statuses: &'a [RunStatus],
    /// Runs skipped after the filters apply.
    pub offset: u32,
}

/// A runner asking for work.
#[derive(Debug, Clone)]
pub struct ClaimRequest<'a> {
    pub runner: RunnerId,
    pub scope: RunnerScope,
    pub labels: &'a [RunnerLabel],
    /// An ephemeral runner takes one task at most.
    pub ephemeral: bool,
    /// `[actions] instance_runners_for_all`.
    pub instance_runners_for_all: bool,
    /// The digest of the task token minted for the task.
    pub token_digest: &'a [u8; 32],
    /// The poll's `Idempotency-Key`: a retried poll finds the task by it.
    pub idempotency_key: Option<&'a str>,
    pub now: Timestamp,
}

/// A job a runner claimed and the task created for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    pub job: Job,
    pub task: Task,
}

/// The state of a step as a runner reports it.
#[derive(Debug, Clone)]
pub struct StepUpdate<'a> {
    pub number: i64,
    pub name: &'a str,
    pub status: Status,
    pub started_at: Option<Timestamp>,
    pub stopped_at: Option<Timestamp>,
    pub log_index: i64,
    pub log_length: i64,
}

/// What a transition changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    pub run_id: RunId,
    pub run_status: RunStatus,
    /// Jobs that became waiting: the scheduler's version must move.
    pub now_waiting: Vec<JobId>,
    // Actions: environments.
    /// Jobs that would have become waiting but wait for a reviewer of their environment.
    pub now_reviewing: Vec<JobId>,
    // Actions: join (J).
    /// Jobs whose needs are done and whose own `if:` the caller must now decide.
    pub to_decide: Vec<JobId>,
}

// Actions: join (J).
/// How a job's own `if:` came out (see [`ActionsRunStore::decide`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// True: the job waits for a runner.
    Run,
    /// False: the job is skipped, like one whose needs failed.
    Skip,
    /// The condition could not be evaluated: the job fails, as on GitHub.
    Fail,
}

/// Which jobs a re-run starts again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rerun {
    All,
    /// Failed and cancelled jobs, and the jobs that need them.
    Failed,
    /// One job, and the jobs that need it.
    Job(JobId),
}

#[derive(Debug, Clone)]
pub struct ActionsRunStore {
    pool: SqlitePool,
}

impl ActionsRunStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Creates a run with its next number and its jobs.
    pub async fn create_run(&self, new: &NewRun<'_>) -> Result<Run, StorageError> {
        if new.jobs.is_empty() != new.error.is_some() {
            return Err(corrupt("new run", "a run has jobs, or an error instead"));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let number: i64 = sqlx::query_scalar(
            "INSERT INTO actions_run_numbers (repo_id, last) VALUES (?, 1) \
             ON CONFLICT (repo_id) DO UPDATE SET last = last + 1 RETURNING last",
        )
        .bind(new.repo.0)
        .fetch_one(&mut *tx)
        .await?;
        let failed = new.error.is_some();
        let run_id: i64 = sqlx::query_scalar(
            "INSERT INTO actions_runs (repo_id, run_number, workflow_path, workflow_name, title, event, ref, sha, actor_id, \
             trigger_actor_id, pull_number, is_fork, trusted, status, concurrency_group, cancel_in_progress, event_payload, \
             dispatch_inputs, error, created_at, finished_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21) RETURNING id",
        )
        .bind(new.repo.0)
        .bind(number)
        .bind(new.workflow_path)
        .bind(new.workflow_name)
        .bind(new.title)
        .bind(new.event.as_str())
        .bind(new.reference)
        .bind(new.sha)
        .bind(new.actor.map(|id| id.0))
        .bind(new.trigger_actor.map(|id| id.0))
        .bind(new.pull_number)
        .bind(new.is_fork)
        .bind(new.trusted)
        .bind(if failed { Status::Failure } else { placeholder_status(new.start) }.code())
        .bind(new.concurrency_group)
        .bind(new.cancel_in_progress)
        .bind(new.event_payload)
        .bind(new.dispatch_inputs)
        .bind(new.error)
        .bind(new.now.0)
        .bind(failed.then_some(new.now.0))
        .fetch_one(&mut *tx)
        .await?;
        let mut waiting = Vec::new();
        for job in new.jobs {
            // A job with its own `if:` waits for the server's decision like one with needs.
            let status = initial_status(new.start, !job.needs.is_empty() || job.condition);
            let job_id: i64 = sqlx::query_scalar(
                "INSERT INTO actions_jobs (run_id, repo_id, job_key, name, matrix, needs, permissions, payload, status, created_at, \
                 queued_at, condition, max_parallel, job_group, environment) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) RETURNING id",
            )
            .bind(run_id)
            .bind(new.repo.0)
            .bind(job.key)
            .bind(job.name)
            .bind(job.matrix)
            .bind(strings_json(job.needs)?)
            .bind(job.permissions)
            .bind(job.payload)
            .bind(status.code())
            .bind(new.now.0)
            .bind((status == Status::Waiting).then_some(new.now.0))
            .bind(job.condition)
            .bind(job.max_parallel)
            .bind(job.group)
            .bind(job.environment)
            .fetch_one(&mut *tx)
            .await?;
            insert_labels(&mut tx, job_id, job.runs_on).await?;
            // Actions: environments. Named for the first time, an environment exists with no
            // protection, as on GitHub, so it can be configured; past the cap of a repository
            // it is not created (and so has no protection to apply either).
            if let Some(environment) = job.environment {
                sqlx::query(
                    "INSERT INTO actions_environments (repo_id, name, created_at, updated_at) SELECT ?1, ?2, ?3, ?3                      WHERE (SELECT COUNT(*) FROM actions_environments WHERE repo_id = ?1) < ?4 ON CONFLICT DO NOTHING",
                )
                .bind(new.repo.0)
                .bind(environment)
                .bind(new.now.0)
                .bind(i64::try_from(super::actions_environments::MAX_ENVIRONMENTS).unwrap_or(i64::MAX))
                .execute(&mut *tx)
                .await?;
            }
            if status == Status::Waiting {
                waiting.push(job_id);
            }
        }
        if !failed {
            settle(&mut tx, RunId(run_id), waiting, new.now).await?;
        }
        let row: RunRow = sqlx::query_as(select_run!("WHERE id = ?")).bind(run_id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Run::try_from(row)
    }

    /// Run `id`, only if it belongs to `repo`.
    pub async fn find_run(&self, repo: RepoId, id: RunId) -> Result<Option<Run>, StorageError> {
        let row: Option<RunRow> =
            sqlx::query_as(select_run!("WHERE id = ? AND repo_id = ?")).bind(id.0).bind(repo.0).fetch_optional(&self.pool).await?;
        row.map(Run::try_from).transpose()
    }

    /// Run `#number` of `repo`.
    pub async fn find_run_by_number(&self, repo: RepoId, number: i64) -> Result<Option<Run>, StorageError> {
        let row: Option<RunRow> = sqlx::query_as(select_run!("WHERE repo_id = ? AND run_number = ?"))
            .bind(repo.0)
            .bind(number)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Run::try_from).transpose()
    }

    /// Run `id` wherever it is (for runner calls, which carry no repository).
    pub async fn run(&self, id: RunId) -> Result<Option<Run>, StorageError> {
        let row: Option<RunRow> = sqlx::query_as(select_run!("WHERE id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(Run::try_from).transpose()
    }

    /// The repository's runs, newest first.
    pub async fn list_runs(&self, repo: RepoId, filter: &RunFilter<'_>) -> Result<Vec<Run>, StorageError> {
        let query = sqlx::query_as(select_run!(concat!(runs_where!(), " ORDER BY id DESC LIMIT ?9 OFFSET ?10")));
        let rows: Vec<RunRow> = bind_run_filter!(query, repo, filter).bind(filter.limit).bind(filter.offset).fetch_all(&self.pool).await?;
        rows.into_iter().map(Run::try_from).collect()
    }

    // Actions: integration (I).
    /// How many runs of the repository [`Self::list_runs`] pages through with `filter`
    /// (its `limit` and `offset` aside).
    pub async fn count_runs(&self, repo: RepoId, filter: &RunFilter<'_>) -> Result<u64, StorageError> {
        let query = sqlx::query_scalar(concat!("SELECT COUNT(*) FROM actions_runs ", runs_where!()));
        let count: i64 = bind_run_filter!(query, repo, filter).fetch_one(&self.pool).await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    // Actions: step summaries.
    /// Keeps the summaries of `task`'s steps it does not have yet, by step number; steps the
    /// task was not created with are ignored.
    pub async fn add_summaries(&self, task: TaskId, summaries: &[(i64, &str)]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for (number, summary) in summaries {
            sqlx::query(
                "INSERT OR IGNORE INTO actions_step_summaries (task_id, number, summary) \
                 SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM actions_steps WHERE task_id = ?1 AND number = ?2)",
            )
            .bind(task.0)
            .bind(number)
            .bind(summary)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The step summaries of `tasks`: task, step number and text, by task and number.
    pub async fn summaries_of_tasks(&self, tasks: &[TaskId]) -> Result<Vec<(TaskId, i64, String)>, StorageError> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let ids = format!("[{}]", tasks.iter().map(|task| task.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<(i64, i64, String)> = sqlx::query_as(
            "SELECT task_id, number, summary FROM actions_step_summaries \
             WHERE task_id IN (SELECT value FROM json_each(?)) ORDER BY task_id, number",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(task, number, summary)| (TaskId(task), number, summary)).collect())
    }

    /// The steps of `tasks`, by task and number.
    pub async fn steps_of_tasks(&self, tasks: &[TaskId]) -> Result<Vec<Step>, StorageError> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let ids = format!("[{}]", tasks.iter().map(|task| task.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<StepRow> = sqlx::query_as(
            "SELECT task_id, number, name, status, started_at, stopped_at, log_index, log_length FROM actions_steps \
             WHERE task_id IN (SELECT value FROM json_each(?)) ORDER BY task_id, number",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Step::try_from).collect()
    }

    /// The run's event payload (JSON) and dispatch inputs.
    pub async fn event(&self, run: RunId) -> Result<Option<(String, Option<String>)>, StorageError> {
        Ok(sqlx::query_as("SELECT event_payload, dispatch_inputs FROM actions_runs WHERE id = ?")
            .bind(run.0)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// The runs of `repo` in concurrency group `group` that have not finished, oldest first.
    pub async fn unfinished_in_group(&self, repo: RepoId, group: &str) -> Result<Vec<Run>, StorageError> {
        let rows: Vec<RunRow> =
            sqlx::query_as(select_run!("WHERE repo_id = ? AND concurrency_group = ? AND status IN (5, 6, 7, 8) ORDER BY id"))
                .bind(repo.0)
                .bind(group)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(Run::try_from).collect()
    }

    /// The pull request's runs that have not finished, oldest first.
    pub async fn unfinished_for_pull(&self, repo: RepoId, pull_number: i64) -> Result<Vec<Run>, StorageError> {
        let rows: Vec<RunRow> = sqlx::query_as(select_run!("WHERE repo_id = ? AND pull_number = ? AND status IN (5, 6, 7, 8) ORDER BY id"))
            .bind(repo.0)
            .bind(pull_number)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Run::try_from).collect()
    }

    /// The jobs of a run, in creation order.
    pub async fn jobs(&self, run: RunId) -> Result<Vec<Job>, StorageError> {
        let rows: Vec<JobRow> = sqlx::query_as(select_job!("WHERE j.run_id = ? ORDER BY j.id")).bind(run.0).fetch_all(&self.pool).await?;
        rows.into_iter().map(Job::try_from).collect()
    }

    /// Job `id`, only if it belongs to `repo`.
    pub async fn find_job(&self, repo: RepoId, id: JobId) -> Result<Option<Job>, StorageError> {
        let row: Option<JobRow> =
            sqlx::query_as(select_job!("WHERE j.id = ? AND j.repo_id = ?")).bind(id.0).bind(repo.0).fetch_optional(&self.pool).await?;
        row.map(Job::try_from).transpose()
    }

    /// Job `id` wherever it is (for runner calls).
    pub async fn job(&self, id: JobId) -> Result<Option<Job>, StorageError> {
        let row: Option<JobRow> = sqlx::query_as(select_job!("WHERE j.id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(Job::try_from).transpose()
    }

    /// The single-job workflow handed to the runner.
    pub async fn job_payload(&self, id: JobId) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(sqlx::query_scalar("SELECT payload FROM actions_jobs WHERE id = ?").bind(id.0).fetch_optional(&self.pool).await?)
    }

    /// The job's outputs, by key.
    pub async fn outputs(&self, id: JobId) -> Result<Vec<(String, String)>, StorageError> {
        Ok(sqlx::query_as("SELECT key, value FROM actions_job_outputs WHERE job_id = ? ORDER BY key")
            .bind(id.0)
            .fetch_all(&self.pool)
            .await?)
    }

    /// Stores outputs whose keys are new, as long as the job's outputs stay within
    /// `max_total_bytes`; later values of a stored key are ignored. Returns every stored key.
    pub async fn add_outputs(&self, id: JobId, outputs: &[(&str, &str)], max_total_bytes: u64) -> Result<Vec<String>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(length(CAST(key AS BLOB)) + length(CAST(value AS BLOB))), 0) FROM actions_job_outputs WHERE job_id = ?",
        )
        .bind(id.0)
        .fetch_one(&mut *tx)
        .await?;
        let budget = i64::try_from(max_total_bytes).unwrap_or(i64::MAX);
        for (key, value) in outputs {
            let size = i64::try_from(key.len() + value.len()).unwrap_or(i64::MAX);
            if total.saturating_add(size) > budget {
                continue;
            }
            let inserted = sqlx::query("INSERT INTO actions_job_outputs (job_id, key, value) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
                .bind(id.0)
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await?;
            if inserted.rows_affected() == 1 {
                total += size;
            }
        }
        let keys =
            sqlx::query_scalar("SELECT key FROM actions_job_outputs WHERE job_id = ? ORDER BY key").bind(id.0).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(keys)
    }

    /// Waiting jobs of `repo` (the per-repository cap).
    pub async fn count_waiting(&self, repo: RepoId) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actions_jobs WHERE repo_id = ? AND status = 5")
            .bind(repo.0)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// Waiting jobs on the whole instance (the `rbit_actions_jobs_waiting` gauge).
    pub async fn count_all_waiting(&self) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actions_jobs WHERE status = 5").fetch_one(&self.pool).await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// Gives the runner the oldest waiting job it may take, or `None`. The job becomes
    /// running and gets a new task holding `token_digest`, in one write transaction: of two
    /// runners claiming at once, one gets the job.
    pub async fn claim(&self, request: &ClaimRequest<'_>) -> Result<Option<Claimed>, StorageError> {
        let (kind, owner, repo) = runner_columns(request.scope);
        let labels = labels_json(request.labels)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if request.ephemeral {
            let busy: Option<i64> = sqlx::query_scalar("SELECT 1 FROM actions_tasks WHERE runner_id = ? AND status = 6 LIMIT 1")
                .bind(request.runner.0)
                .fetch_optional(&mut *tx)
                .await?;
            if busy.is_some() {
                return Ok(None);
            }
        }
        let candidate: Option<i64> = sqlx::query_scalar(CLAIM_CANDIDATE)
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .bind(request.instance_runners_for_all)
            .bind(labels)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(job_id) = candidate else {
            return Ok(None);
        };
        let attempt: i64 = sqlx::query_scalar(
            "UPDATE actions_jobs SET status = 6, runner_id = ?1, started_at = ?2 WHERE id = ?3 AND status = 5 RETURNING attempt",
        )
        .bind(request.runner.0)
        .bind(request.now.0)
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;
        let task_id: i64 = sqlx::query_scalar(
            "INSERT INTO actions_tasks (job_id, attempt, runner_id, token_digest, idempotency_key, status, started_at, heartbeat_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 6, ?6, ?6) RETURNING id",
        )
        .bind(job_id)
        .bind(attempt)
        .bind(request.runner.0)
        .bind(&request.token_digest[..])
        .bind(request.idempotency_key)
        .bind(request.now.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| StorageError::from_insert(error, "task idempotency key"))?;
        sqlx::query("UPDATE actions_jobs SET task_id = ? WHERE id = ?").bind(task_id).bind(job_id).execute(&mut *tx).await?;
        let run_id: i64 = sqlx::query_scalar("SELECT run_id FROM actions_jobs WHERE id = ?").bind(job_id).fetch_one(&mut *tx).await?;
        settle(&mut tx, RunId(run_id), Vec::new(), request.now).await?;
        let job: JobRow = sqlx::query_as(select_job!("WHERE j.id = ?")).bind(job_id).fetch_one(&mut *tx).await?;
        let task: TaskRow = sqlx::query_as(select_task!("WHERE id = ?")).bind(task_id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(Claimed { job: Job::try_from(job)?, task: Task::try_from(task)? }))
    }

    pub async fn task(&self, id: TaskId) -> Result<Option<Task>, StorageError> {
        let row: Option<TaskRow> = sqlx::query_as(select_task!("WHERE id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(Task::try_from).transpose()
    }

    /// The task a runner was given for idempotency key `key` (a retried poll).
    pub async fn task_by_idempotency_key(&self, runner: RunnerId, key: &str) -> Result<Option<Task>, StorageError> {
        let row: Option<TaskRow> = sqlx::query_as(select_task!("WHERE runner_id = ? AND idempotency_key = ?"))
            .bind(runner.0)
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Task::try_from).transpose()
    }

    /// The running task whose token has `digest`, and its job. Tokens of stopped or
    /// cancelled tasks are cleared, so they find nothing.
    pub async fn running_task_by_token(&self, digest: &[u8; 32]) -> Result<Option<(Task, Job)>, StorageError> {
        let row: Option<TaskRow> =
            sqlx::query_as(select_task!("WHERE token_digest = ? AND status = 6")).bind(&digest[..]).fetch_optional(&self.pool).await?;
        let Some(task) = row.map(Task::try_from).transpose()? else {
            return Ok(None);
        };
        Ok(self.job(task.job_id).await?.map(|job| (task, job)))
    }

    /// Replaces the token of a running task (a retried poll gets a fresh token).
    pub async fn replace_task_token(&self, id: TaskId, digest: &[u8; 32]) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE actions_tasks SET token_digest = ? WHERE id = ? AND status = 6 AND token_digest IS NOT NULL")
            .bind(&digest[..])
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Records a heartbeat of a running task.
    pub async fn heartbeat(&self, id: TaskId, now: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE actions_tasks SET heartbeat_at = MAX(heartbeat_at, ?) WHERE id = ? AND status = 6")
            .bind(now.0)
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    // Actions: S.
    /// Creates the steps of a newly claimed task, waiting, named in order from 0.
    pub async fn create_steps(&self, task: TaskId, names: &[String]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for (number, name) in names.iter().enumerate() {
            sqlx::query("INSERT INTO actions_steps (task_id, number, name, status) VALUES (?, ?, ?, 5) ON CONFLICT DO NOTHING")
                .bind(task.0)
                .bind(i64::try_from(number).unwrap_or(i64::MAX))
                .bind(name)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Updates the state of existing steps of a task, by number (their names stay as
    /// created); numbers without a step are ignored.
    pub async fn update_steps(&self, task: TaskId, steps: &[StepUpdate<'_>]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for step in steps {
            sqlx::query(
                "UPDATE actions_steps SET status = ?1, started_at = ?2, stopped_at = ?3, log_index = ?4, log_length = ?5 \
                 WHERE task_id = ?6 AND number = ?7",
            )
            .bind(step.status.code())
            .bind(step.started_at.map(|at| at.0))
            .bind(step.stopped_at.map(|at| at.0))
            .bind(step.log_index)
            .bind(step.log_length)
            .bind(task.0)
            .bind(step.number)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    // Actions: environments.
    /// Records a reviewer's decision on jobs of `run` waiting for their environment: approved
    /// jobs wait for a runner (their attempt approved), rejected ones fail. Only jobs still
    /// waiting for review count; `None` when none of `jobs` was.
    pub async fn review_jobs(
        &self,
        run: RunId,
        jobs: &[JobId],
        reviewer: AccountId,
        decision: rbit_core::actions::ReviewDecision,
        comment: &str,
        now: Timestamp,
    ) -> Result<Option<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut decided = 0;
        for job in jobs {
            let row: Option<(i64, String)> = sqlx::query_as(
                "SELECT attempt, environment FROM actions_jobs WHERE id = ? AND run_id = ? AND status = 8 AND review = 1 AND environment IS NOT NULL",
            )
            .bind(job.0)
            .bind(run.0)
            .fetch_optional(&mut *tx)
            .await?;
            let Some((attempt, environment)) = row else { continue };
            let update = match decision {
                rbit_core::actions::ReviewDecision::Approved => {
                    "UPDATE actions_jobs SET status = 5, review = 0, approved_attempt = attempt, queued_at = ?2 WHERE id = ?1"
                }
                rbit_core::actions::ReviewDecision::Rejected => {
                    "UPDATE actions_jobs SET status = 2, review = 0, finished_at = ?2 WHERE id = ?1"
                }
            };
            sqlx::query(update).bind(job.0).bind(now.0).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO actions_deployment_reviews (job_id, attempt, environment, reviewer_id, decision, comment, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(job.0)
            .bind(attempt)
            .bind(&environment)
            .bind(reviewer.0)
            .bind(decision.as_str())
            .bind(comment)
            .bind(now.0)
            .execute(&mut *tx)
            .await?;
            decided += 1;
        }
        if decided == 0 {
            return Ok(None);
        }
        let waiting =
            if decision == rbit_core::actions::ReviewDecision::Approved { jobs.iter().map(|job| job.0).collect() } else { Vec::new() };
        let settled = settle(&mut tx, run, waiting, now).await?;
        tx.commit().await?;
        Ok(Some(settled))
    }

    /// Brings the jobs of `repo` in line with its environments' reviewers, after they
    /// changed: jobs waiting for review run when their environment no longer has reviewers
    /// (or no longer exists), and queued jobs no runner took yet wait for review when it now
    /// has some. Settles their runs.
    pub async fn release_reviews(&self, repo: RepoId, now: Timestamp) -> Result<Vec<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let released: Vec<(i64, i64)> = sqlx::query_as(
            "UPDATE actions_jobs SET status = 5, review = 0, queued_at = ?2 WHERE repo_id = ?1 AND status = 8 AND review = 1              AND NOT EXISTS (SELECT 1 FROM actions_environments e JOIN actions_environment_reviewers r ON r.environment_id = e.id              WHERE e.repo_id = actions_jobs.repo_id AND e.name = actions_jobs.environment) RETURNING id, run_id",
        )
        .bind(repo.0)
        .bind(now.0)
        .fetch_all(&mut *tx)
        .await?;
        // `settle` holds the queued ones back.
        let held: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT run_id FROM actions_jobs WHERE repo_id = ?1 AND status = 5 AND environment IS NOT NULL              AND (approved_attempt IS NULL OR approved_attempt <> attempt) AND EXISTS              (SELECT 1 FROM actions_environments e JOIN actions_environment_reviewers r ON r.environment_id = e.id              WHERE e.repo_id = actions_jobs.repo_id AND e.name = actions_jobs.environment)",
        )
        .bind(repo.0)
        .fetch_all(&mut *tx)
        .await?;
        let mut runs: Vec<i64> = released.iter().map(|(_, run)| *run).chain(held).collect();
        runs.sort_unstable();
        runs.dedup();
        let mut settled = Vec::new();
        for run in runs {
            let waiting = released.iter().filter(|(_, of)| *of == run).map(|(job, _)| *job).collect();
            settled.push(settle(&mut tx, RunId(run), waiting, now).await?);
        }
        tx.commit().await?;
        Ok(settled)
    }

    /// The decisions reviewers made on the jobs of `run`, oldest first.
    pub async fn deployment_reviews(&self, run: RunId) -> Result<Vec<rbit_core::actions::DeploymentReview>, StorageError> {
        // id, job, attempt, environment, reviewer, decision, comment, created_at
        type ReviewRow = (i64, i64, i64, String, Option<i64>, String, String, i64);
        let rows: Vec<ReviewRow> = sqlx::query_as(
            "SELECT r.id, r.job_id, r.attempt, r.environment, r.reviewer_id, r.decision, r.comment, r.created_at \
             FROM actions_deployment_reviews r JOIN actions_jobs j ON j.id = r.job_id WHERE j.run_id = ? ORDER BY r.id",
        )
        .bind(run.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, job, attempt, environment, reviewer, decision, comment, at)| {
                Ok(rbit_core::actions::DeploymentReview {
                    id: rbit_core::actions::DeploymentReviewId(id),
                    job_id: JobId(job),
                    attempt,
                    environment,
                    reviewer: reviewer.map(AccountId),
                    decision: rbit_core::actions::ReviewDecision::parse(&decision).ok_or_else(|| corrupt("review decision", &decision))?,
                    comment,
                    created_at: Timestamp(at),
                })
            })
            .collect()
    }

    /// Records the URL the runner evaluated for the environment of `job`'s current task.
    pub async fn set_environment_url(&self, job: JobId, url: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE actions_jobs SET environment_url = ? WHERE id = ? AND environment IS NOT NULL")
            .bind(url)
            .bind(job.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Actions: third-party actions.
    /// Creates steps a runner added to a task (its actions' `post` scripts), waiting; a
    /// number that already has a step is left as it is.
    pub async fn add_steps(&self, task: TaskId, steps: &[(i64, &str)]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for (number, name) in steps {
            sqlx::query(
                "INSERT INTO actions_steps (task_id, number, name, status, started_at, stopped_at, log_index, log_length) \
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, 0, 0) ON CONFLICT (task_id, number) DO NOTHING",
            )
            .bind(task.0)
            .bind(number)
            .bind(name)
            .bind(Status::Waiting.code())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Creates or updates steps of a task, by number.
    pub async fn put_steps(&self, task: TaskId, steps: &[StepUpdate<'_>]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for step in steps {
            sqlx::query(
                "INSERT INTO actions_steps (task_id, number, name, status, started_at, stopped_at, log_index, log_length) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT (task_id, number) DO UPDATE SET name = ?3, status = ?4, \
                 started_at = ?5, stopped_at = ?6, log_index = ?7, log_length = ?8",
            )
            .bind(task.0)
            .bind(step.number)
            .bind(step.name)
            .bind(step.status.code())
            .bind(step.started_at.map(|at| at.0))
            .bind(step.stopped_at.map(|at| at.0))
            .bind(step.log_index)
            .bind(step.log_length)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The steps of a task, by number.
    pub async fn steps(&self, task: TaskId) -> Result<Vec<Step>, StorageError> {
        let rows: Vec<StepRow> = sqlx::query_as(
            "SELECT task_id, number, name, status, started_at, stopped_at, log_index, log_length FROM actions_steps \
             WHERE task_id = ? ORDER BY number",
        )
        .bind(task.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Step::try_from).collect()
    }

    /// Records how much of the task's log is written.
    pub async fn set_log_progress(&self, id: TaskId, lines: i64, bytes: i64, truncated: bool) -> Result<(), StorageError> {
        sqlx::query("UPDATE actions_tasks SET log_lines = ?, log_bytes = ?, log_truncated = ? WHERE id = ?")
            .bind(lines)
            .bind(bytes)
            .bind(truncated)
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_log_state(&self, id: TaskId, state: LogState) -> Result<(), StorageError> {
        sqlx::query("UPDATE actions_tasks SET log_state = ? WHERE id = ?").bind(state.as_str()).bind(id.0).execute(&self.pool).await?;
        Ok(())
    }

    /// Finishes a running task with `result` (its token stops working), and its job unless
    /// the job was cancelled meanwhile or was given to a newer task by a rerun; then settles
    /// the run. `None` when the task was not running.
    pub async fn finish_task(&self, id: TaskId, result: Status, now: Timestamp) -> Result<Option<Settled>, StorageError> {
        if !result.is_done() {
            return Err(corrupt("task result", result));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let job: Option<i64> = sqlx::query_scalar(
            "UPDATE actions_tasks SET status = ?1, stopped_at = ?2, token_digest = NULL WHERE id = ?3 AND status = 6 RETURNING job_id",
        )
        .bind(result.code())
        .bind(now.0)
        .bind(id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(job) = job else {
            return Ok(None);
        };
        let run: i64 = sqlx::query_scalar(
            "UPDATE actions_jobs SET status = CASE WHEN status = 6 AND task_id = ?4 THEN ?1 ELSE status END, \
             finished_at = CASE WHEN task_id = ?4 THEN COALESCE(finished_at, ?2) ELSE finished_at END \
             WHERE id = ?3 RETURNING run_id",
        )
        .bind(result.code())
        .bind(now.0)
        .bind(job)
        .bind(id.0)
        .fetch_one(&mut *tx)
        .await?;
        let settled = settle(&mut tx, RunId(run), Vec::new(), now).await?;
        tx.commit().await?;
        Ok(Some(settled))
    }

    /// Cancels a run: jobs not yet running at once; running jobs in the database (their
    /// runners learn it on their next update) with their task tokens cleared immediately.
    /// `None` when the run is not in `repo` or already finished.
    pub async fn cancel_run(&self, repo: RepoId, run: RunId, now: Timestamp) -> Result<Option<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let found: Option<i64> =
            sqlx::query_scalar("SELECT id FROM actions_runs WHERE id = ? AND repo_id = ? AND status NOT IN (1, 2, 3, 4)")
                .bind(run.0)
                .bind(repo.0)
                .fetch_optional(&mut *tx)
                .await?;
        if found.is_none() {
            return Ok(None);
        }
        sqlx::query(
            "UPDATE actions_tasks SET token_digest = NULL WHERE status = 6 AND job_id IN (SELECT id FROM actions_jobs WHERE run_id = ?)",
        )
        .bind(run.0)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE actions_jobs SET status = 3, finished_at = ? WHERE run_id = ? AND status IN (5, 6, 7, 8)")
            .bind(now.0)
            .bind(run.0)
            .execute(&mut *tx)
            .await?;
        let settled = settle(&mut tx, run, Vec::new(), now).await?;
        tx.commit().await?;
        Ok(Some(settled))
    }

    /// Cancels waiting jobs nobody took since `queued_before` (the abandoned sweep) and
    /// settles their runs.
    pub async fn cancel_abandoned(&self, queued_before: Timestamp, now: Timestamp) -> Result<Vec<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let runs: Vec<i64> =
            sqlx::query_scalar("UPDATE actions_jobs SET status = 3, finished_at = ?1 WHERE status = 5 AND queued_at < ?2 RETURNING run_id")
                .bind(now.0)
                .bind(queued_before.0)
                .fetch_all(&mut *tx)
                .await?;
        let mut settled = Vec::new();
        for run in runs.into_iter().collect::<BTreeSet<_>>() {
            settled.push(settle(&mut tx, RunId(run), Vec::new(), now).await?);
        }
        tx.commit().await?;
        Ok(settled)
    }

    /// Running tasks whose last heartbeat is older than `before` (the zombie sweep).
    pub async fn silent_tasks(&self, before: Timestamp) -> Result<Vec<TaskId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM actions_tasks WHERE status = 6 AND heartbeat_at < ? ORDER BY id")
            .bind(before.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().map(TaskId).collect())
    }

    // Actions: S.
    /// The running tasks of `runner` (it is going away).
    pub async fn running_tasks_of(&self, runner: RunnerId) -> Result<Vec<TaskId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM actions_tasks WHERE status = 6 AND runner_id = ? ORDER BY id")
            .bind(runner.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().map(TaskId).collect())
    }

    /// Log bytes kept across the instance (`[actions] max_log_storage_bytes`).
    pub async fn stored_log_bytes(&self) -> Result<u64, StorageError> {
        let bytes: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(log_bytes), 0) FROM actions_tasks WHERE log_state != 'expired'")
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(bytes).unwrap_or_default())
    }

    /// Stopped tasks whose log was never finalised (a runner that went away, a crash).
    pub async fn unfinalised_logs(&self, limit: u32) -> Result<Vec<TaskId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM actions_tasks WHERE status != 6 AND log_state = 'live' ORDER BY id LIMIT ?")
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().map(TaskId).collect())
    }

    /// Running tasks started before `before` (the endless sweep).
    pub async fn long_running_tasks(&self, before: Timestamp) -> Result<Vec<TaskId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM actions_tasks WHERE status = 6 AND started_at < ? ORDER BY id")
            .bind(before.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().map(TaskId).collect())
    }

    /// Lets a held or approved run start: jobs awaiting approval or held are blocked, then
    /// those without needs become waiting. `None` when the run is not in `repo` or is
    /// neither held nor awaiting approval.
    pub async fn release_run(&self, repo: RepoId, run: RunId, now: Timestamp) -> Result<Option<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let released = release(&mut tx, repo, run, now).await?;
        tx.commit().await?;
        Ok(released)
    }

    /// Approves a run awaiting approval. With `hold`, its jobs stay blocked for its
    /// concurrency group (see [`Self::release_run`]); otherwise it starts. `None` when the
    /// run is not in `repo` or does not await approval.
    pub async fn approve_run(
        &self,
        repo: RepoId,
        run: RunId,
        by: AccountId,
        hold: bool,
        now: Timestamp,
    ) -> Result<Option<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let approved = sqlx::query("UPDATE actions_runs SET approved_by = ?, approved_at = ? WHERE id = ? AND repo_id = ? AND status = 8")
            .bind(by.0)
            .bind(now.0)
            .bind(run.0)
            .bind(repo.0)
            .execute(&mut *tx)
            .await?;
        if approved.rows_affected() == 0 {
            return Ok(None);
        }
        let settled = if hold {
            sqlx::query("UPDATE actions_jobs SET status = 7 WHERE run_id = ? AND status = 8").bind(run.0).execute(&mut *tx).await?;
            sqlx::query("UPDATE actions_runs SET status = 7 WHERE id = ?").bind(run.0).execute(&mut *tx).await?;
            settle(&mut tx, run, Vec::new(), now).await?
        } else {
            release(&mut tx, repo, run, now).await?.ok_or_else(|| corrupt("run", "approved run vanished"))?
        };
        tx.commit().await?;
        Ok(Some(settled))
    }

    /// Starts a finished run again: the chosen jobs (and those needing them) get a new
    /// attempt, the run's attempt grows by one. `None` when the run is not in `repo`, has
    /// not finished, or nothing matches.
    pub async fn rerun(
        &self,
        repo: RepoId,
        run: RunId,
        which: Rerun,
        start: RunStart,
        now: Timestamp,
    ) -> Result<Option<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let found: Option<i64> =
            sqlx::query_scalar("SELECT id FROM actions_runs WHERE id = ? AND repo_id = ? AND status IN (1, 2, 3, 4) AND error IS NULL")
                .bind(run.0)
                .bind(repo.0)
                .fetch_optional(&mut *tx)
                .await?;
        if found.is_none() {
            return Ok(None);
        }
        let rows: Vec<JobRow> = sqlx::query_as(select_job!("WHERE j.run_id = ? ORDER BY j.id")).bind(run.0).fetch_all(&mut *tx).await?;
        let jobs = rows.into_iter().map(Job::try_from).collect::<Result<Vec<_>, _>>()?;
        // Chosen by id: a matrix instance that succeeded does not run again with a failed
        // sibling, as on GitHub.
        let mut chosen: BTreeSet<i64> = jobs
            .iter()
            .filter(|job| match which {
                Rerun::All => true,
                Rerun::Failed => matches!(job.status, Status::Failure | Status::Cancelled),
                Rerun::Job(id) => job.id == id,
            })
            .map(|job| job.id.0)
            .collect();
        if chosen.is_empty() {
            return Ok(None);
        }
        let mut keys: BTreeSet<&str> = jobs.iter().filter(|job| chosen.contains(&job.id.0)).map(|job| job.key.as_str()).collect();
        // The jobs needing a chosen job run again too, transitively.
        loop {
            let before = chosen.len();
            for job in &jobs {
                if job.needs.iter().any(|need| keys.contains(need.as_str())) {
                    chosen.insert(job.id.0);
                    keys.insert(&job.key);
                }
            }
            if chosen.len() == before {
                break;
            }
        }
        // A new attempt starts its own clock, as GitHub times each attempt; the first job it
        // starts sets `started_at` again.
        sqlx::query("UPDATE actions_runs SET attempt = attempt + 1, status = ?, started_at = NULL, finished_at = NULL WHERE id = ?")
            .bind(placeholder_status(start).code())
            .bind(run.0)
            .execute(&mut *tx)
            .await?;
        let mut waiting = Vec::new();
        for job in jobs.iter().filter(|job| chosen.contains(&job.id.0)) {
            let waits = job.needs.iter().any(|need| keys.contains(need.as_str()));
            let status = initial_status(start, waits);
            sqlx::query(
                "UPDATE actions_jobs SET status = ?1, attempt = attempt + 1, task_id = NULL, runner_id = NULL, started_at = NULL, \
                 finished_at = NULL, queued_at = ?2, review = 0, environment_url = NULL WHERE id = ?3",
            )
            .bind(status.code())
            .bind((status == Status::Waiting).then_some(now.0))
            .bind(job.id.0)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM actions_job_outputs WHERE job_id = ?").bind(job.id.0).execute(&mut *tx).await?;
            if status == Status::Waiting {
                waiting.push(job.id.0);
            }
        }
        let settled = settle(&mut tx, run, waiting, now).await?;
        tx.commit().await?;
        Ok(Some(settled))
    }

    /// Records who started the run's latest attempt (`github.triggering_actor`): whoever ran
    /// it again, as on GitHub, so an environment that prevents self-review sees them too.
    pub async fn set_trigger_actor(&self, run: RunId, actor: AccountId) -> Result<(), StorageError> {
        sqlx::query("UPDATE actions_runs SET trigger_actor_id = ? WHERE id = ?").bind(actor.0).bind(run.0).execute(&self.pool).await?;
        Ok(())
    }

    /// Finished runs whose logs are kept, finished before `before`, oldest first: the
    /// candidates for log retention and the instance log budget.
    pub async fn runs_with_logs(&self, before: Timestamp, limit: u32) -> Result<Vec<(RunId, Vec<TaskId>)>, StorageError> {
        let runs: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM actions_runs WHERE finished_at IS NOT NULL AND logs_expired_at IS NULL AND finished_at < ? \
             ORDER BY finished_at, id LIMIT ?",
        )
        .bind(before.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut found = Vec::with_capacity(runs.len());
        for run in runs {
            let tasks: Vec<i64> = sqlx::query_scalar(
                "SELECT t.id FROM actions_tasks t JOIN actions_jobs j ON j.id = t.job_id WHERE j.run_id = ? ORDER BY t.id",
            )
            .bind(run)
            .fetch_all(&self.pool)
            .await?;
            found.push((RunId(run), tasks.into_iter().map(TaskId).collect()));
        }
        Ok(found)
    }

    /// Marks a run's logs as removed.
    pub async fn expire_logs(&self, run: RunId, now: Timestamp) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("UPDATE actions_runs SET logs_expired_at = COALESCE(logs_expired_at, ?) WHERE id = ?")
            .bind(now.0)
            .bind(run.0)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE actions_tasks SET log_state = 'expired' WHERE job_id IN (SELECT id FROM actions_jobs WHERE run_id = ?)")
            .bind(run.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Deletes runs that finished before `before` (`[actions] run_retention_days`).
    pub async fn delete_finished_before(&self, before: Timestamp) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM actions_runs WHERE finished_at < ?").bind(before.0).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    // Actions: interface and secrets (U).
    /// The workflows that have runs in `repo`, by path, each with the name of its newest
    /// run (the runs page's sidebar).
    pub async fn workflows(&self, repo: RepoId) -> Result<Vec<(String, String)>, StorageError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT r.workflow_path, r.workflow_name FROM actions_runs r              WHERE r.id IN (SELECT MAX(id) FROM actions_runs WHERE repo_id = ? GROUP BY workflow_path) ORDER BY r.workflow_path",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Records `action` as the `workflow_run` action last announced for `run`; `false`
    /// when it already was, so concurrent callers announce each action once.
    pub async fn announce_run(&self, run: RunId, action: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE actions_runs SET announced = ?2 WHERE id = ?1 AND announced IS NOT ?2")
            .bind(run.0)
            .bind(action)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// [`Self::announce_run`] for the `workflow_job` action of `job`.
    pub async fn announce_job(&self, job: JobId, action: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE actions_jobs SET announced = ?2 WHERE id = ?1 AND announced IS NOT ?2")
            .bind(job.0)
            .bind(action)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    // Actions: join (J).
    /// Settles job `job`'s own `if:` (listed in [`Settled::to_decide`]): it waits for a
    /// runner, is skipped, or fails (the condition could not be evaluated). `None` when the
    /// job is no longer blocked — someone decided first, or the run was cancelled.
    pub async fn decide(&self, job: JobId, decision: Decision, now: Timestamp) -> Result<Option<Settled>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let (status, queued) = match decision {
            Decision::Run => (Status::Waiting, Some(now.0)),
            Decision::Skip => (Status::Skipped, None),
            Decision::Fail => (Status::Failure, None),
        };
        let run: Option<i64> = sqlx::query_scalar(
            "UPDATE actions_jobs SET status = ?2, queued_at = COALESCE(?3, queued_at), \
             finished_at = CASE WHEN ?3 IS NULL THEN ?4 ELSE finished_at END WHERE id = ?1 AND status = 7 AND condition = 1 RETURNING run_id",
        )
        .bind(job.0)
        .bind(status.code())
        .bind(queued)
        .bind(now.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(run) = run else { return Ok(None) };
        let waiting = if decision == Decision::Run { vec![job.0] } else { Vec::new() };
        let settled = settle(&mut tx, RunId(run), waiting, now).await?;
        tx.commit().await?;
        Ok(Some(settled))
    }

    /// Cancels the unfinished jobs among `jobs` of run `run` (`strategy.fail-fast`: the
    /// matrix siblings of a job that failed). Running ones are cancelled in the database —
    /// their task tokens die at once, as with [`Self::cancel_run`] (§11.11) — and their
    /// runners learn it on their next state call.
    pub async fn cancel_jobs(&self, run: RunId, jobs: &[JobId], now: Timestamp) -> Result<Option<Settled>, StorageError> {
        if jobs.is_empty() {
            return Ok(None);
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut cancelled = 0;
        for job in jobs {
            sqlx::query(
                "UPDATE actions_tasks SET token_digest = NULL WHERE status = 6 AND job_id = ?1 \
                 AND job_id IN (SELECT id FROM actions_jobs WHERE run_id = ?2)",
            )
            .bind(job.0)
            .bind(run.0)
            .execute(&mut *tx)
            .await?;
            let result = sqlx::query(
                "UPDATE actions_jobs SET status = 3, finished_at = ?3 WHERE id = ?1 AND run_id = ?2 AND status IN (5, 6, 7, 8)",
            )
            .bind(job.0)
            .bind(run.0)
            .bind(now.0)
            .execute(&mut *tx)
            .await?;
            cancelled += result.rows_affected();
        }
        if cancelled == 0 {
            return Ok(None);
        }
        let settled = settle(&mut tx, run, Vec::new(), now).await?;
        tx.commit().await?;
        Ok(Some(settled))
    }

    /// Settles run `run` again without changing anything: what [`Self::create_run`] did,
    /// as a [`Settled`] the caller can follow up (jobs to decide, the scheduler's version).
    pub async fn resettle(&self, run: RunId, now: Timestamp) -> Result<Settled, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let waiting: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM actions_jobs WHERE run_id = ? AND status = 5").bind(run.0).fetch_all(&mut *tx).await?;
        let settled = settle(&mut tx, run, waiting, now).await?;
        tx.commit().await?;
        Ok(settled)
    }

    /// Jobs left blocked with their `if:` undecided although their needs are done and their
    /// run is not held — a decision lost to a restart — oldest first, for the sweeper.
    pub async fn undecided(&self, limit: u32) -> Result<Vec<JobId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar(concat!(
            "SELECT id FROM actions_jobs WHERE condition = 1 AND status = 7 AND NOT EXISTS (",
            "SELECT 1 FROM json_each(actions_jobs.needs) n JOIN actions_jobs d ON d.run_id = actions_jobs.run_id AND d.job_key = n.value ",
            "WHERE d.status NOT IN (1, 2, 3, 4)) AND run_id IN (SELECT id FROM actions_runs WHERE status NOT IN (7, 8)) ORDER BY id LIMIT ?"
        ))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids.into_iter().map(JobId).collect())
    }
}

/// The waiting job a runner may take, oldest first: the repository has Actions on, the
/// runner's scope covers it, and the job asks for labels, all of which the runner has.
const CLAIM_CANDIDATE: &str = "SELECT j.id FROM actions_jobs j \
     JOIN repositories r ON r.id = j.repo_id \
     JOIN actions_repositories a ON a.repo_id = j.repo_id AND a.enabled = 1 \
     WHERE j.status = 5 \
     AND CASE ?1 \
         WHEN 'instance' THEN ?4 = 1 OR EXISTS (SELECT 1 FROM actions_owner_policies p WHERE p.owner_id = r.owner_id AND p.instance_runners = 1) \
         WHEN 'owner' THEN r.owner_id = ?2 \
         ELSE j.repo_id = ?3 END \
     AND EXISTS (SELECT 1 FROM actions_job_labels l WHERE l.job_id = j.id) \
     AND NOT EXISTS (SELECT 1 FROM actions_job_labels l WHERE l.job_id = j.id AND l.label NOT IN (SELECT value FROM json_each(?5))) \
     AND (j.max_parallel IS NULL OR (SELECT COUNT(*) FROM actions_jobs s \
         WHERE s.run_id = j.run_id AND s.job_key = j.job_key AND s.status = 6) < j.max_parallel) \
     AND (j.job_group IS NULL OR NOT EXISTS (SELECT 1 FROM actions_jobs g \
         WHERE g.repo_id = j.repo_id AND g.job_group = j.job_group AND g.status = 6)) \
     ORDER BY j.queued_at, j.id LIMIT 1";

/// A run's status before its first settling: held and awaiting runs keep theirs, which
/// tells [`settle`] not to unblock their jobs.
fn placeholder_status(start: RunStart) -> RunStatus {
    match start {
        RunStart::Runnable => Status::Waiting,
        RunStart::HeldForConcurrency => Status::Blocked,
        RunStart::AwaitingApproval => Status::AwaitingApproval,
    }
}

fn initial_status(start: RunStart, has_needs: bool) -> JobStatus {
    match start {
        RunStart::Runnable if has_needs => Status::Blocked,
        RunStart::Runnable => Status::Waiting,
        RunStart::HeldForConcurrency => Status::Blocked,
        RunStart::AwaitingApproval => Status::AwaitingApproval,
    }
}

async fn insert_labels(tx: &mut SqliteConnection, job: i64, labels: &[String]) -> Result<(), StorageError> {
    for label in labels {
        sqlx::query("INSERT INTO actions_job_labels (job_id, label) VALUES (?, ?) ON CONFLICT DO NOTHING")
            .bind(job)
            .bind(label)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

async fn release(tx: &mut SqliteConnection, repo: RepoId, run: RunId, now: Timestamp) -> Result<Option<Settled>, StorageError> {
    let found: Option<i64> = sqlx::query_scalar("SELECT id FROM actions_runs WHERE id = ? AND repo_id = ? AND status IN (7, 8)")
        .bind(run.0)
        .bind(repo.0)
        .fetch_optional(&mut *tx)
        .await?;
    if found.is_none() {
        return Ok(None);
    }
    sqlx::query("UPDATE actions_jobs SET status = 7 WHERE run_id = ? AND status = 8").bind(run.0).execute(&mut *tx).await?;
    // The run is no longer held: settling unblocks the jobs whose needs are done, which
    // includes every job that needs nothing.
    sqlx::query("UPDATE actions_runs SET status = 6 WHERE id = ?").bind(run.0).execute(&mut *tx).await?;
    settle(tx, run, Vec::new(), now).await.map(Some)
}

/// The blocked jobs of run `?1` whose needs have all finished.
macro_rules! needs_done {
    () => {
        "run_id = ?1 AND status = 7 AND NOT EXISTS (\
         SELECT 1 FROM json_each(actions_jobs.needs) n JOIN actions_jobs d ON d.run_id = actions_jobs.run_id AND d.job_key = n.value \
         WHERE d.status NOT IN (1, 2, 3, 4))"
    };
}

/// A job needed by the row did not succeed (every matrix instance of a needed key counts).
macro_rules! a_need_failed {
    () => {
        "EXISTS (SELECT 1 FROM json_each(actions_jobs.needs) n JOIN actions_jobs d ON d.run_id = actions_jobs.run_id \
         AND d.job_key = n.value WHERE d.status <> 1)"
    };
}

/// Unblocks the jobs whose needs have all finished — unless the run is held for its
/// concurrency group or awaits approval (its stored status says so) — then recomputes the
/// run's status and times. `now_waiting` lists jobs the caller already made waiting.
///
/// A job without its own `if:` follows GitHub's default, `success()`: it waits for a runner
/// when every job it needs succeeded and is skipped otherwise, which can finish the needs of
/// others in turn. A job with its own `if:` stays blocked and is listed in
/// [`Settled::to_decide`] for the caller to evaluate (§12.4).
async fn settle(tx: &mut SqliteConnection, run: RunId, mut now_waiting: Vec<i64>, now: Timestamp) -> Result<Settled, StorageError> {
    let run_status: i64 = sqlx::query_scalar("SELECT status FROM actions_runs WHERE id = ?").bind(run.0).fetch_one(&mut *tx).await?;
    let held = matches!(status(run_status)?, Status::Blocked | Status::AwaitingApproval);
    let mut to_decide = Vec::new();
    if !held {
        // Each round's skips may finish the needs of further jobs, so this ends after as
        // many rounds as the run's longest `needs` chain.
        loop {
            let skipped: Vec<i64> = sqlx::query_scalar(concat!(
                "UPDATE actions_jobs SET status = 4, finished_at = ?2 WHERE ",
                needs_done!(),
                " AND condition = 0 AND ",
                a_need_failed!(),
                " RETURNING id"
            ))
            .bind(run.0)
            .bind(now.0)
            .fetch_all(&mut *tx)
            .await?;
            if skipped.is_empty() {
                break;
            }
        }
        let unblocked: Vec<i64> = sqlx::query_scalar(concat!(
            "UPDATE actions_jobs SET status = 5, queued_at = ?2 WHERE ",
            needs_done!(),
            " AND condition = 0 RETURNING id"
        ))
        .bind(run.0)
        .bind(now.0)
        .fetch_all(&mut *tx)
        .await?;
        now_waiting.extend(unblocked);
        to_decide = sqlx::query_scalar(concat!("SELECT id FROM actions_jobs WHERE ", needs_done!(), " AND condition = 1 ORDER BY id"))
            .bind(run.0)
            .fetch_all(&mut *tx)
            .await?;
    }
    // Actions: environments. A job about to wait for a runner whose environment has
    // reviewers, and whose attempt none approved, waits for one of them instead; in the same
    // transaction, so no runner can take it in between. Protection is read as it is now.
    let reviewing: Vec<i64> = sqlx::query_scalar(
        "UPDATE actions_jobs SET status = 8, review = 1, queued_at = NULL WHERE run_id = ?1 AND status = 5 \
         AND environment IS NOT NULL AND (approved_attempt IS NULL OR approved_attempt <> attempt) AND EXISTS \
         (SELECT 1 FROM actions_environments e JOIN actions_environment_reviewers r ON r.environment_id = e.id \
          WHERE e.repo_id = actions_jobs.repo_id AND e.name = actions_jobs.environment) RETURNING id",
    )
    .bind(run.0)
    .fetch_all(&mut *tx)
    .await?;
    now_waiting.retain(|job| !reviewing.contains(job));
    // A job waiting for its reviewer counts as waiting for the run: the run itself is not
    // held, as one awaiting approval is.
    let codes: Vec<(i64, bool)> =
        sqlx::query_as("SELECT status, review FROM actions_jobs WHERE run_id = ?").bind(run.0).fetch_all(&mut *tx).await?;
    let statuses = codes
        .into_iter()
        .map(|(code, review)| if review && code == Status::AwaitingApproval.code() { Ok(Status::Waiting) } else { status(code) })
        .collect::<Result<Vec<_>, _>>()?;
    let aggregated = match aggregate(statuses.iter().copied()) {
        // Every job blocked on a decision (its own `if:`) is not a run held for its group:
        // a blocked run status would stop the next settle from unblocking anything.
        Status::Blocked if !held => Status::Waiting,
        other => other,
    };
    let started = statuses.contains(&Status::Running);
    sqlx::query(
        "UPDATE actions_runs SET status = ?1, started_at = CASE WHEN ?2 THEN COALESCE(started_at, ?3) ELSE started_at END, \
         finished_at = CASE WHEN ?4 THEN COALESCE(finished_at, ?3) ELSE NULL END WHERE id = ?5",
    )
    .bind(aggregated.code())
    .bind(started)
    .bind(now.0)
    .bind(aggregated.is_done())
    .bind(run.0)
    .execute(&mut *tx)
    .await?;
    now_waiting.sort_unstable();
    Ok(Settled {
        run_id: run,
        run_status: aggregated,
        now_waiting: now_waiting.into_iter().map(JobId).collect(),
        now_reviewing: reviewing.into_iter().map(JobId).collect(),
        to_decide: to_decide.into_iter().map(JobId).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `FetchTask`'s query must walk the partial index of waiting jobs in queue order rather
    /// than scan every job ever run.
    #[tokio::test]
    async fn the_claim_query_walks_the_waiting_index() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::Database::open(&dir.path().join("rbit.db")).await.unwrap();
        sqlx::query("ANALYZE").execute(&database.pool).await.unwrap();
        let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {CLAIM_CANDIDATE}")))
            .bind("repository")
            .bind(None::<i64>)
            .bind(1)
            .bind(false)
            .bind("[\"linux\"]")
            .fetch_all(&database.pool)
            .await
            .unwrap();
        let plan: Vec<&str> = plan.iter().map(|row| row.3.as_str()).collect();
        assert!(plan.iter().any(|step| step.contains("USING INDEX actions_jobs_waiting")), "{plan:#?}");
        assert!(!plan.iter().any(|step| step.contains("TEMP B-TREE FOR ORDER BY")), "{plan:#?}");
    }
}
