//! The long poll's port: the scheduler's version and its `Notify`, the runner's idle
//! version, the claim and replay sessions, the poll's deadline, its replacement by a newer
//! poll and server shutdown. The loop itself is `rbit_actions::polling::LongPoll`.
use super::{ActionsService, CachedRunner, Claim, PollEnded, Resumed, RunnerError};
use rbit_actions::polling::{PollQueue, Wakeup};
use rbit_workflow::spec::JobSpec;
use std::{pin::pin, sync::atomic::Ordering};

pub(super) struct PollSession<'a> {
    service: &'a ActionsService,
    runner: &'a CachedRunner,
    key: Option<&'a str>,
    deadline: tokio::time::Instant,
    ended: &'a PollEnded,
}

impl<'a> PollSession<'a> {
    pub(super) fn new(
        service: &'a ActionsService,
        runner: &'a CachedRunner,
        key: Option<&'a str>,
        deadline: tokio::time::Instant,
        ended: &'a PollEnded,
    ) -> Self {
        Self { service, runner, key, deadline, ended }
    }
}

impl PollQueue for PollSession<'_> {
    type Error = RunnerError;
    type Spec = JobSpec;

    fn version(&self) -> i64 {
        self.service.tasks_version()
    }

    fn idle_version(&self) -> i64 {
        self.runner.idle_version.load(Ordering::SeqCst)
    }

    fn set_idle_version(&self, version: i64) {
        self.runner.idle_version.store(version, Ordering::SeqCst);
    }

    async fn resume(&self, key: &str) -> Result<Resumed, RunnerError> {
        self.service.resume(self.runner, key).await
    }

    async fn claim(&self) -> Result<Claim, RunnerError> {
        self.service.claim_for(self.runner, self.key).await
    }

    async fn wait(&self, seen: i64) -> Wakeup {
        // Registered before the version is read again, so a move in between still wakes it.
        let mut woken = pin!(self.service.inner.wake.notified());
        woken.as_mut().enable();
        if self.service.tasks_version() != seen {
            return Wakeup::Moved;
        }
        tokio::select! {
            () = woken => Wakeup::Moved,
            () = tokio::time::sleep_until(self.deadline) => Wakeup::Ended,
            () = self.ended.cancelled() => Wakeup::Ended,
            () = self.service.inner.shutdown.cancelled() => Wakeup::Ended,
        }
    }
}
