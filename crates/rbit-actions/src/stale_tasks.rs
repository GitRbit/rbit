//! The sweep's task passes (§5.5, §11.8): which tasks and jobs went stale, and what each
//! becomes. The atomic SQL that selects them and moves their state stays the race
//! authority; these passes decide the cutoffs, the reasons and the order of the steps.
//!
//! * **Zombies**: a running task without a state call for `zombie_timeout_seconds` fails,
//!   its runner gone or cut off. **Endless tasks**: a task running longer than
//!   `endless_timeout_seconds` fails. Each says why in its log, then is failed; only a task
//!   that was still running is finalised (masks forgotten, log finalised, job and
//!   dependents settled), and its runner is deleted when it was ephemeral.
//! * **Abandoned jobs**: a job waiting longer than `abandoned_timeout_seconds` for a runner
//!   is cancelled, and each run that changed is settled.

use std::future::Future;

/// The instance's timeouts (`[actions]`), in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub zombie_seconds: u64,
    pub endless_seconds: u64,
    pub abandoned_seconds: u64,
}

impl Timeouts {
    /// What a zombie task's log says.
    pub fn zombie_reason(&self) -> String {
        format!("the runner sent no word for {} seconds, so the task failed", self.zombie_seconds)
    }

    /// What an endless task's log says.
    pub fn endless_reason(&self) -> String {
        format!("the task ran longer than {} seconds, so it failed", self.endless_seconds)
    }
}

/// `seconds` before `now` (Unix seconds), never overflowing.
pub fn before(now: i64, seconds: u64) -> i64 {
    now.saturating_sub(i64::try_from(seconds).unwrap_or(i64::MAX))
}

/// Which pass found a task stale, so its termination record can say why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// No state call for the zombie timeout: the runner is gone or cut off.
    Silent,
    /// Running longer than the endless timeout.
    Endless,
}

/// What the passes did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaleSwept {
    pub zombies: usize,
    pub endless: usize,
    pub abandoned: usize,
}

/// The task store's stale-task transitions and what follows them.
pub trait StaleTaskStore: Sync {
    type Error: Send;
    type TaskId: Copy + Send + Sync;
    /// A stored task, as it was before it was failed.
    type Task: Send + Sync;
    /// What a transition changed, for settling.
    type Settled: Send + Sync;

    /// Running tasks with no state call since `before`, oldest first.
    fn silent(&self, before: i64) -> impl Future<Output = Result<Vec<Self::TaskId>, Self::Error>> + Send;
    /// Running tasks started before `before`, oldest first.
    fn long_running(&self, before: i64) -> impl Future<Output = Result<Vec<Self::TaskId>, Self::Error>> + Send;
    /// Cancels jobs queued before `before`, in one transaction; what changed.
    fn cancel_abandoned(&self, before: i64, now: i64) -> impl Future<Output = Result<Vec<Self::Settled>, Self::Error>> + Send;
    fn task(&self, id: Self::TaskId) -> impl Future<Output = Result<Option<Self::Task>, Self::Error>> + Send;
    /// Writes `reason` into the task's log.
    fn note(&self, id: Self::TaskId, reason: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Fails the task if it is still running, recording why (`staleness`); `None` when it
    /// was not.
    fn fail(
        &self,
        id: Self::TaskId,
        staleness: Staleness,
        now: i64,
    ) -> impl Future<Output = Result<Option<Self::Settled>, Self::Error>> + Send;
    /// After a failure: forgets the task's masks and finalises its log, logging `reason`.
    fn close(&self, id: Self::TaskId, reason: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Settles the jobs and runs a transition changed, and what depends on them.
    fn settle(&self, settled: &Self::Settled) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Deletes the task's runner when it is ephemeral, and stops authenticating it.
    fn retire_ephemeral_runner(&self, task: &Self::Task) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// The sweep's task passes.
#[derive(Debug)]
pub struct StaleTasks;

impl StaleTasks {
    /// The zombie, endless and abandoned passes as of `now`, in that order.
    pub async fn sweep<P: StaleTaskStore>(port: &P, timeouts: &Timeouts, now: i64) -> Result<StaleSwept, P::Error> {
        let mut swept = StaleSwept::default();
        let silent = port.silent(before(now, timeouts.zombie_seconds)).await?;
        let reason = timeouts.zombie_reason();
        for task in silent {
            swept.zombies += usize::from(Self::time_out(port, task, &reason, Staleness::Silent, now).await?);
        }
        let endless = port.long_running(before(now, timeouts.endless_seconds)).await?;
        let reason = timeouts.endless_reason();
        for task in endless {
            swept.endless += usize::from(Self::time_out(port, task, &reason, Staleness::Endless, now).await?);
        }
        for settled in port.cancel_abandoned(before(now, timeouts.abandoned_seconds), now).await? {
            swept.abandoned += 1;
            port.settle(&settled).await?;
        }
        Ok(swept)
    }

    /// Fails running task `id` with `reason` in its log; deletes its runner when that was
    /// ephemeral. False when it was gone or no longer running.
    pub async fn time_out<P: StaleTaskStore>(
        port: &P,
        id: P::TaskId,
        reason: &str,
        staleness: Staleness,
        now: i64,
    ) -> Result<bool, P::Error> {
        let Some(task) = port.task(id).await? else { return Ok(false) };
        port.note(id, reason).await?;
        let Some(settled) = port.fail(id, staleness, now).await? else { return Ok(false) };
        port.close(id, reason).await?;
        port.settle(&settled).await?;
        port.retire_ephemeral_runner(&task).await?;
        Ok(true)
    }
}
