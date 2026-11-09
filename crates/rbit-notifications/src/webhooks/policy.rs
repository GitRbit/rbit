//! Webhook limits and the retry decision after each attempt.
use rbit_contracts::Timestamp;
use std::time::Duration;

/// Hooks per repository or organisation.
pub const MAX_HOOKS_PER_OWNER: i64 = 20;
/// Longest accepted secret, in bytes.
pub const MAX_SECRET_LEN: usize = 256;
/// Delays before the second, third, ... attempt (about 10 s, 1 min, 10 min, 1 h, 6 h);
/// later attempts reuse the last one.
const RETRY_DELAYS_SECONDS: [u64; 5] = [10, 60, 600, 3_600, 21_600];
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// Why a hook secret is refused; the message is shown to the administrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    TooLong,
    ControlCharacter,
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLong => write!(out, "secret must be at most {MAX_SECRET_LEN} bytes"),
            Self::ControlCharacter => out.write_str("secret must not contain control characters"),
        }
    }
}

/// The secret to store: `None` for an empty one (deliveries are then unsigned).
pub fn check_secret(secret: &str) -> Result<Option<String>, SecretError> {
    if secret.is_empty() {
        return Ok(None);
    }
    if secret.len() > MAX_SECRET_LEN {
        return Err(SecretError::TooLong);
    }
    if secret.chars().any(char::is_control) {
        return Err(SecretError::ControlCharacter);
    }
    Ok(Some(secret.to_owned()))
}

/// Why another hook cannot be created on an owner that already has `count` hooks.
pub fn hook_limit_error(count: i64, repository: bool) -> Option<String> {
    let kind = if repository { "repository" } else { "organization" };
    (count >= MAX_HOOKS_PER_OWNER).then(|| format!("a {kind} can have at most {MAX_HOOKS_PER_OWNER} webhooks; delete one first"))
}

/// The page size and row offset of 1-based delivery log page `page`.
pub fn delivery_page(page: u32, per_page: u32) -> (u32, u32) {
    let per_page = per_page.max(1);
    (per_page, page.max(1).saturating_sub(1).saturating_mul(per_page))
}

/// Why a hook's event list is refused; the message is shown to the administrator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventsError {
    Unknown(String),
    NeedsActions(String),
    Empty,
}

impl std::fmt::Display for EventsError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(name) => write!(out, "unknown event \"{name}\"; choose from push, repository, release and fork"),
            Self::NeedsActions(name) => write!(out, "\"{name}\" needs Actions, which this instance does not have"),
            Self::Empty => out.write_str("choose at least one event"),
        }
    }
}

/// How a hook's event names are read: `parse` knows the wire names, `ping` is always
/// sent (and so never stored), and `needs_actions` events exist only with Actions on.
#[derive(Debug, Clone, Copy)]
pub struct EventCatalog<P, G, A> {
    pub parse: P,
    pub ping: G,
    pub needs_actions: A,
}

/// The events a hook subscribes to: trimmed names, `ping` dropped, sorted and
/// de-duplicated, at least one, and Actions events only while Actions is enabled.
pub fn subscribed_events<E, P, G, A>(
    names: &[String],
    catalog: &EventCatalog<P, G, A>,
    actions_enabled: bool,
) -> Result<Vec<E>, EventsError>
where
    E: Copy + Ord,
    P: Fn(&str) -> Option<E>,
    G: Fn(E) -> bool,
    A: Fn(E) -> bool,
{
    let mut events = Vec::new();
    for name in names {
        let name = name.trim();
        match (catalog.parse)(name) {
            Some(event) if (catalog.ping)(event) => {}
            Some(event) if !actions_enabled && (catalog.needs_actions)(event) => return Err(EventsError::NeedsActions(name.to_owned())),
            Some(event) => events.push(event),
            None => return Err(EventsError::Unknown(name.to_owned())),
        }
    }
    events.sort_unstable();
    events.dedup();
    if events.is_empty() {
        return Err(EventsError::Empty);
    }
    Ok(events)
}

/// How long to wait before retrying after `failed_attempts` failures. `random` in `[0, 1)`
/// spreads retries by plus or minus 20 % so a receiver that comes back is not hit by a
/// thundering herd.
pub fn retry_delay(failed_attempts: u32, random: f64) -> Duration {
    let index = usize::try_from(failed_attempts.saturating_sub(1)).unwrap_or(usize::MAX).min(RETRY_DELAYS_SECONDS.len() - 1);
    let base = Duration::from_secs(RETRY_DELAYS_SECONDS[index]);
    base.mul_f64(0.8 + 0.4 * random.clamp(0.0, 1.0))
}

/// What becomes of a delivery after one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextStep {
    Succeeded,
    /// Retry at this time.
    Retry(Timestamp),
    /// Out of attempts, or retrying cannot help.
    Failed,
}

impl NextStep {
    /// The metrics label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Retry(_) => "retrying",
            Self::Failed => "failed",
        }
    }
}

/// One attempt's outcome: 2xx succeeds; a refused or invalid target and the last allowed
/// attempt fail; anything else retries after [`retry_delay`] (the clock and randomness
/// are read only when a retry is scheduled).
pub fn next_step(
    succeeded: bool,
    permanent: bool,
    previous_attempts: u32,
    max_attempts: u32,
    now: impl FnOnce() -> Timestamp,
    random: impl FnOnce() -> f64,
) -> NextStep {
    let failures = previous_attempts + 1;
    if succeeded {
        NextStep::Succeeded
    } else if permanent || failures >= max_attempts.max(1) {
        NextStep::Failed
    } else {
        let delay = retry_delay(failures, random());
        NextStep::Retry(Timestamp(now().seconds() + i64::try_from(delay.as_secs()).unwrap_or(i64::MAX)))
    }
}

/// Finished deliveries last updated before this are pruned.
pub fn retention_cutoff(now: Timestamp, days: u32) -> Timestamp {
    Timestamp(now.seconds() - i64::from(days) * SECONDS_PER_DAY)
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
