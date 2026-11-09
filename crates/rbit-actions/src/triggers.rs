//! Trigger decisions: which pushed refs and pull request events may start runs.
//!
//! Inputs are borrowed projections of the push and pull request records; adapters keep
//! storage, Git reads, workflow evaluation and run creation. Invariants: a push considers
//! at most [`MAX_REF_UPDATES`] live branch or tag updates in push order; a new branch is
//! compared with the default branch; unknown or oversized change lists make `paths`
//! filters pass; only people's pushes synchronize pull request runs; closing a pull
//! request cancels only runs that have not started.

use std::time::Duration;

/// Updated refs of one push that are considered.
pub const MAX_REF_UPDATES: usize = 16;
/// Changed paths listed for `paths` filters; beyond it, `paths` filters pass (as on GitHub).
pub const MAX_CHANGED_PATHS: usize = 3_000;
/// Files a comparison lists: one more than [`MAX_CHANGED_PATHS`] detects an overflow.
pub const LISTED_CHANGED_PATHS: usize = MAX_CHANGED_PATHS + 1;
/// How long one evaluation may take before it gives up.
pub const EVALUATION_BUDGET: Duration = Duration::from_secs(10);
/// Commits listed in a push run's event payload, as in the `push` webhook.
pub const PAYLOAD_COMMITS: usize = 20;

/// One updated ref of a push: its full name and object ids (`after` is `None` when deleted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushedRef<'a> {
    pub name: &'a str,
    pub before: Option<&'a str>,
    pub after: Option<&'a str>,
}

impl<'a> PushedRef<'a> {
    fn branch(self) -> Option<&'a str> {
        self.name.strip_prefix("refs/heads/")
    }

    fn considered(self) -> bool {
        self.after.is_some() && (self.branch().is_some() || self.name.starts_with("refs/tags/"))
    }
}

/// The updates of one push that may start runs: live branches and tags, in push order,
/// at most [`MAX_REF_UPDATES`].
pub fn considered_refs<'u, T>(updates: &'u [T], view: impl Fn(&'u T) -> PushedRef<'u>) -> Vec<&'u T> {
    updates.iter().filter(|update| view(update).considered()).take(MAX_REF_UPDATES).collect()
}

/// The new head of the default branch among `refs`, which refreshes the schedule.
pub fn default_branch_head<'a>(refs: impl IntoIterator<Item = PushedRef<'a>>, default_branch: &str) -> Option<&'a str> {
    refs.into_iter().find(|update| update.branch() == Some(default_branch)).and_then(|update| update.after)
}

/// What `paths` filters compare the pushed commit with: the previous tip, or for a new
/// branch the default branch, which it most likely left. `None` for a new tag or a new
/// default branch.
pub fn comparison_base<'a>(update: PushedRef<'a>, default_branch: &'a str) -> Option<&'a str> {
    match (update.before, update.branch()) {
        (Some(before), _) => Some(before),
        (None, Some(branch)) if branch != default_branch => Some(default_branch),
        (None, _) => None,
    }
}

/// Whether a comparison's file list is complete enough for `paths` filters. Without a
/// merge base (a force push of an unrelated history) the comparison lists nothing although
/// files changed, so the paths are unknown.
pub fn changed_paths_known(listed: usize, has_merge_base: bool) -> bool {
    listed <= MAX_CHANGED_PATHS && has_merge_base
}

/// The full head commit message, where a push's skip marker is looked for.
pub fn head_message(summary: &str, body: Option<&str>) -> String {
    match body {
        Some(body) => format!("{summary}\n\n{body}"),
        None => summary.to_owned(),
    }
}

/// A pull request event, projected from its record. `pusher` is who pushed the new head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullEvent<P> {
    Opened,
    Reopened,
    Edited,
    ReadyForReview,
    Closed,
    Synchronized { starts_runs: bool, pusher: P },
    Other,
}

/// The `on.pull_request` activity an event is matched as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullActivity {
    Opened,
    Reopened,
    Edited,
    ReadyForReview,
    Closed,
    Synchronize,
}

/// The activity of `event` and, for `synchronize`, the pusher who caused it. `None` when
/// the event starts no runs: other actions, and a `synchronize` by a push that may not
/// start runs (a mirror, a task).
pub fn pull_trigger<P>(event: PullEvent<P>) -> Option<(PullActivity, Option<P>)> {
    Some(match event {
        PullEvent::Opened => (PullActivity::Opened, None),
        PullEvent::Reopened => (PullActivity::Reopened, None),
        PullEvent::Edited => (PullActivity::Edited, None),
        PullEvent::ReadyForReview => (PullActivity::ReadyForReview, None),
        PullEvent::Closed => (PullActivity::Closed, None),
        PullEvent::Synchronized { starts_runs: true, pusher } => (PullActivity::Synchronize, Some(pusher)),
        PullEvent::Synchronized { starts_runs: false, .. } | PullEvent::Other => return None,
    })
}

/// Who a pull request run is attributed to: the pusher of a `synchronize`, otherwise the
/// pull request's author.
pub fn pull_cause<A>(pusher: Option<A>, author: A) -> A {
    pusher.unwrap_or(author)
}

/// Where a run stands, projected from its status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStage {
    Waiting,
    Held,
    AwaitingApproval,
    Started,
}

/// Closing a pull request cancels its runs that have not started; running ones finish.
pub fn cancelled_when_pull_closes(stage: RunStage) -> bool {
    matches!(stage, RunStage::Waiting | RunStage::Held | RunStage::AwaitingApproval)
}

#[cfg(test)]
mod tests;
