//! Runner decisions that need no store: registration-token terms, who manages runners,
//! label pins and changes, when last-seen times are written and shown, the intervals
//! runners keep, and the idempotency keys a runner's recent claims are remembered by.

use std::{collections::VecDeque, future::Future, time::Duration};

use rbit_workflow::protocol::{Intervals, MAX_HEARTBEAT_INTERVAL, MAX_LONG_POLL};

/// How long a registration token lasts unless its creator chose otherwise.
pub const DEFAULT_REGISTRATION_LIFETIME: Duration = Duration::from_hours(1);
/// The longest a registration token may last.
pub const MAX_REGISTRATION_LIFETIME: Duration = Duration::from_hours(30 * 24);
/// `last_online` is written at most this often per runner.
pub const SEEN_GRANULARITY_SECONDS: i64 = 60;
/// Idempotency keys of claims remembered per runner, newest last.
pub const REMEMBERED_KEYS: usize = 32;

/// How a new registration token may be used (§11.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationOptions<L> {
    /// At most [`MAX_REGISTRATION_LIFETIME`].
    pub lifetime: Duration,
    /// `None`: unlimited.
    pub uses: Option<u32>,
    /// Runners registered with it may declare only these labels.
    pub labels: Option<Vec<L>>,
}

impl<L> Default for RegistrationOptions<L> {
    /// Single-use, for an hour, any labels (§11.9).
    fn default() -> Self {
        Self { lifetime: DEFAULT_REGISTRATION_LIFETIME, uses: Some(1), labels: None }
    }
}

impl<L> RegistrationOptions<L> {
    /// When a token with these options, created at `now`, expires; or why the options are
    /// refused. At most `max_labels` labels may be pinned.
    pub fn expires_at(&self, now: i64, max_labels: usize) -> Result<i64, String> {
        if self.lifetime.is_zero() || self.lifetime > MAX_REGISTRATION_LIFETIME {
            return Err("a registration token lasts from a minute up to 30 days".to_owned());
        }
        if self.uses == Some(0) {
            return Err("a registration token must allow at least one registration".to_owned());
        }
        if self.labels.as_ref().is_some_and(|labels| labels.is_empty() || labels.len() > max_labels) {
            return Err(format!("pin 1 to {max_labels} labels, or none to allow any"));
        }
        let seconds = i64::try_from(self.lifetime.as_secs()).unwrap_or(i64::MAX).max(60);
        Ok(now.saturating_add(seconds))
    }

    /// The audit detail of token `id` expiring at `expires_at`.
    pub fn audit_detail(&self, id: impl std::fmt::Display, expires_at: i64) -> String {
        let uses = self.uses.map_or_else(|| "unlimited uses".to_owned(), |uses| format!("{uses} use(s)"));
        format!("#{id} until {expires_at}, {uses}")
    }
}

/// Where runners are managed from, as the manager check sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedTarget {
    /// The administration panel.
    Instance,
    /// A user's or an organisation's settings.
    Owner { own_account: bool, organization: bool },
    /// A repository's settings, with the actor's access.
    Repository { admin: bool, read: bool },
}

/// What a manager check decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Management {
    Allowed,
    /// A reader of the repository: refused openly.
    Forbidden,
    /// Others learn nothing.
    Hidden,
}

/// Who manages runners: administrators the instance's; a user their own, an
/// organisation's owners (and administrators) the organisation's; repository
/// administrators the repository's. `organization_owner` is asked only when it decides.
pub async fn management<E, F: Future<Output = Result<bool, E>>>(
    administrator: bool,
    target: ManagedTarget,
    organization_owner: impl FnOnce() -> F,
) -> Result<Management, E> {
    let allowed = match target {
        ManagedTarget::Instance => administrator,
        ManagedTarget::Owner { own_account, organization } => {
            administrator || own_account || (organization && organization_owner().await?)
        }
        ManagedTarget::Repository { admin: true, .. } => true,
        ManagedTarget::Repository { read: true, .. } => return Ok(Management::Forbidden),
        ManagedTarget::Repository { .. } => false,
    };
    Ok(if allowed { Management::Allowed } else { Management::Hidden })
}

/// The first declared label its registration token's pin does not allow.
pub fn unpinned_label<'a, L: PartialEq>(pinned: Option<&[L]>, declared: &'a [L]) -> Option<&'a L> {
    let pinned = pinned?;
    declared.iter().find(|label| !pinned.contains(label))
}

/// Whether a runner's labels or host labels changed, order aside: the scheduler's version
/// moves, so jobs it can now take are offered to it.
pub fn labels_changed<L: Ord + Clone>(before: (&[L], &[L]), after: (&[L], &[L])) -> bool {
    let sorted = |labels: &[L]| {
        let mut labels = labels.to_vec();
        labels.sort();
        labels
    };
    (sorted(before.0), sorted(before.1)) != (sorted(after.0), sorted(after.1))
}

/// Whether a runner seen at `seen` and last written at `written` is written now: seen
/// since, and at least [`SEEN_GRANULARITY_SECONDS`] later.
pub fn seen_is_due(seen: i64, written: i64) -> bool {
    seen > written && seen - written >= SEEN_GRANULARITY_SECONDS
}

/// The `last_online` a list shows: what this process saw last when it is newer than the
/// stored value (written at most once a minute), else the stored value.
pub fn shown_online(stored: Option<i64>, seen: Option<i64>) -> Option<i64> {
    match seen {
        Some(seen) if seen > 0 && stored.is_none_or(|stored| stored < seen) => Some(seen),
        _ => stored,
    }
}

/// The intervals runners are told to keep: `long_poll_seconds` within the protocol's
/// longest poll, and the protocol's heartbeat.
pub fn intervals(long_poll_seconds: u64) -> Intervals {
    let long_poll = long_poll_seconds.clamp(1, MAX_LONG_POLL.as_secs());
    Intervals {
        long_poll_seconds: u32::try_from(long_poll).unwrap_or(1),
        heartbeat_seconds: u32::try_from(MAX_HEARTBEAT_INTERVAL.as_secs()).unwrap_or(10),
    }
}

/// Whether a repository's settings list a runner: instance runners only when they serve its
/// owner.
pub fn listed_for_repository(instance_runner: bool, instance_runners_allowed: bool) -> bool {
    !instance_runner || instance_runners_allowed
}

/// The idempotency keys of a runner's recent claims and their tasks, newest last, at most
/// [`REMEMBERED_KEYS`]: a retried poll is recognised without a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentKeys<T> {
    keys: VecDeque<(String, T)>,
}

impl<T> Default for RecentKeys<T> {
    fn default() -> Self {
        Self { keys: VecDeque::new() }
    }
}

impl<T: Copy> RecentKeys<T> {
    /// The task the claim with `key` gave, if remembered.
    pub fn remembered(&self, key: &str) -> Option<T> {
        self.keys.iter().rev().find(|(known, _)| known == key).map(|(_, task)| *task)
    }

    /// Remembers that the claim with `key` gave `task`, forgetting the oldest beyond the cap.
    pub fn remember(&mut self, key: &str, task: T) {
        if self.keys.len() == REMEMBERED_KEYS {
            self.keys.pop_front();
        }
        self.keys.push_back((key.to_owned(), task));
    }
}
