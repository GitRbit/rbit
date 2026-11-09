//! Status records and the values a report may carry. Target URL syntax is shared with
//! webhooks and stays with the report parser in `rbit_core::status`.

use std::fmt;

use rbit_contracts::{AccountId, RepoId, Timestamp};
use serde::{Deserialize, Serialize};

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

// Actions: triggers (T).
/// Who posted a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusSource {
    /// A person, or CI acting with a person's credentials, through the API.
    #[default]
    User,
    /// rbit Actions, for one of its jobs. Such a status has no creator.
    Actions,
}

impl StatusSource {
    /// The stored value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Actions => "actions",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::User, Self::Actions].into_iter().find(|source| source.as_str() == value)
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
    /// `None` once the account is deleted, and for statuses rbit Actions posted.
    pub creator: Option<AccountId>,
    pub source: StatusSource,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
