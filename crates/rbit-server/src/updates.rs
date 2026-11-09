//! Bounded background release checking, composed by the server.
//!
//! One owned task asks a [`ReleaseSource`] for the newest published release, decides
//! whether it supersedes the running version and publishes the result to the shared
//! [`UpdateNotices`] snapshot that pages read. Page rendering never reaches this module.
//! The production source is the fixed upstream feed ([`GithubReleases`]); it receives no
//! instance identity, credentials or running version.

mod github;
mod release;
#[cfg(test)]
mod tests;

use std::{future::Future, time::Duration};

use rbit_core::config::UpdateConfig;
use rbit_kernel::{TaskLifecycle, TaskStop};
use rbit_services::updates::{AvailableRelease, UpdateNotices, UpdateState};

use github::GithubReleases;
use release::Release;

/// How long shutdown waits for the checker, matching one bounded upstream request.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(10);

/// Where the newest published release comes from.
pub(crate) trait ReleaseSource: Send + Sync + 'static {
    /// The latest release record, or the upstream-advised retry delay when unavailable.
    fn latest(&self) -> impl Future<Output = Result<Release, Option<Duration>>> + Send;
}

/// Starts one checker when enabled. Disabled instances build no HTTP client and start no task.
pub(crate) fn start(config: &UpdateConfig, notices: UpdateNotices) -> TaskLifecycle {
    let owner = TaskLifecycle::new();
    if config.check_for_updates {
        let source = GithubReleases::new();
        if !source.ready() {
            // A failed optional client must not prevent the instance starting.
            notices.publish(UpdateState::Unavailable, None);
        }
        let checker = UpdateChecker { source, notices };
        let stop = owner.stop_signal();
        owner.start(checker.run(stop));
    }
    owner
}

/// Stops the checker, waiting at most for one bounded upstream request.
pub(crate) async fn shutdown(owner: &TaskLifecycle) {
    let _ = tokio::time::timeout(SHUTDOWN_WAIT, owner.shutdown()).await;
}

struct UpdateChecker<S> {
    source: S,
    notices: UpdateNotices,
}

impl<S: ReleaseSource> UpdateChecker<S> {
    async fn run(self, stop: TaskStop) {
        let mut failures = 0_u32;
        loop {
            let delay = tokio::select! {
                () = stop.cancelled() => return,
                delay = self.step(&mut failures) => delay,
            };
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(delay) => {}
            }
        }
    }

    /// Checks once, publishes the observation and returns the delay before the next check.
    async fn step(&self, failures: &mut u32) -> Duration {
        match self.check().await {
            Ok(release) => {
                *failures = 0;
                let state = if release.is_some() { UpdateState::Available } else { UpdateState::UpToDate };
                self.notices.publish(state, release);
                release::CACHE_TIME
            }
            Err(retry) => {
                *failures = failures.saturating_add(1);
                // Retain the last successful notice during an upstream outage.
                self.notices.publish(UpdateState::Unavailable, self.notices.status().release);
                release::retry_delay(*failures, retry)
            }
        }
    }

    async fn check(&self) -> Result<Option<AvailableRelease>, Option<Duration>> {
        let release = self.source.latest().await?;
        release::newer_release(release, env!("CARGO_PKG_VERSION")).map_err(|_| None)
    }
}
