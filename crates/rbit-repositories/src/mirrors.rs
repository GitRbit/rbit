//! Pull mirror policy: intervals, the scheduler's bounds, what one fetch does and which
//! default branch a new mirror adopts. Address checks, Git and storage stay behind
//! [`MirrorSync`]; the scheduler's tasks, permits and shutdown stay with composition.
//!
//! A fetch is skipped for repositories that are no longer mirrors or no longer exist,
//! and for archived ones, which are frozen until unarchived. Its progress is shown while
//! it runs and cleared on success, failure and cancellation alike. Its outcome (time and
//! error) is recorded for the settings page; failures that concern the upstream are
//! recorded, not returned.
use rbit_contracts::{RepoId, Timestamp};
use std::{future::Future, time::Duration};

/// How often the scheduler looks for due mirrors.
pub const SCHEDULER_TICK: Duration = Duration::from_secs(30);
/// Fetches running at the same time, instance-wide.
pub const MAX_CONCURRENT_FETCHES: usize = 2;
/// Due mirrors picked up per scheduler pass.
pub const DUE_BATCH: u32 = 16;
/// What a Git client is told when it pushes to a mirror.
pub const PUSH_REFUSED: &str = "This repository is a mirror; push to its upstream instead";
/// The longest interval, in minutes: a year.
const MAX_INTERVAL_MINUTES: u64 = 60 * 24 * 365;

/// The current stage of an active fetch. Transfer sizes are not available from Git,
/// so callers should show an indeterminate indicator rather than a percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorPhase {
    Connecting,
    Fetching,
    Finalizing,
}

/// The interval to store: `minutes`, or `default` when none was given, between
/// `min` minutes and a year.
pub fn checked_interval(minutes: Option<u64>, default: u64, min: u64) -> Result<u64, String> {
    let minutes = minutes.unwrap_or(default);
    if minutes < min || minutes > MAX_INTERVAL_MINUTES {
        return Err(format!("the interval must be between {min} minutes and a year"));
    }
    Ok(minutes)
}

/// Whether a fetched mirror must adopt another default branch: it has branches, but
/// not the one its default names (a new mirror of an upstream whose default is, say,
/// `master`). Only then is the upstream asked for its own default.
pub fn needs_default_branch(current: &str, branches: &[&str]) -> bool {
    !branches.is_empty() && !branches.contains(&current)
}

/// The branch to adopt: the upstream's default when it was fetched, otherwise the first.
pub fn adopted_branch(branches: &[&str], upstream: Option<String>) -> Option<String> {
    upstream.filter(|branch| branches.contains(&branch.as_str())).or_else(|| branches.first().map(|branch| (*branch).to_owned()))
}

/// One mirror fetch's surroundings, bound by composition.
pub trait MirrorSync: Sync {
    type Error: Send;
    type Mirror: Send + Sync;
    type Repository: Send + Sync;
    fn mirror(&self, repo: RepoId) -> impl Future<Output = Result<Option<Self::Mirror>, Self::Error>> + Send;
    /// The repository as the system sees it; `None` once it is gone.
    fn repository(&self, repo: RepoId) -> impl Future<Output = Result<Option<Self::Repository>, Self::Error>> + Send;
    fn is_archived(&self, repository: &Self::Repository) -> bool;
    /// Shows (`Some`) or clears (`None`) the fetch's progress.
    fn set_phase(&self, repo: RepoId, phase: Option<MirrorPhase>);
    /// Checks the upstream's addresses, fetches, reports the push and adopts a default
    /// branch; the error is what the settings page shows.
    fn fetch(&self, repository: &Self::Repository, mirror: &Self::Mirror) -> impl Future<Output = Result<(), String>> + Send;
    /// Logs and counts the outcome.
    fn observe(&self, repository: &Self::Repository, error: Option<&str>);
    fn record(&self, repo: RepoId, at: Timestamp, error: Option<&str>) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Clears the progress however the fetch ends, cancellation included.
struct Progress<'a, S: MirrorSync> {
    sync: &'a S,
    repo: RepoId,
}

impl<S: MirrorSync> Drop for Progress<'_, S> {
    fn drop(&mut self) {
        self.sync.set_phase(self.repo, None);
    }
}

/// Fetches mirror `repo` now and records the outcome at `now()`.
pub async fn sync<S: MirrorSync>(sync: &S, repo: RepoId, now: impl FnOnce() -> Timestamp) -> Result<(), S::Error> {
    let Some(mirror) = sync.mirror(repo).await? else { return Ok(()) };
    let Some(repository) = sync.repository(repo).await? else { return Ok(()) };
    if sync.is_archived(&repository) {
        // Archived mirrors are frozen: they fetch again once unarchived.
        return Ok(());
    }
    let _progress = Progress { sync, repo };
    sync.set_phase(repo, Some(MirrorPhase::Connecting));
    let error = sync.fetch(&repository, &mirror).await.err();
    sync.observe(&repository, error.as_deref());
    sync.record(repo, now(), error.as_deref()).await
}

impl<S: MirrorSync> std::fmt::Debug for Progress<'_, S> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Progress").field("repo", &self.repo).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_default_and_stay_between_the_minimum_and_a_year() {
        assert_eq!(checked_interval(None, 480, 10), Ok(480));
        assert_eq!(checked_interval(Some(10), 480, 10), Ok(10));
        assert_eq!(checked_interval(Some(MAX_INTERVAL_MINUTES), 480, 10), Ok(MAX_INTERVAL_MINUTES));
        let refused = Err("the interval must be between 10 minutes and a year".to_owned());
        assert_eq!(checked_interval(Some(9), 480, 10), refused);
        assert_eq!(checked_interval(Some(MAX_INTERVAL_MINUTES + 1), 480, 10), refused);
        assert_eq!(checked_interval(None, 5, 10), refused, "a default below the minimum is refused too");
    }

    #[test]
    fn a_missing_default_branch_adopts_the_upstreams_or_the_first() {
        assert!(!needs_default_branch("main", &[]));
        assert!(!needs_default_branch("main", &["dev", "main"]));
        assert!(needs_default_branch("main", &["dev", "master"]));
        assert_eq!(adopted_branch(&["dev", "master"], Some("master".into())).as_deref(), Some("master"));
        assert_eq!(adopted_branch(&["dev", "master"], Some("gone".into())).as_deref(), Some("dev"));
        assert_eq!(adopted_branch(&["dev", "master"], None).as_deref(), Some("dev"));
    }
}
