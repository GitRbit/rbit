//! rbit Actions: CI/CD on rbit's own runner, `rbit-runner` (`docs/design/actions.md`).
//!
//! [`ActionsService`] exists only when `[actions] enabled` is set: [`ActionsService::new`]
//! answers `None` otherwise, and `Services::actions` stays `None`. With it absent there is no
//! subscriber, ticker or sweeper, no route is merged, task tokens are refused before any
//! lookup, and nothing is written to the `actions_*` tables or under `<data_dir>/actions`.
//! Code outside this module reaches Actions only through `Services::actions`, so "off" needs
//! no other check.
//!
//! The work is split by area; each submodule extends [`ActionsService`]:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`triggers`] | Domain events → runs, on the bounded evaluation semaphore |
//! | [`chains`] | `workflow_run` and `repository_dispatch` runs |
//! | [`calls`] | Reusable workflows: finding called workflows, and what their jobs get |
//! | [`runs`] | Creating runs, concurrency, cancel, re-run, approval, listings |
//! | [`context`] | The task context and GitHub-shaped event payloads |
//! | [`dispatch`] | `workflow_dispatch` |
//! | [`schedules`] | Schedule refresh and the ticker |
//! | [`trust`] | Pull request trust |
//! | [`statuses`] | Commit statuses for jobs |
//! | [`runners`] | Registration tokens, `register`, `declare`, `unregister`, the runner cache, runners in settings |
//! | [`scheduler`] | `poll`: the long poll, claims, idempotent retries, job specifications |
//! | [`tasks`] | `tasks/{id}/state`: heartbeats, steps, outputs, results, cancellation |
//! | [`logs`] | `tasks/{id}/logs`: appends by line offset, masking, gzip, reads for pages |
//! | [`tokens`] | Task tokens for Git, LFS and the API; what a task's push may not do |
//! | [`secrets`] | Secrets and variables |
//! | [`settings`] | A repository's switch, token permission and trusted authors |
//! | [`conditions`] | Job `if:` once its needs are done, fail-fast |
//! | [`artifacts`] | Artifacts: uploads, downloads, quota and retention |
//! | [`downloads`] | Artifact downloads, as zip or tar.gz, rewritten as they are sent |
//! | [`browse`] | What the pages read: runs, a run and its jobs |
//! | [`sweeper`] | Zombie and endless tasks, abandoned jobs, log and run retention, the log budget, stale tokens |
//! | [`hooks`] | `workflow_run` and `workflow_job` webhooks |
//!
//! The Actions foundation (workstream F) owns this file; the other workstreams append their
//! fields and start-up lines in marked blocks.

// Actions: artifacts (A).
pub mod artifacts;
// Actions: shared cache.
pub mod browse;
pub mod caches;
// Actions: reusable workflows.
pub mod calls;
// Actions: workflow_run and repository_dispatch.
pub mod chains;
// Actions: join (J).
pub mod conditions;
pub mod context;
pub mod dispatch;
// Actions: artifact downloads as zip and tar.gz.
pub mod downloads;
pub mod environments;
pub mod hooks;
pub mod logs;
pub mod runners;
pub mod runs;
pub mod scheduler;
pub mod schedules;
pub mod secrets;
// Actions: interface and secrets (U).
pub mod settings;
pub mod statuses;
pub mod sweeper;
pub mod tasks;
pub mod tokens;
pub mod triggers;
pub mod trust;

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rbit_core::config::ActionsConfig;
use rbit_plugin_api::Generation;
use rbit_plugin_runtime::{DrainError, PluginScope};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::ServiceContext;

/// How long [`ActionsService::shutdown`] waits for background work.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// The Actions control plane. Cheap to clone.
#[derive(Debug, Clone)]
pub struct ActionsService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    shutdown: CancellationToken,
    scope: PluginScope,
    started: AtomicBool,
    /// The scheduler's version: moves whenever a job becomes waiting or a runner's labels
    /// change, so a poll that already looked at this version waits without SQLite.
    tasks_version: AtomicI64,
    /// Workflow evaluations at once (`[actions] evaluation_concurrency`).
    evaluations: Arc<Semaphore>,
    // Actions: triggers (T).
    /// Concurrency-group decisions, one at a time (see [`runs`]).
    groups: tokio::sync::Mutex<()>,
    /// Domain events the subscriber missed because it fell behind.
    missed_events: AtomicU64,
    // Actions: interface and secrets (U).
    /// The secrets key, looked at once (see [`secrets`]).
    secrets_key: tokio::sync::OnceCell<secrets::KeyState>,
    // Actions: S.
    /// Every runner by credential digest, loaded on first use (see [`runners`]).
    runners: tokio::sync::OnceCell<runners::RunnerCache>,
    /// Wakes the polls waiting for work whenever `tasks_version` moves (see [`scheduler`]).
    wake: tokio::sync::Notify,
    /// The polls held open now, one per runner.
    polls: scheduler::Polls,
    /// What each running task's outputs and logs are masked with (see [`logs`]).
    masks: std::sync::Mutex<std::collections::HashMap<rbit_core::actions::TaskId, Arc<logs::Masker>>>,
    /// One lock per task whose log is being appended to, with whether its files were
    /// checked against the database in this process (see [`logs`]).
    log_locks: std::sync::Mutex<std::collections::HashMap<rbit_core::actions::TaskId, Arc<tokio::sync::Mutex<bool>>>>,
}

impl ActionsService {
    /// The service, or `None` when `[actions] enabled` is off.
    pub fn new(ctx: ServiceContext) -> Option<Self> {
        if !ctx.config.actions.enabled {
            return None;
        }
        let evaluations = Arc::new(Semaphore::new(ctx.config.actions.evaluation_concurrency));
        Some(Self {
            inner: Arc::new(Inner {
                ctx,
                shutdown: CancellationToken::new(),
                scope: PluginScope::new(Generation::INITIAL),
                started: AtomicBool::new(false),
                tasks_version: AtomicI64::new(initial_tasks_version()),
                evaluations,
                // Actions: triggers (T).
                groups: tokio::sync::Mutex::new(()),
                missed_events: AtomicU64::new(0),
                // Actions: interface and secrets (U).
                secrets_key: tokio::sync::OnceCell::new(),
                // Actions: S.
                runners: tokio::sync::OnceCell::new(),
                wake: tokio::sync::Notify::new(),
                polls: scheduler::Polls::default(),
                masks: std::sync::Mutex::default(),
                log_locks: std::sync::Mutex::default(),
            }),
        })
    }

    /// Starts the subscriber, the schedule ticker and the sweeper. Call once, after start-up.
    pub fn spawn_background(&self) {
        if self.inner.scope.is_closed() || self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        tracing::info!(evaluation_concurrency = self.config().evaluation_concurrency, "Actions is enabled");
        // Actions: triggers (T). Subscribed before anything else runs, so no event published
        // from here on is missed.
        let events = self.inner.ctx.events.subscribe();
        self.spawn_task(self.clone().follow_events(events));
        self.spawn_task(self.clone().run_schedules());
        // Actions: interface and secrets (U). The key is created, or its problem logged, at
        // start-up rather than when a secret is first needed.
        let this = self.clone();
        self.spawn_task(async move {
            let _ = this.secrets_key().await;
        });
        // Actions: S. Stalled tasks, abandoned jobs, retention.
        self.spawn_task(self.clone().run_sweeper());
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        if let Err(error) = self.stop_background().await {
            tracing::warn!(%error, "Actions background work did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    /// Closes task admission and verifies that background work has stopped.
    ///
    /// A timeout leaves surviving work tracked and admission closed. Runtime lifecycle
    /// callers must preserve this failure instead of claiming successful disposal.
    pub async fn stop_background(&self) -> Result<(), DrainError> {
        self.inner.scope.quiesce();
        self.inner.shutdown.cancel();
        self.inner.scope.dispose(SHUTDOWN_GRACE).await
    }

    /// Background tasks running now (the disabled-is-disabled tests count them).
    pub fn background_tasks(&self) -> usize {
        self.inner.scope.in_flight()
    }

    /// Every background task enters the generation gate, including work spawned by a
    /// subscriber racing with shutdown. Refused futures are dropped without execution.
    fn spawn_task(&self, task: impl Future<Output = ()> + Send + 'static) {
        if let Err(error) = self.inner.scope.spawn(|_| task) {
            tracing::debug!(%error, "Actions refused background work during shutdown");
        }
    }

    pub fn config(&self) -> &ActionsConfig {
        &self.inner.ctx.config.actions
    }

    // Actions: triggers (T).
    fn ctx(&self) -> &ServiceContext {
        &self.inner.ctx
    }

    /// The limits a workflow file of this instance is parsed and planned within.
    pub fn limits(&self) -> rbit_workflow::Limits {
        let max_bytes = usize::try_from(self.config().max_workflow_bytes).unwrap_or(usize::MAX);
        let max_timeout_minutes = u32::try_from(self.config().endless_timeout_seconds / 60).unwrap_or(u32::MAX).max(1);
        rbit_workflow::Limits { max_bytes, max_timeout_minutes, ..rbit_workflow::Limits::default() }
    }

    /// The current `FetchTask` version.
    pub fn tasks_version(&self) -> i64 {
        self.inner.tasks_version.load(Ordering::SeqCst)
    }

    /// Moves the `FetchTask` version after a job became waiting or a runner's labels changed.
    pub fn bump_tasks_version(&self) {
        self.inner.tasks_version.fetch_add(1, Ordering::SeqCst);
        // Actions: S. Every waiting poll looks again.
        self.inner.wake.notify_waiters();
    }

    /// The semaphore every workflow evaluation holds a permit of until its blocking work
    /// returns.
    pub fn evaluations(&self) -> &Arc<Semaphore> {
        &self.inner.evaluations
    }
}

/// The first `FetchTask` version: the current time in milliseconds, so it is never 0 and
/// never a value a runner kept from before a restart.
fn initial_tasks_version() -> i64 {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |elapsed| elapsed.as_millis());
    i64::try_from(millis).unwrap_or(i64::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_tasks_version_is_the_time_in_milliseconds() {
        let version = initial_tasks_version();
        let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
        assert!(version > 1_700_000_000_000 && version <= now);
    }
}
