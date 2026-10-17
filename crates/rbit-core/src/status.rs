//! Commit statuses: what external CI reports about a commit, and the decisions made from
//! the reports — a commit's combined state ([`combined_state`]) and whether the required
//! checks of a protected branch pass ([`required_checks`]).
//!
//! Milestone 3 workstream S (commit statuses and required checks) owns this file.
//!
//! The model is GitHub's commit status API, so existing CI integrations work unchanged:
//! * a status has a `state` (`pending`, `success`, `failure`, `error`), a `context` that
//!   names the check (default `default`), an optional `description` (≤
//!   [`MAX_DESCRIPTION_CHARS`] characters) and an optional `target_url` (an absolute
//!   `http`/`https` URL);
//! * every report is kept; for each `(repository, commit, context)` the **latest** report
//!   wins, and at most [`MAX_STATUSES_PER_CONTEXT`] reports are kept per triple;
//! * the combined state is `failure` if any latest status is `failure` or `error`,
//!   `pending` if there are none or any is `pending`, and `success` otherwise.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{AccountId, RepoId, Timestamp, pull::ChecksStatus, webhook::WebhookUrl};

/// The context of a status posted without one.
pub const DEFAULT_CONTEXT: &str = "default";
/// Longest context, in characters.
pub const MAX_CONTEXT_CHARS: usize = 255;
/// Longest description, in characters (GitHub's limit).
pub const MAX_DESCRIPTION_CHARS: usize = 140;
/// Reports kept per commit and context (GitHub's limit); further ones are refused.
pub const MAX_STATUSES_PER_CONTEXT: u32 = 1000;
/// Most required contexts per branch protection rule.
pub const MAX_REQUIRED_CONTEXTS: usize = 50;

/// Database id of one status report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StatusId(pub i64);

/// The state a status reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusState {
    Pending,
    Success,
    Failure,
    /// The check could not run (as opposed to running and failing).
    Error,
}

impl StatusState {
    pub const ALL: [Self; 4] = [Self::Pending, Self::Success, Self::Failure, Self::Error];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }

    /// `failure` and `error`.
    pub const fn is_failed(self) -> bool {
        matches!(self, Self::Failure | Self::Error)
    }
}

impl fmt::Display for StatusState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a submitted status was rejected. The message is shown to the caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatusError {
    #[error("state must be one of pending, success, failure or error")]
    State,
    #[error("context must not be empty")]
    EmptyContext,
    #[error("context must be at most {MAX_CONTEXT_CHARS} characters")]
    ContextTooLong,
    #[error("context must not contain control characters")]
    ContextCharacters,
    #[error("description must be at most {MAX_DESCRIPTION_CHARS} characters")]
    DescriptionTooLong,
    #[error("description must not contain control characters")]
    DescriptionCharacters,
    #[error("target_url must be an absolute http or https URL ({0})")]
    TargetUrl(String),
    #[error("sha must be a full commit id")]
    Sha,
}

/// The name of a check (`ci/build`, `continuous-integration/jenkins`): trimmed, 1 to
/// [`MAX_CONTEXT_CHARS`] characters, no control characters. Case-sensitive, as on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatusContext(String);

impl StatusContext {
    pub fn parse(value: &str) -> Result<Self, StatusError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(StatusError::EmptyContext);
        }
        if value.chars().count() > MAX_CONTEXT_CHARS {
            return Err(StatusError::ContextTooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(StatusError::ContextCharacters);
        }
        Ok(Self(value.to_owned()))
    }

    /// `default`.
    pub fn default_context() -> Self {
        Self(DEFAULT_CONTEXT.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StatusContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for StatusContext {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A full commit id: 40 (SHA-1) or 64 (SHA-256) hexadecimal digits, lower-cased.
pub fn parse_sha(value: &str) -> Result<String, StatusError> {
    let value = value.trim();
    if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err(StatusError::Sha)
    }
}

/// A validated status as submitted, before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    pub state: StatusState,
    pub context: StatusContext,
    /// `None` when absent or blank.
    pub description: Option<String>,
    /// `None` when absent or blank.
    pub target_url: Option<String>,
}

impl StatusReport {
    /// Validates the fields of a submitted status: an absent or blank context is
    /// [`DEFAULT_CONTEXT`], blank descriptions and URLs are absent.
    pub fn parse(state: &str, context: Option<&str>, description: Option<&str>, target_url: Option<&str>) -> Result<Self, StatusError> {
        let state = StatusState::parse(state.trim()).ok_or(StatusError::State)?;
        let context = match context.map(str::trim).filter(|context| !context.is_empty()) {
            Some(context) => StatusContext::parse(context)?,
            None => StatusContext::default_context(),
        };
        let description = description.map(str::trim).filter(|text| !text.is_empty());
        if let Some(text) = description {
            if text.chars().count() > MAX_DESCRIPTION_CHARS {
                return Err(StatusError::DescriptionTooLong);
            }
            if text.chars().any(char::is_control) {
                return Err(StatusError::DescriptionCharacters);
            }
        }
        let target_url = match target_url.map(str::trim).filter(|url| !url.is_empty()) {
            Some(url) => Some(WebhookUrl::parse(url).map_err(|error| StatusError::TargetUrl(error.to_string()))?.to_string()),
            None => None,
        };
        Ok(Self { state, context, description: description.map(str::to_owned), target_url })
    }
}

/// One stored report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitStatus {
    pub id: StatusId,
    pub repo_id: RepoId,
    /// The full commit id.
    pub sha: String,
    pub state: StatusState,
    pub context: StatusContext,
    pub description: Option<String>,
    pub target_url: Option<String>,
    /// `None` once the account is deleted.
    pub creator: Option<AccountId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ------------------------------------------------------------------ decisions

/// GitHub's combined state of a commit from the **latest** status of each context.
pub fn combined_state(latest: &[CommitStatus]) -> StatusState {
    if latest.iter().any(|status| status.state.is_failed()) {
        StatusState::Failure
    } else if latest.is_empty() || latest.iter().any(|status| status.state == StatusState::Pending) {
        StatusState::Pending
    } else {
        StatusState::Success
    }
}

/// The merge box's summary of a commit's checks: [`ChecksStatus::NotConfigured`] when
/// nothing reported, otherwise the combined state.
pub fn checks_status(latest: &[CommitStatus]) -> ChecksStatus {
    if latest.is_empty() {
        return ChecksStatus::NotConfigured;
    }
    match combined_state(latest) {
        StatusState::Pending => ChecksStatus::Pending,
        StatusState::Success => ChecksStatus::Passing,
        StatusState::Failure | StatusState::Error => ChecksStatus::Failing,
    }
}

/// How the required contexts of a branch stand on a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequiredChecks {
    /// Required contexts.
    pub required: usize,
    /// Required contexts whose latest status is `success`.
    pub passing: usize,
    /// Required contexts whose latest status is `failure` or `error`.
    pub failing: usize,
}

impl RequiredChecks {
    /// Every required context passes.
    pub const fn all_pass(self) -> bool {
        self.passing == self.required
    }

    /// Required contexts that are pending or have not reported yet.
    pub const fn waiting(self) -> usize {
        self.required - self.passing - self.failing
    }
}

/// Counts `required` contexts by the state of their latest status in `latest`.
/// Duplicated contexts count once.
pub fn required_checks<S: AsRef<str>>(required: &[S], latest: &[CommitStatus]) -> RequiredChecks {
    let mut contexts: Vec<&str> = required.iter().map(AsRef::as_ref).collect();
    contexts.sort_unstable();
    contexts.dedup();
    let mut checks = RequiredChecks { required: contexts.len(), ..RequiredChecks::default() };
    for context in contexts {
        let newest = latest.iter().filter(|status| status.context.as_str() == context).max_by_key(|status| status.id);
        match newest.map(|status| status.state) {
            Some(StatusState::Success) => checks.passing += 1,
            Some(StatusState::Failure | StatusState::Error) => checks.failing += 1,
            Some(StatusState::Pending) | None => {}
        }
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(id: i64, context: &str, state: StatusState) -> CommitStatus {
        CommitStatus {
            id: StatusId(id),
            repo_id: RepoId(1),
            sha: "a".repeat(40),
            state,
            context: StatusContext::parse(context).unwrap(),
            description: None,
            target_url: None,
            creator: None,
            created_at: Timestamp(id),
            updated_at: Timestamp(id),
        }
    }

    #[test]
    fn reports_are_validated() {
        let report = StatusReport::parse("success", None, Some("  "), Some("")).unwrap();
        assert_eq!(report.context.as_str(), DEFAULT_CONTEXT);
        assert_eq!((report.description, report.target_url), (None, None));

        let report =
            StatusReport::parse(" failure ", Some(" ci/build "), Some(" 3 tests failed "), Some("https://ci.example/b/1")).unwrap();
        assert_eq!(report.state, StatusState::Failure);
        assert_eq!(report.context.as_str(), "ci/build");
        assert_eq!(report.description.as_deref(), Some("3 tests failed"));
        assert_eq!(report.target_url.as_deref(), Some("https://ci.example/b/1"));

        let long = "x".repeat(MAX_DESCRIPTION_CHARS);
        assert!(StatusReport::parse("pending", None, Some(&long), None).is_ok(), "exactly the limit");
        let cases = [
            (StatusReport::parse("passed", None, None, None), StatusError::State),
            (StatusReport::parse("SUCCESS", None, None, None), StatusError::State),
            (StatusReport::parse("success", None, Some(&format!("{long}y")), None), StatusError::DescriptionTooLong),
            (StatusReport::parse("success", None, Some("a\nb"), None), StatusError::DescriptionCharacters),
            (StatusReport::parse("success", Some(&"c".repeat(256)), None, None), StatusError::ContextTooLong),
            (StatusReport::parse("success", Some("a\tb"), None, None), StatusError::ContextCharacters),
        ];
        for (result, error) in cases {
            assert_eq!(result.unwrap_err(), error);
        }
        for url in ["javascript:alert(1)", "ftp://ci.example/x", "/relative", "https://user:pw@ci.example/x", "https://ci.example/a b"] {
            assert!(matches!(StatusReport::parse("success", None, None, Some(url)), Err(StatusError::TargetUrl(_))), "{url}");
        }
        assert_eq!(StatusContext::parse(" "), Err(StatusError::EmptyContext));
        assert_eq!(StatusContext::parse(&"é".repeat(255)).map(|c| c.as_str().chars().count()), Ok(255), "characters, not bytes");
    }

    #[test]
    fn shas_are_full_commit_ids() {
        let sha1 = "A".repeat(40);
        assert_eq!(parse_sha(&sha1), Ok("a".repeat(40)));
        assert_eq!(parse_sha(&"b".repeat(64)), Ok("b".repeat(64)));
        for bad in ["", "abc1234", &"g".repeat(40), &"a".repeat(41), "main"] {
            assert_eq!(parse_sha(bad), Err(StatusError::Sha), "{bad:?}");
        }
        assert_eq!(StatusState::parse("error"), Some(StatusState::Error));
        assert!(StatusState::Error.is_failed() && !StatusState::Pending.is_failed());
    }

    #[test]
    fn the_combined_state_follows_github() {
        use StatusState::{Error, Failure, Pending, Success};
        assert_eq!(combined_state(&[]), Pending, "nothing reported");
        assert_eq!(combined_state(&[status(1, "a", Success), status(2, "b", Success)]), Success);
        assert_eq!(combined_state(&[status(1, "a", Success), status(2, "b", Pending)]), Pending);
        assert_eq!(combined_state(&[status(1, "a", Pending), status(2, "b", Error)]), Failure, "error counts as failure");
        assert_eq!(combined_state(&[status(1, "a", Failure), status(2, "b", Success)]), Failure);

        assert_eq!(checks_status(&[]), ChecksStatus::NotConfigured);
        assert_eq!(checks_status(&[status(1, "a", Pending)]), ChecksStatus::Pending);
        assert_eq!(checks_status(&[status(1, "a", Success)]), ChecksStatus::Passing);
        assert_eq!(checks_status(&[status(1, "a", Error)]), ChecksStatus::Failing);
    }

    #[test]
    fn required_checks_count_the_latest_status_of_each_context() {
        use StatusState::{Error, Failure, Pending, Success};
        let latest = [status(1, "build", Success), status(2, "lint", Failure), status(3, "deploy", Pending), status(4, "docs", Error)];
        let checks = required_checks(&["build", "lint", "deploy", "missing", "build"], &latest);
        assert_eq!(checks, RequiredChecks { required: 4, passing: 1, failing: 1 });
        assert_eq!(checks.waiting(), 2, "deploy is pending, missing never reported");
        assert!(!checks.all_pass());

        // The newest report of a context wins, even if a caller passes the history.
        let history = [status(1, "build", Failure), status(5, "build", Success)];
        assert!(required_checks(&["build"], &history).all_pass());
        assert!(required_checks::<&str>(&[], &[]).all_pass(), "nothing required");
        assert_eq!(required_checks(&["Build"], &history).passing, 0, "contexts are case-sensitive");
    }
}
