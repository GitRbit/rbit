//! Projections of push, pull request and run records into the inputs of
//! [`rbit_actions::triggers`]. They translate types only; the decisions live there.

use rbit_actions::triggers::{PullActivity, PullEvent, PushedRef, RunStage};
use rbit_core::{AccountId, actions::Status, issue::IssueAction};
use rbit_git::RefUpdate;
use rbit_workflow::model::ActivityType;

/// A ref update, borrowed.
pub(super) fn pushed(update: &RefUpdate) -> PushedRef<'_> {
    PushedRef { name: &update.name, before: update.before.as_deref(), after: update.after.as_deref() }
}

/// An issue action on a pull request, with the pusher of a new head.
pub(super) fn pull_event(action: &IssueAction) -> PullEvent<Option<AccountId>> {
    match action {
        IssueAction::Opened => PullEvent::Opened,
        IssueAction::Reopened => PullEvent::Reopened,
        IssueAction::Edited { .. } => PullEvent::Edited,
        IssueAction::ReadyForReview => PullEvent::ReadyForReview,
        IssueAction::Closed { .. } => PullEvent::Closed,
        IssueAction::Synchronized { source, .. } => PullEvent::Synchronized { starts_runs: source.starts_runs(), pusher: source.pusher() },
        _ => PullEvent::Other,
    }
}

/// The workflow model's activity type.
pub(super) const fn activity_type(activity: PullActivity) -> ActivityType {
    match activity {
        PullActivity::Opened => ActivityType::Opened,
        PullActivity::Reopened => ActivityType::Reopened,
        PullActivity::Edited => ActivityType::Edited,
        PullActivity::ReadyForReview => ActivityType::ReadyForReview,
        PullActivity::Closed => ActivityType::Closed,
        PullActivity::Synchronize => ActivityType::Synchronize,
    }
}

/// Where a run stands: held runs are `Blocked`; every other status has started or finished.
pub(super) const fn run_stage(status: Status) -> RunStage {
    match status {
        Status::Waiting => RunStage::Waiting,
        Status::Blocked => RunStage::Held,
        Status::AwaitingApproval => RunStage::AwaitingApproval,
        _ => RunStage::Started,
    }
}
