//! Deployment environment protection rules (§12.2.2 of the Actions design): which refs may
//! deploy, who may review, and the bounds an administrator may set. Storage, the review
//! gate and wait timer in SQL, audit, and the glob engine stay in their adapters.
//!
//! * A job runs only from the refs the branch policy allows: any; branches with a
//!   protection rule; or branches and tags its selected patterns admit, where `main` names
//!   a branch and `refs/tags/v*` a tag ([`full_ref_patterns`], [`DeploymentGate`]).
//!   It is checked when a runner takes the job, so a changed policy applies at once, and
//!   protection rules are read only for a protected policy and a branch.
//! * A reviewer must be listed and able to read the repository; with
//!   `prevent_self_review`, whoever started the run may not approve it ([`may_review`]).
//! * At most [`MAX_REVIEWERS`] reviewers and a [`MAX_WAIT_TIMER_MINUTES`] wait timer.
//!
//! Secrets of an environment are given only to trusted runs (`crate::trust::Grants`).

mod gate;
#[cfg(test)]
mod tests;

pub use gate::{DeploymentGate, RefRules};

/// Most reviewers an environment may have.
pub const MAX_REVIEWERS: usize = 6;
/// The longest wait timer, as on GitHub: 30 days.
pub const MAX_WAIT_TIMER_MINUTES: u32 = 43_200;

/// Which refs may deploy to an environment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BranchPolicy {
    /// Any branch or tag.
    #[default]
    All,
    /// Branches with a branch protection rule.
    Protected,
    /// Branches and tags matching these GitHub filter patterns (`main`, `release/*`).
    Selected(Vec<String>),
}

impl BranchPolicy {
    /// The stored `branch_policy`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Protected => "protected",
            Self::Selected(_) => "selected",
        }
    }
}

/// Why someone may not review an environment's jobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewRefusal {
    /// Not a reviewer, or cannot read the repository.
    NotReviewer,
    /// The environment prevents self-review and they started the run; the message says so.
    SelfReview(String),
}

/// Who asks to review an environment's jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reviewer {
    /// They can read the repository now.
    pub reads: bool,
    /// They are among the environment's reviewers.
    pub listed: bool,
}

/// Whether `reviewer` may review `environment`'s jobs of a run: listed and able to read
/// the repository and, when the environment prevents self-review, not whoever started it.
pub fn may_review(environment: &str, reviewer: Reviewer, prevent_self_review: bool, started_run: bool) -> Result<(), ReviewRefusal> {
    if !reviewer.reads || !reviewer.listed {
        return Err(ReviewRefusal::NotReviewer);
    }
    if prevent_self_review && started_run {
        return Err(ReviewRefusal::SelfReview(format!("{environment} does not let whoever started a run approve it")));
    }
    Ok(())
}

/// At most [`MAX_REVIEWERS`] reviewers.
pub fn check_reviewer_count(count: usize) -> Result<(), String> {
    if count > MAX_REVIEWERS { Err(format!("an environment has at most {MAX_REVIEWERS} reviewers")) } else { Ok(()) }
}

/// At most [`MAX_WAIT_TIMER_MINUTES`].
pub fn check_wait_timer(minutes: u32) -> Result<(), String> {
    if minutes > MAX_WAIT_TIMER_MINUTES { Err(format!("the wait timer is at most {MAX_WAIT_TIMER_MINUTES} minutes")) } else { Ok(()) }
}

/// An environment's selected patterns as patterns over full refs: `release/*` is a branch
/// pattern (`refs/heads/release/*`), `refs/tags/v*` a tag pattern, so a tag never passes
/// for a branch of the same name or the other way round. A `!` pattern stays an exclusion;
/// the matcher lets the last pattern that matches a ref decide, as in `branches:` filters.
pub fn full_ref_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .map(|raw| {
            let (bang, pattern) = raw.strip_prefix('!').map_or(("", raw.as_str()), |rest| ("!", rest));
            if pattern.starts_with("refs/heads/") || pattern.starts_with("refs/tags/") {
                format!("{bang}{pattern}")
            } else {
                format!("{bang}refs/heads/{pattern}")
            }
        })
        .collect()
}
