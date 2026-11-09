//! Push mirror policy: limits, where a stored secret may follow an edit, the credential
//! user name, the push and debounce schedule, and what one push does. Remote URL
//! parsing, address checks, header encoding, Git, secret storage and the scheduler's
//! tasks stay with composition.
//!
//! A push is claimed first: the mirror's next regular push is due one interval later
//! whatever happens. Mirrors that no longer exist, and repositories that are gone or
//! archived (their mirrors pause until unarchived), are skipped. An empty repository has
//! nothing to mirror. The outcome (time and error) is recorded for the settings page;
//! failures that concern the remote are recorded, not returned.
use rbit_contracts::Timestamp;
use std::{future::Future, time::Duration};

/// How often the scheduler looks for due mirrors.
pub const SCHEDULER_TICK: Duration = Duration::from_secs(30);
/// Pushes running at the same time, instance-wide.
pub const MAX_CONCURRENT_PUSHES: usize = 2;
/// Due mirrors picked up per scheduler pass.
pub const DUE_BATCH: u32 = 16;
/// The user name sent when none is configured.
pub const DEFAULT_USERNAME: &str = "git";

/// Why another push mirror cannot be added to a repository that has `count` of `max`.
pub fn limit_error(count: u64, max: usize) -> Option<String> {
    (count >= u64::try_from(max).unwrap_or(u64::MAX)).then(|| format!("a repository can push to at most {max} mirrors"))
}

/// Where a remote sends credentials: an `http(s)` origin, or a local path (its own origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOrigin<'a> {
    Network { https: bool, host: &'a str, port: u16 },
    Local(&'a str),
}

/// Whether two remotes share a scheme, host (case-insensitively) and port. Local
/// remotes are their own origin, so any change to one counts.
pub fn same_origin(stored: Option<RemoteOrigin<'_>>, new: RemoteOrigin<'_>) -> bool {
    match (stored, new) {
        (Some(RemoteOrigin::Network { https: a, host: old, port: p }), RemoteOrigin::Network { https: b, host: new, port: q }) => {
            a == b && old.eq_ignore_ascii_case(new) && p == q
        }
        (Some(RemoteOrigin::Local(old)), RemoteOrigin::Local(new)) => old == new,
        _ => false,
    }
}

/// The error when an edit would let a stored secret follow the remote to another
/// origin: the secret only ever goes where it was entered for.
pub fn secret_must_be_entered_again(has_secret: bool, secret_given: bool, same_origin: bool) -> Option<&'static str> {
    (has_secret && !secret_given && !same_origin).then_some("the remote moved to another host; enter the password or token again for it")
}

/// The user name and secret a mirror pushes with: none without a secret; an empty user
/// name is sent as [`DEFAULT_USERNAME`].
pub fn credentials<'a>(username: &'a str, secret: Option<&'a str>) -> Option<(&'a str, &'a str)> {
    secret.map(|secret| (Some(username).filter(|name| !name.is_empty()).unwrap_or(DEFAULT_USERNAME), secret))
}

/// `seconds` after `now`, saturating.
pub fn after(now: Timestamp, seconds: u64) -> Timestamp {
    Timestamp(now.seconds().saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX)))
}

/// When a mirror claimed at `now` is next due for its regular push.
pub fn next_regular_push(now: Timestamp, interval_minutes: u64) -> Timestamp {
    after(now, interval_minutes * 60)
}

/// One push's surroundings, bound by composition.
pub trait MirrorPush: Sync {
    type Error: Send;
    type Id: Copy + Send + Sync;
    type Target: Send + Sync;
    type Repository: Send + Sync;
    fn target(&self, id: Self::Id) -> impl Future<Output = Result<Option<Self::Target>, Self::Error>> + Send;
    fn claim(&self, id: Self::Id, next: Timestamp) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn repository(&self, target: &Self::Target) -> impl Future<Output = Result<Option<Self::Repository>, Self::Error>> + Send;
    fn is_archived(&self, repository: &Self::Repository) -> bool;
    /// Checks the remote, then force-pushes branches and tags unless the repository is
    /// empty; the error is what the settings page shows.
    fn push(&self, repository: &Self::Repository, target: &Self::Target) -> impl Future<Output = Result<(), String>> + Send;
    /// Logs and counts the outcome.
    fn observe(&self, repository: &Self::Repository, id: Self::Id, error: Option<&str>);
    fn record(&self, id: Self::Id, at: Timestamp, error: Option<&str>) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Pushes mirror `id` now: claims it until an interval after `now()`, then records the
/// outcome at a fresh `now()`.
pub async fn push<P: MirrorPush>(port: &P, id: P::Id, interval_minutes: u64, now: impl Fn() -> Timestamp) -> Result<(), P::Error> {
    let Some(target) = port.target(id).await? else { return Ok(()) };
    port.claim(id, next_regular_push(now(), interval_minutes)).await?;
    let Some(repository) = port.repository(&target).await? else { return Ok(()) };
    if port.is_archived(&repository) {
        // Archived since it was picked: it resumes once the repository is unarchived.
        return Ok(());
    }
    let error = port.push(&repository, &target).await.err();
    port.observe(&repository, id, error.as_deref());
    port.record(id, now(), error.as_deref()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_user_name_is_sent_as_git() {
        assert_eq!(credentials("", Some("s3cret")), Some(("git", "s3cret")));
        assert_eq!(credentials("bot", Some("s3cret")), Some(("bot", "s3cret")));
        assert_eq!(credentials("bot", None), None);
    }

    #[test]
    fn secrets_follow_edits_only_within_one_origin() {
        let https = |host| RemoteOrigin::Network { https: true, host, port: 443 };
        assert!(same_origin(Some(https("Git.Example.com")), https("git.example.com")));
        assert!(!same_origin(Some(https("git.example.com")), RemoteOrigin::Network { https: false, host: "git.example.com", port: 443 }));
        assert!(!same_origin(Some(https("git.example.com")), RemoteOrigin::Network { https: true, host: "git.example.com", port: 8443 }));
        assert!(!same_origin(Some(https("a")), https("b")));
        assert!(!same_origin(None, https("a")), "an unreadable stored URL never matches");
        assert!(same_origin(Some(RemoteOrigin::Local("file:///a")), RemoteOrigin::Local("file:///a")));
        assert!(!same_origin(Some(RemoteOrigin::Local("file:///a")), https("a")));
        assert!(secret_must_be_entered_again(true, false, false).is_some());
        assert_eq!(secret_must_be_entered_again(true, true, false), None);
        assert_eq!(secret_must_be_entered_again(true, false, true), None);
        assert_eq!(secret_must_be_entered_again(false, false, false), None);
    }

    #[test]
    fn limits_and_times_keep_their_arithmetic() {
        assert_eq!(limit_error(9, 10), None);
        assert_eq!(limit_error(10, 10).as_deref(), Some("a repository can push to at most 10 mirrors"));
        assert_eq!(next_regular_push(Timestamp(100), 2), Timestamp(220));
        assert_eq!(after(Timestamp(i64::MAX - 1), 5), Timestamp(i64::MAX));
    }
}
