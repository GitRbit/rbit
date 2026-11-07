//! Protocol tokens contracts and original validation.
use super::limits::{MAX_IDEMPOTENCY_KEY_BYTES, RUNNER_CREDENTIAL_PREFIX, TASK_TOKEN_PREFIX, TOKEN_BODY_CHARS};

/// Whether `value` has the shape of a runner credential.
pub fn valid_runner_credential(value: &str) -> bool {
    valid_token(value, RUNNER_CREDENTIAL_PREFIX)
}

/// Whether `value` has the shape of a task token.
pub fn valid_task_token(value: &str) -> bool {
    valid_token(value, TASK_TOKEN_PREFIX)
}

fn valid_token(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|body| body.len() == TOKEN_BODY_CHARS && body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

/// Whether `value` is a usable [`crate::protocol::IDEMPOTENCY_KEY_HEADER`]: 1 to
/// [`crate::protocol::MAX_IDEMPOTENCY_KEY_BYTES`] of `[A-Za-z0-9_-]` (a UUID fits).
pub fn valid_idempotency_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDEMPOTENCY_KEY_BYTES
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
