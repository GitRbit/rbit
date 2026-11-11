//! The runner's account of how a task ended, sent with the final state call
//! ([`super::TaskStateRequest::termination`]).
//!
//! A runner reports what only it knows: the reason, the step that ended the job and that
//! step's exit code. The server adds what only it knows (a lost runner, a cancellation, a
//! superseded run). The field is optional both ways: an older runner sends none, and the
//! server then records the reason as `unknown`; an older server ignores it. The reason is
//! a string on the wire so a newer runner's reason never makes an older server refuse the
//! whole call: the server reads a reason it does not know as `unknown`.

use serde::{Deserialize, Serialize};

use super::validation::{ValidationError, ensure};

/// The reasons a runner reports, in their wire spelling.
pub const RUNNER_REASONS: [&str; 5] = ["completed", "step_failed", "timeout", "cancelled", "setup_failed"];

/// The longest reason accepted on the wire.
pub const MAX_REASON_BYTES: usize = 32;

/// How a task ended, as its runner saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTermination {
    /// One of [`RUNNER_REASONS`] (or a newer runner's word).
    pub reason: String,
    /// The index of the step that ended the job, in the task's steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// That step's exit code, when it was a process that exited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl TaskTermination {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let reason = &self.reason;
        ensure(!reason.is_empty() && reason.len() <= MAX_REASON_BYTES, || {
            format!("a termination reason has 1 to {MAX_REASON_BYTES} bytes")
        })?;
        ensure(reason.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'_'), || {
            "a termination reason is lowercase letters and `_`".to_owned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_termination_is_small_and_optional_on_the_wire() {
        let failed = TaskTermination { reason: "step_failed".to_owned(), step: Some(3), exit_code: Some(101) };
        assert_eq!(serde_json::to_string(&failed).unwrap(), r#"{"reason":"step_failed","step":3,"exit_code":101}"#);
        let bare: TaskTermination = serde_json::from_str(r#"{"reason":"timeout"}"#).unwrap();
        assert_eq!((bare.step, bare.exit_code), (None, None));
        assert!(failed.validate().is_ok() && bare.validate().is_ok());
        for reason in RUNNER_REASONS {
            assert!(TaskTermination { reason: reason.to_owned(), step: None, exit_code: None }.validate().is_ok(), "{reason}");
        }
        for bad in ["", "Step Failed", "x\n", &"a".repeat(MAX_REASON_BYTES + 1)] {
            assert!(TaskTermination { reason: bad.to_owned(), step: None, exit_code: None }.validate().is_err(), "{bad:?}");
        }
    }
}
