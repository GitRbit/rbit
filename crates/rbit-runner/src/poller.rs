//! `rbit-runner run`: declare, then long-poll for tasks while a slot is free (§12.5.4
//! "Capacity" and "Reconnect", §12.6 "Long poll").
//!
//! The runner declares its labels and capabilities at every start and learns the poll and
//! heartbeat intervals from the answer. It then holds at most one poll open, and only
//! while fewer than `capacity` tasks run; the poll says how many slots are free. Each poll
//! carries an idempotency key, which is kept across retries of the same poll and replaced
//! after a definite answer, so a task whose answer was lost in transit is handed out again
//! rather than stranded.
//!
//! An unreachable or busy server is retried with the reconnect backoff (1 s to 60 s, with
//! jitter, or the server's `Retry-After`). A refused credential, a removed runner or an
//! unsupported protocol stops the runner with a message saying what to do. An ephemeral
//! runner (`--once`) takes one task, waits for it and unregisters.

use std::{sync::Arc, time::Duration};

use anyhow::bail;
use rbit_workflow::protocol::{DeclareRequest, Intervals};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

use crate::{
    client::{self, Backoff, CallError, Claimed, Client, Disposition},
    config::Loaded,
    task::{self, Executor, Pacing},
};

/// The shortest time between two polls, so a server that answers `204` at once is not
/// polled in a tight loop.
const MIN_POLL_GAP: Duration = Duration::from_secs(1);
/// How often an ephemeral runner tries to unregister.
const UNREGISTER_ATTEMPTS: u32 = 5;

/// The poll loop of one runner.
#[derive(Debug)]
pub struct Poller<E> {
    client: Client,
    loaded: Loaded,
    executor: Arc<E>,
    once: bool,
    backoff: Backoff,
    min_poll_gap: Duration,
    /// Cancels every running task (the second stage of a shutdown).
    cancel_tasks: CancellationToken,
}

impl<E: Executor> Poller<E> {
    pub fn new(client: Client, loaded: Loaded, executor: Arc<E>, once: bool) -> Self {
        Self {
            client,
            loaded,
            executor,
            once,
            backoff: Backoff::RECONNECT,
            min_poll_gap: MIN_POLL_GAP,
            cancel_tasks: CancellationToken::new(),
        }
    }

    /// Running tasks are cancelled when `cancel_tasks` is.
    #[must_use]
    pub fn cancel_tasks_with(mut self, cancel_tasks: CancellationToken) -> Self {
        self.cancel_tasks = cancel_tasks;
        self
    }

    /// The same poller with a faster backoff, for tests.
    #[cfg(test)]
    pub fn with_backoff(mut self, backoff: Backoff, min_poll_gap: Duration) -> Self {
        self.backoff = backoff;
        self.min_poll_gap = min_poll_gap;
        self
    }

    /// Polls and runs tasks until `shutdown`, then waits for the running ones.
    pub async fn run(self, shutdown: CancellationToken) -> anyhow::Result<()> {
        let Some(intervals) = self.declare(&shutdown).await? else { return Ok(()) };
        let mut pacing = Pacing::new(intervals.heartbeat_seconds);
        pacing.backoff = self.backoff;
        pacing.cancel_all = self.cancel_tasks.clone();
        let long_poll = Duration::from_secs(intervals.long_poll_seconds.into());
        let capacity = if self.once { 1 } else { self.loaded.config.capacity };
        tracing::info!(
            name = %self.loaded.config.name,
            capacity,
            labels = ?self.loaded.config.labels.iter().map(|label| label.name.as_str()).collect::<Vec<_>>(),
            "runner started; waiting for jobs"
        );

        let slots = Arc::new(Semaphore::new(usize::try_from(capacity).unwrap_or(1)));
        let mut running = JoinSet::new();
        let mut backoff = self.backoff;
        let mut key = client::idempotency_key();
        let mut failure = None;
        loop {
            while running.try_join_next().is_some() {}
            let permit = tokio::select! {
                () = shutdown.cancelled() => break,
                permit = Arc::clone(&slots).acquire_owned() => match permit {
                    Ok(permit) => permit,
                    Err(_) => break,
                },
            };
            let free = u32::try_from(slots.available_permits() + 1).unwrap_or(capacity);
            let started = tokio::time::Instant::now();
            let answer = tokio::select! {
                () = shutdown.cancelled() => break,
                answer = self.client.poll(free, &key, long_poll) => answer,
            };
            match answer {
                Ok(claimed) => {
                    backoff.reset();
                    key = client::idempotency_key();
                    let Some(claimed) = claimed else {
                        let rest = self.min_poll_gap.saturating_sub(started.elapsed());
                        if !rest.is_zero() && !sleep_or_shutdown(rest, &shutdown).await {
                            break;
                        }
                        continue;
                    };
                    self.start(claimed, permit, &mut running, pacing.clone());
                    if self.once {
                        break;
                    }
                }
                Err(error) => match error.disposition() {
                    Disposition::Retry => {
                        let delay = backoff.after(&error);
                        tracing::warn!(%error, retry_in = ?delay, "poll failed");
                        if !sleep_or_shutdown(delay, &shutdown).await {
                            break;
                        }
                    }
                    Disposition::Refused => {
                        key = client::idempotency_key();
                        let delay = backoff.after(&error);
                        tracing::error!(%error, retry_in = ?delay, "the server refused the poll");
                        if !sleep_or_shutdown(delay, &shutdown).await {
                            break;
                        }
                    }
                    Disposition::Gone | Disposition::Fatal => {
                        failure = Some(error);
                        break;
                    }
                },
            }
        }

        if !running.is_empty() {
            tracing::info!(tasks = running.len(), "waiting for running tasks");
        }
        while running.join_next().await.is_some() {}
        if let Some(error) = failure {
            bail!(stop_message(&error));
        }
        if self.once {
            self.unregister().await;
        }
        Ok(())
    }

    /// Runs a claimed task in the background; its slot is free again when it ends.
    fn start(&self, claimed: Claimed, permit: OwnedSemaphorePermit, running: &mut JoinSet<()>, pacing: Pacing) {
        let (client, work_dir, executor) = (self.client.clone(), self.loaded.work_dir.clone(), Arc::clone(&self.executor));
        match claimed {
            Claimed::Task(spec) => {
                running.spawn(async move {
                    task::run(client, &work_dir, *spec, executor, pacing).await;
                    drop(permit);
                });
            }
            Claimed::Unreadable { task_id: Some(task_id), error } => {
                tracing::error!(task = %task_id, %error, "the server sent a job this runner cannot read");
                running.spawn(async move {
                    task::refuse(client, &work_dir, task_id, &error, pacing).await;
                    drop(permit);
                });
            }
            Claimed::Unreadable { task_id: None, error } => {
                tracing::error!(%error, "the server sent an unreadable task; it will time out on the server");
            }
        }
    }

    /// `POST declare`, retried while the server is unreachable. `None` on shutdown.
    async fn declare(&self, shutdown: &CancellationToken) -> anyhow::Result<Option<Intervals>> {
        let config = &self.loaded.config;
        let request =
            DeclareRequest { name: config.name.clone(), labels: config.declared_labels(), capabilities: config.capabilities(self.once) };
        let mut backoff = self.backoff;
        loop {
            match self.client.declare(&request).await {
                Ok(answer) => {
                    tracing::info!(runner = answer.runner_id.0, server = %config.url, "declared to the server");
                    return Ok(Some(answer.intervals));
                }
                Err(error) if error.disposition() == Disposition::Retry => {
                    let delay = backoff.after(&error);
                    tracing::warn!(%error, retry_in = ?delay, "cannot reach the server");
                    if !sleep_or_shutdown(delay, shutdown).await {
                        return Ok(None);
                    }
                }
                Err(error) => bail!(stop_message(&error)),
            }
        }
    }

    async fn unregister(&self) {
        let mut backoff = self.backoff;
        for attempt in 1..=UNREGISTER_ATTEMPTS {
            match self.client.unregister().await {
                Ok(()) => {
                    tracing::info!("ephemeral runner unregistered");
                    return;
                }
                Err(error) if error.disposition() == Disposition::Retry && attempt < UNREGISTER_ATTEMPTS => {
                    tokio::time::sleep(backoff.after(&error)).await;
                }
                Err(error) => {
                    tracing::warn!(%error, "cannot unregister; remove the runner on the runners page");
                    return;
                }
            }
        }
    }
}

/// Why the runner stops, and what to do about it.
fn stop_message(error: &CallError) -> String {
    match error.code() {
        Some(rbit_workflow::protocol::ErrorCode::Unauthenticated) => {
            format!("{error}. The runner credential was refused: the runner was removed or its credential is wrong; register again")
        }
        Some(rbit_workflow::protocol::ErrorCode::NotFound) => {
            format!("{error}. The server does not know this runner (or has Actions switched off); register again")
        }
        _ => error.to_string(),
    }
}

/// Sleeps for `delay`; `false` when `shutdown` came first.
async fn sleep_or_shutdown(delay: Duration, shutdown: &CancellationToken) -> bool {
    tokio::select! {
        () = shutdown.cancelled() => false,
        () = tokio::time::sleep(delay) => true,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rbit_workflow::protocol::{Backend, StepStatus, TaskId, TaskResult};

    use super::*;
    use crate::testing::{FakeServer, FnExecutor, job_spec};

    fn fast() -> Backoff {
        Backoff::new(Duration::from_millis(20), Duration::from_millis(100))
    }

    fn poller<E: Executor>(server: &FakeServer, work: &std::path::Path, executor: E, once: bool) -> Poller<E> {
        Poller::new(server.client(), server.loaded(work), Arc::new(executor), once).with_backoff(fast(), Duration::from_millis(10))
    }

    /// Runs each step, logging one line, and sets an output.
    fn succeed() -> FnExecutor {
        FnExecutor::new(|spec, reporter| async move {
            for index in 0..spec.template.steps.len() {
                reporter.start_step(index);
                reporter.log(&format!("step {index}"));
                reporter.finish_step(index, StepStatus::Success);
            }
            reporter.set_outputs([("version".to_owned(), "1.2.3".to_owned())].into());
            TaskResult::Success
        })
    }

    #[tokio::test]
    async fn an_ephemeral_runner_runs_one_task_and_unregisters() {
        let server = FakeServer::start().await;
        server.push_task(job_spec(7, 2));
        server.push_task(job_spec(8, 1));
        let work = tempfile::tempdir().unwrap();
        poller(&server, work.path(), succeed(), true).run(CancellationToken::new()).await.unwrap();

        let declares = server.declares();
        assert!(declares[0].capabilities.ephemeral && declares[0].capabilities.capacity == 1);
        assert_eq!(declares[0].labels[0].backend, Backend::Container);
        let polls = server.polls();
        assert_eq!(polls.len(), 1, "one task, one poll");
        assert!(polls[0].key.as_deref().is_some_and(rbit_workflow::protocol::valid_idempotency_key));
        assert_eq!(polls[0].free_slots, 1);

        assert_eq!(server.log(TaskId(7)), ["step 0", "step 1"]);
        let last = server.states(TaskId(7)).pop().unwrap();
        assert_eq!(last.result, Some(TaskResult::Success));
        assert_eq!(last.outputs["version"], "1.2.3");
        let ranges: Vec<_> = last.steps.iter().map(|step| (step.status, step.log_index, step.log_length)).collect();
        assert_eq!(ranges, [(StepStatus::Success, 0, 1), (StepStatus::Success, 1, 1)]);
        assert_eq!(server.unregistered(), 1);
        assert!(server.states(TaskId(8)).is_empty(), "the second task stays queued");
        assert!(!crate::task::task_dir(work.path(), TaskId(7)).exists(), "the task directory is removed");
    }

    #[tokio::test]
    async fn retried_polls_keep_their_idempotency_key() {
        let server = FakeServer::start().await;
        server.fail_next("poll", 503, 2);
        server.push_task(job_spec(1, 1));
        let work = tempfile::tempdir().unwrap();
        poller(&server, work.path(), succeed(), true).run(CancellationToken::new()).await.unwrap();
        let keys: Vec<_> = server.polls().into_iter().map(|poll| poll.key.unwrap()).collect();
        assert_eq!(keys.len(), 3);
        assert!(keys.iter().all(|key| *key == keys[0]), "{keys:?}");

        // After a definite answer the next poll has a new key.
        let server = FakeServer::start().await;
        let work = tempfile::tempdir().unwrap();
        let shutdown = CancellationToken::new();
        let runner = tokio::spawn(poller(&server, work.path(), succeed(), false).run(shutdown.clone()));
        server.wait_for(|server| server.polls().len() >= 3).await;
        shutdown.cancel();
        runner.await.unwrap().unwrap();
        let keys: Vec<_> = server.polls().into_iter().map(|poll| poll.key.unwrap()).collect();
        assert!(keys.windows(2).all(|pair| pair[0] != pair[1]), "{keys:?}");
    }

    #[tokio::test]
    async fn polls_wait_for_a_free_slot() {
        let server = FakeServer::start().await;
        for id in 1..=3 {
            server.push_task(job_spec(id, 1));
        }
        let release = CancellationToken::new();
        let gate = release.clone();
        let executor = FnExecutor::new(move |_, reporter| {
            let gate = gate.clone();
            async move {
                reporter.start_step(0);
                gate.cancelled().await;
                reporter.finish_step(0, StepStatus::Success);
                TaskResult::Success
            }
        });
        let work = tempfile::tempdir().unwrap();
        let shutdown = CancellationToken::new();
        let runner = tokio::spawn(poller(&server, work.path(), executor, false).run(shutdown.clone()));
        // Capacity 2: two tasks start, and no third poll is made while both run.
        server.wait_for(|server| server.polls().len() == 2 && !server.states(TaskId(2)).is_empty()).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(server.polls().iter().map(|poll| poll.free_slots).collect::<Vec<_>>(), [2, 1]);
        release.cancel();
        server.wait_for(|server| server.final_state(TaskId(3)).is_some()).await;
        shutdown.cancel();
        runner.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn an_unsupported_protocol_or_refused_credential_stops_the_runner() {
        let server = FakeServer::start().await;
        server.fail_next("declare", 426, 1);
        let work = tempfile::tempdir().unwrap();
        let error = poller(&server, work.path(), succeed(), false).run(CancellationToken::new()).await.unwrap_err().to_string();
        assert!(error.contains("does not speak runner protocol 1") && error.contains("it speaks 1"), "{error}");

        server.fail_next("poll", 401, 1);
        let error = poller(&server, work.path(), succeed(), false).run(CancellationToken::new()).await.unwrap_err().to_string();
        assert!(error.contains("register again"), "{error}");
    }

    #[tokio::test]
    async fn throttled_polls_wait_for_retry_after() {
        let server = FakeServer::start().await;
        server.fail_next_after("poll", 429, 1, 1);
        server.push_task(job_spec(1, 1));
        let work = tempfile::tempdir().unwrap();
        let started = tokio::time::Instant::now();
        poller(&server, work.path(), succeed(), true).run(CancellationToken::new()).await.unwrap();
        assert!(started.elapsed() >= Duration::from_secs(1), "Retry-After: 1 was honoured");
        assert_eq!(server.polls().len(), 2);
    }

    /// Waits for cancellation, then reports the step cancelled.
    fn wait_for_cancel(seen: Arc<Mutex<Vec<TaskId>>>) -> FnExecutor {
        FnExecutor::new(move |spec, reporter| {
            let seen = Arc::clone(&seen);
            async move {
                reporter.start_step(0);
                reporter.log("working");
                reporter.cancellation().cancelled().await;
                seen.lock().unwrap().push(spec.task_id);
                reporter.finish_step(0, StepStatus::Cancelled);
                TaskResult::Cancelled
            }
        })
    }

    #[tokio::test]
    async fn the_server_cancels_through_the_heartbeat() {
        let server = FakeServer::start().await;
        server.push_task(job_spec(4, 1));
        let seen = Arc::default();
        let work = tempfile::tempdir().unwrap();
        let runner = tokio::spawn(poller(&server, work.path(), wait_for_cancel(Arc::clone(&seen)), true).run(CancellationToken::new()));
        server.wait_for(|server| server.log(TaskId(4)) == ["working"]).await;
        server.cancel(TaskId(4));
        runner.await.unwrap().unwrap();
        assert_eq!(*seen.lock().unwrap(), [TaskId(4)]);
        let last = server.final_state(TaskId(4)).unwrap();
        assert_eq!((last.result, last.steps[0].status), (Some(TaskResult::Cancelled), StepStatus::Cancelled));
    }

    #[tokio::test]
    async fn a_task_the_server_forgot_is_cancelled_locally() {
        let server = FakeServer::start().await;
        server.push_task(job_spec(5, 1));
        let seen = Arc::default();
        let work = tempfile::tempdir().unwrap();
        let runner = tokio::spawn(poller(&server, work.path(), wait_for_cancel(Arc::clone(&seen)), true).run(CancellationToken::new()));
        server.wait_for(|server| !server.states(TaskId(5)).is_empty()).await;
        server.forget(TaskId(5));
        runner.await.unwrap().unwrap();
        assert_eq!(*seen.lock().unwrap(), [TaskId(5)]);
        assert!(server.final_state(TaskId(5)).is_none(), "nothing is reported for a forgotten task");
    }

    #[tokio::test]
    async fn the_end_of_a_task_survives_an_outage() {
        let server = FakeServer::start().await;
        server.push_task(job_spec(6, 1));
        // The first heartbeat gets through; then the server is away for a while.
        let executor = {
            let server = server.clone();
            FnExecutor::new(move |_, reporter| {
                let server = server.clone();
                async move {
                    reporter.start_step(0);
                    server.wait_for(|server| !server.states(TaskId(6)).is_empty()).await;
                    server.fail_next("state", 503, 4);
                    server.fail_next("logs", 503, 4);
                    for line in 0..100 {
                        reporter.log(&format!("line {line}"));
                    }
                    reporter.finish_step(0, StepStatus::Success);
                    TaskResult::Success
                }
            })
        };
        let work = tempfile::tempdir().unwrap();
        poller(&server, work.path(), executor, true).run(CancellationToken::new()).await.unwrap();
        assert_eq!(server.log(TaskId(6)).len(), 100);
        assert_eq!(server.final_state(TaskId(6)).unwrap().result, Some(TaskResult::Success));
    }

    #[tokio::test]
    async fn a_shutdown_cancels_running_tasks_after_it_stops_polling() {
        let server = FakeServer::start().await;
        server.push_task(job_spec(11, 1));
        let seen = Arc::default();
        let work = tempfile::tempdir().unwrap();
        let (stop_polling, cancel_tasks) = (CancellationToken::new(), CancellationToken::new());
        let poller = poller(&server, work.path(), wait_for_cancel(Arc::clone(&seen)), false).cancel_tasks_with(cancel_tasks.clone());
        let runner = tokio::spawn(poller.run(stop_polling.clone()));
        server.wait_for(|server| server.log(TaskId(11)) == ["working"]).await;
        stop_polling.cancel();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(seen.lock().unwrap().is_empty(), "the running task keeps going during the grace period");
        cancel_tasks.cancel();
        runner.await.unwrap().unwrap();
        assert_eq!(*seen.lock().unwrap(), [TaskId(11)]);
        assert_eq!(server.final_state(TaskId(11)).unwrap().result, Some(TaskResult::Cancelled));
    }

    #[tokio::test]
    async fn unreadable_tasks_are_failed() {
        let server = FakeServer::start().await;
        let mut spec = job_spec(9, 1);
        spec.token = rbit_workflow::secret::Secret::new("rbat_malformed");
        server.push_task(spec);
        let work = tempfile::tempdir().unwrap();
        poller(&server, work.path(), succeed(), true).run(CancellationToken::new()).await.unwrap();
        assert_eq!(server.final_state(TaskId(9)).unwrap().result, Some(TaskResult::Failure));
        assert!(server.log(TaskId(9))[0].contains("task token is malformed"));
    }
}
