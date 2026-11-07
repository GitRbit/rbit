//! Typed workflow events records; expression text remains unevaluated.
use crate::cron::Cron;
use crate::error::Location;
use crate::pattern::Patterns;

// Actions: workflow_run and repository_dispatch.
/// `on.workflow_run`: runs of these workflows (by `name:`) of the same repository start
/// this one.
#[derive(Debug, Clone, Default)]
pub struct WorkflowRunFilters {
    pub workflows: Vec<String>,
    /// `None` means all three.
    pub types: Option<Vec<WorkflowRunType>>,
    /// Matched against the triggering run's branch.
    pub branches: Filter,
}

/// What happened to the triggering run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRunType {
    Requested,
    InProgress,
    Completed,
}

impl WorkflowRunType {
    pub const ALL: [Self; 3] = [Self::Requested, Self::InProgress, Self::Completed];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// `on.repository_dispatch`: the `event_type`s that start it; `None` means any.
#[derive(Debug, Clone, Default)]
pub struct RepositoryDispatchFilters {
    pub types: Option<Vec<String>>,
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
