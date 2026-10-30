//! Workflow lifecycle values shared by event producers and webhook consumers.

use super::Status;

/// A `workflow_run` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunAction {
    Requested,
    InProgress,
    Completed,
}

impl RunAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    /// What a run with `status` shows; `running` says whether a job of it runs.
    pub const fn of(status: Status, running: bool) -> Self {
        if status.is_done() {
            Self::Completed
        } else if running {
            Self::InProgress
        } else {
            Self::Requested
        }
    }
}

/// A `workflow_job` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobAction {
    Waiting,
    Queued,
    InProgress,
    Completed,
}

impl JobAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    /// What a job with `status` shows; `None` while it is blocked on `needs` or a group.
    pub const fn of(status: Status) -> Option<Self> {
        match status {
            Status::AwaitingApproval => Some(Self::Waiting),
            Status::Waiting => Some(Self::Queued),
            Status::Running => Some(Self::InProgress),
            Status::Blocked => None,
            Status::Success | Status::Failure | Status::Cancelled | Status::Skipped => Some(Self::Completed),
        }
    }
}
