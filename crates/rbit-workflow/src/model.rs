//! The typed model of a workflow file — the parts rbit itself acts on. Everything else
//! (steps, `env`, `container`, `services`, unknown keys) stays in the raw YAML and goes to
//! the runner as written.
//!
//! A [`Workflow`] only exists once [`crate::parse`] has validated it: job ids are well
//! formed, `needs` name existing jobs and form no cycle, filter patterns compile, and the
//! file is within its [`crate::Limits`].

use crate::{
    cron::Cron,
    error::Location,
    pattern::Patterns,
    yaml::{Mapping, Node},
};

/// A parsed workflow file.
#[derive(Debug, Clone)]
pub struct Workflow {
    /// `name`; runs show the file path when it is absent.
    pub name: Option<String>,
    /// `run-name`, as written (it may contain expressions).
    pub run_name: Option<String>,
    pub on: On,
    /// Top-level `permissions`.
    pub permissions: Option<Permissions>,
    /// Workflow-level `concurrency`.
    pub concurrency: Option<Concurrency>,
    /// The jobs in file order.
    pub jobs: Vec<Job>,
    /// The whole document, for re-emitting single-job payloads.
    pub(crate) document: Mapping,
}

impl Workflow {
    pub fn job(&self, id: &str) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == id)
    }

    /// The whole document as written.
    pub fn document(&self) -> &Mapping {
        &self.document
    }
}

/// The events a workflow runs on (`on:`).
#[derive(Debug, Clone, Default)]
pub struct On {
    pub push: Option<PushFilters>,
    pub pull_request: Option<PullRequestFilters>,
    pub workflow_dispatch: Option<Dispatch>,
    /// `schedule` entries, in file order.
    pub schedule: Vec<Schedule>,
    /// Other events, by name. rbit starts no runs for them.
    pub other: Vec<String>,
}

/// One of the three filters of an event: absent, an inclusion list (`branches`) or an
/// ignore list (`branches-ignore`). GitHub refuses both at once, and so does rbit.
#[derive(Debug, Clone, Default)]
pub enum Filter {
    #[default]
    Absent,
    Include(Patterns),
    Ignore(Patterns),
}

impl Filter {
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// Whether the inputs pass the filter (act's `!Skip` and `!Filter`); an absent filter
    /// passes everything.
    pub fn passes<I, S>(&self, inputs: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        match self {
            Self::Absent => true,
            Self::Include(patterns) => !patterns.skip(inputs),
            Self::Ignore(patterns) => !patterns.filter(inputs),
        }
    }
}

/// `on.push`.
#[derive(Debug, Clone, Default)]
pub struct PushFilters {
    pub branches: Filter,
    pub tags: Filter,
    pub paths: Filter,
}

/// `on.pull_request`.
#[derive(Debug, Clone, Default)]
pub struct PullRequestFilters {
    /// `types`; `None` means GitHub's default (`opened`, `reopened`, `synchronize`).
    pub types: Option<Vec<ActivityType>>,
    /// Matched against the base branch.
    pub branches: Filter,
    pub paths: Filter,
}

/// A pull request activity type. rbit emits the named ones; [`ActivityType::Other`] keeps
/// the GitHub types it never emits (`labeled`, `assigned`, …), which therefore never match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityType {
    Opened,
    Reopened,
    Synchronize,
    Closed,
    Edited,
    ReadyForReview,
    Other(String),
}

impl ActivityType {
    /// The types a `pull_request` trigger without `types` runs on.
    pub const DEFAULT: [Self; 3] = [Self::Opened, Self::Reopened, Self::Synchronize];

    /// Every type GitHub documents for `pull_request`, so a typo is an error.
    const GITHUB_OTHERS: [&'static str; 14] = [
        "assigned",
        "unassigned",
        "labeled",
        "unlabeled",
        "converted_to_draft",
        "locked",
        "unlocked",
        "review_requested",
        "review_request_removed",
        "auto_merge_enabled",
        "auto_merge_disabled",
        "milestoned",
        "demilestoned",
        "enqueued",
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "opened" => Self::Opened,
            "reopened" => Self::Reopened,
            "synchronize" => Self::Synchronize,
            "closed" => Self::Closed,
            "edited" => Self::Edited,
            "ready_for_review" => Self::ReadyForReview,
            other if Self::GITHUB_OTHERS.contains(&other) || other == "dequeued" => Self::Other(other.to_owned()),
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Opened => "opened",
            Self::Reopened => "reopened",
            Self::Synchronize => "synchronize",
            Self::Closed => "closed",
            Self::Edited => "edited",
            Self::ReadyForReview => "ready_for_review",
            Self::Other(other) => other,
        }
    }
}

/// `on.workflow_dispatch`.
#[derive(Debug, Clone, Default)]
pub struct Dispatch {
    pub inputs: Vec<DispatchInput>,
}

/// One `workflow_dispatch` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchInput {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    /// The default as text (`true`, `3`, an option of a choice).
    pub default: Option<String>,
    pub kind: InputKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    String,
    Boolean,
    Number,
    /// One of these options.
    Choice(Vec<String>),
    /// GitHub's deployment environment picker; rbit has no environments and treats it as a string.
    Environment,
}

/// One `schedule` entry: a valid expression at least [`crate::cron::MIN_INTERVAL_MINUTES`]
/// apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub cron: Cron,
    pub at: Location,
}

/// One coarse permission of the workflow token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Permission {
    #[default]
    None,
    Read,
    Write,
}

/// `permissions:`, coarsely: `contents`, `issues` and `pull-requests`. Every other
/// permission is accepted and ignored. A mapping leaves unnamed permissions at `none`, as
/// on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Permissions {
    pub contents: Permission,
    pub issues: Permission,
    pub pull_requests: Permission,
}

impl Permissions {
    pub const fn uniform(level: Permission) -> Self {
        Self { contents: level, issues: level, pull_requests: level }
    }
}

/// Workflow-level `concurrency`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concurrency {
    /// The group, as written (it may contain expressions).
    pub group: String,
    pub cancel_in_progress: bool,
    pub at: Location,
}

/// One job of a workflow.
#[derive(Debug, Clone)]
pub struct Job {
    /// The key under `jobs`: `[A-Za-z_][A-Za-z0-9_-]*`.
    pub id: String,
    /// `name`, as written (it may contain expressions).
    pub name: Option<String>,
    /// The jobs this one waits for; each exists in the workflow.
    pub needs: Vec<String>,
    /// `runs-on` labels, as written (they may contain `${{ matrix.* }}`).
    pub runs_on: Vec<String>,
    pub matrix: Option<Matrix>,
    /// Job-level `permissions`; they replace the workflow's.
    pub permissions: Option<Permissions>,
    pub at: Location,
    /// The job as written, for its payload.
    pub(crate) node: Mapping,
}

impl Job {
    /// The job as written.
    pub fn raw(&self) -> &Mapping {
        &self.node
    }
}

/// `strategy.matrix`.
#[derive(Debug, Clone)]
pub struct Matrix {
    /// Each dimension and its values, in file order.
    pub dimensions: Vec<(String, Vec<Node>)>,
    pub include: Vec<Mapping>,
    pub exclude: Vec<Mapping>,
    pub at: Location,
}
