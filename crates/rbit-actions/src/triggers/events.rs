//! Decisions for the events that do not come from a push or a pull request:
//! `workflow_dispatch`, `repository_dispatch`, `workflow_run` and `schedule`.
//!
//! Invariants: a dispatch names one of the repository's own branches or tags, never
//! `refs/pull/*` or a commit id, and a branch wins over a tag of the same name;
//! `repository_dispatch` input follows GitHub's limits; `workflow_run` chains stop after
//! [`MAX_WORKFLOW_RUN_CHAIN`] levels; a schedule entry waits while the previous scheduled
//! run of its workflow still waits for a runner.

use std::time::Duration;

use super::RunStage;

/// How many `workflow_run` levels may follow one another.
pub const MAX_WORKFLOW_RUN_CHAIN: usize = 3;
/// The longest `event_type` of a `repository_dispatch`.
pub const MAX_DISPATCH_EVENT_TYPE_CHARS: usize = 100;
/// The most top-level properties of a `client_payload`.
pub const MAX_CLIENT_PAYLOAD_PROPERTIES: usize = 10;
/// The longest `client_payload`, as JSON.
pub const MAX_CLIENT_PAYLOAD_CHARS: usize = 65_535;
/// Why a `client_payload` that is neither absent, `null` nor an object is refused.
pub const CLIENT_PAYLOAD_NOT_OBJECT: &str = "client_payload must be an object";
/// Why a dispatch ref is refused.
pub const REF_REFUSED: &str = "the ref must be a branch or tag of this repository";
/// How often the schedule ticker looks for due entries.
pub const TICK: Duration = Duration::from_secs(60);
/// Consecutive workflow errors that pause a schedule entry.
pub const PAUSE_AFTER_FAILURES: i64 = 3;
/// Schedule entries started by one tick at most; the rest are due on the next.
pub const DUE_PER_TICK: u32 = 100;
/// Seconds until an entry whose expression can no longer be read is looked at again.
pub const RETRY_UNREADABLE: i64 = 24 * 60 * 60;

/// Whether a run that `depth` `workflow_run` runs led to may start more.
pub fn chain_continues(depth: usize) -> bool {
    depth < MAX_WORKFLOW_RUN_CHAIN
}

/// A `repository_dispatch` event type, trimmed, or why it is refused.
pub fn dispatch_event_type(raw: &str) -> Result<&str, String> {
    let event_type = raw.trim();
    if event_type.is_empty() || event_type.chars().count() > MAX_DISPATCH_EVENT_TYPE_CHARS {
        return Err(format!("event_type must be 1 to {MAX_DISPATCH_EVENT_TYPE_CHARS} characters"));
    }
    Ok(event_type)
}

/// Why an object `client_payload` is refused; `json_chars` is measured only when needed.
pub fn client_payload_problem(properties: usize, json_chars: impl FnOnce() -> usize) -> Option<String> {
    if properties > MAX_CLIENT_PAYLOAD_PROPERTIES {
        return Some(format!("client_payload may have at most {MAX_CLIENT_PAYLOAD_PROPERTIES} top-level properties"));
    }
    (json_chars() > MAX_CLIENT_PAYLOAD_CHARS).then(|| format!("client_payload may be at most {MAX_CLIENT_PAYLOAD_CHARS} characters"))
}

/// The kind of ref a dispatch may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefSide {
    Branch,
    Tag,
}

impl RefSide {
    /// The full ref of `name` on this side.
    pub fn full_ref(self, name: &str) -> String {
        match self {
            Self::Branch => format!("refs/heads/{name}"),
            Self::Tag => format!("refs/tags/{name}"),
        }
    }
}

/// Where to look for the ref a dispatch names, in order, and the short name to find;
/// `None` refuses it ([`REF_REFUSED`]): empty, or a full ref outside branches and tags.
pub fn dispatch_ref(given: &str) -> Option<(&'static [RefSide], &str)> {
    let given = given.trim();
    if let Some(branch) = given.strip_prefix("refs/heads/") {
        Some((&[RefSide::Branch], branch))
    } else if let Some(tag) = given.strip_prefix("refs/tags/") {
        Some((&[RefSide::Tag], tag))
    } else if given.starts_with("refs/") || given.is_empty() {
        None
    } else {
        Some((&[RefSide::Branch, RefSide::Tag], given))
    }
}

/// Whether a due schedule entry is skipped because its workflow's previous scheduled run
/// still waits: waiting or held, not awaiting approval or started.
pub fn schedule_waits_for(previous: RunStage) -> bool {
    matches!(previous, RunStage::Waiting | RunStage::Held)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_and_repository_dispatch_follow_github_limits() {
        assert!(chain_continues(2) && !chain_continues(MAX_WORKFLOW_RUN_CHAIN));
        assert_eq!(dispatch_event_type("  deploy "), Ok("deploy"));
        assert_eq!(dispatch_event_type(" ").unwrap_err(), "event_type must be 1 to 100 characters");
        assert!(dispatch_event_type(&"é".repeat(100)).is_ok() && dispatch_event_type(&"x".repeat(101)).is_err());
        assert_eq!(client_payload_problem(10, || 65_535), None);
        assert_eq!(client_payload_problem(11, || unreachable!()).unwrap(), "client_payload may have at most 10 top-level properties");
        assert_eq!(client_payload_problem(1, || 65_536).unwrap(), "client_payload may be at most 65535 characters");
    }

    #[test]
    fn dispatch_refs_are_own_branches_then_tags() {
        assert_eq!(dispatch_ref("refs/heads/main"), Some((&[RefSide::Branch][..], "main")));
        assert_eq!(dispatch_ref("refs/tags/v1"), Some((&[RefSide::Tag][..], "v1")));
        assert_eq!(dispatch_ref(" v1 "), Some((&[RefSide::Branch, RefSide::Tag][..], "v1")));
        for refused in ["", "  ", "refs/pull/1/head", "refs/remotes/x"] {
            assert_eq!(dispatch_ref(refused), None, "{refused}");
        }
        assert_eq!(RefSide::Tag.full_ref("v1"), "refs/tags/v1");
        assert!(schedule_waits_for(RunStage::Held) && !schedule_waits_for(RunStage::AwaitingApproval));
    }
}
