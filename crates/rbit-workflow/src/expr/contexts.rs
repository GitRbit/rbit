//! Expression contexts with original evaluation and allocation behavior.
use crate::spec::JobResult;
use serde_json::Value;
use std::collections::BTreeMap;

/// What the status functions see.
///
/// For a job's `if` this is the outcome of its `needs`; for a step's `if`, the outcome of
/// the steps before it ([`Status::Skipped`] does not occur there).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// Everything before succeeded: `success()` is true.
    Success,
    /// Something before failed: `failure()` is true.
    Failure,
    /// The run was cancelled: `cancelled()` is true.
    Cancelled,
    /// A needed job was skipped: `success()`, `failure()` and `cancelled()` are all false,
    /// so the job is skipped too unless its condition says `always()` (or `failure()` when a
    /// job further up failed: see [`Status::with_failed_ancestor`]).
    Skipped,
}

impl Status {
    /// The status a job's `if` sees from the results of its needs: a failure wins, then a
    /// cancellation, then a skip; no needs is success.
    pub fn of_needs(results: impl IntoIterator<Item = JobResult>) -> Self {
        let mut status = Self::Success;
        for result in results {
            status = match (status, result) {
                (_, JobResult::Failure) | (Self::Failure, _) => Self::Failure,
                (_, JobResult::Cancelled) | (Self::Cancelled, _) => Self::Cancelled,
                (_, JobResult::Skipped) | (Self::Skipped, _) => Self::Skipped,
                (Self::Success, JobResult::Success) => Self::Success,
            };
        }
        status
    }

    /// The status a job's `if` sees when, in addition, a job further up its chain failed:
    /// a need skipped because of that failure makes `failure()` true, as on GitHub ("if any
    /// ancestor job fails"); otherwise the needs alone decide.
    #[must_use]
    pub const fn with_failed_ancestor(self, failed: bool) -> Self {
        match self {
            Self::Skipped if failed => Self::Failure,
            other => other,
        }
    }
}

/// What one side supplies to an evaluation.
pub trait Contexts {
    /// The context called `name` (lower-case, one of [`CONTEXTS`]), or `None` when this
    /// side does not supply it here. Reading an unsupplied context is an evaluation error,
    /// never `null`, so the server cannot quietly evaluate what only the runner knows.
    fn context(&self, name: &str) -> Option<&Value>;

    /// The status the status functions read, or `None` where they cannot be used (outside
    /// `if` conditions).
    fn status(&self) -> Option<Status> {
        None
    }

    /// `hashFiles(patterns…)`: the SHA-256 the runner computes over the matching files of
    /// the workspace, `Some(Err(reason))` when it cannot, or `None` where `hashFiles` is not
    /// available (on the server).
    fn hash_files(&self, patterns: &[String]) -> Option<Result<String, String>> {
        let _ = patterns;
        None
    }
}

/// [`Contexts`] over owned values: each context by name, and optionally a [`Status`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextMap {
    contexts: BTreeMap<String, Value>,
    status: Option<Status>,
}

impl ContextMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// With the context `name` (case-insensitive) set to `value`.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.insert(name, value);
        self
    }

    /// With the status the status functions read.
    #[must_use]
    pub fn with_status(&self, status: Status) -> Self {
        Self { status: Some(status), ..self.clone() }
    }

    pub fn insert(&mut self, name: &str, value: impl Into<Value>) {
        self.contexts.insert(name.to_ascii_lowercase(), value.into());
    }

    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.contexts.remove(&name.to_ascii_lowercase())
    }

    pub fn set_status(&mut self, status: Option<Status>) {
        self.status = status;
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.contexts.get(&name.to_ascii_lowercase())
    }
}

impl Contexts for ContextMap {
    fn context(&self, name: &str) -> Option<&Value> {
        self.contexts.get(name)
    }

    fn status(&self) -> Option<Status> {
        self.status
    }
}
