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
