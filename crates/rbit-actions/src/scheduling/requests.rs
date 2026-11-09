//! What people may ask of runs and deployments, and what they are told (§11.5, §12.2.2):
//! cancelling, running again, approving, and reviewing jobs that wait for an environment.
//! Who may ask is the adapter's access check; the store's transition is the race
//! authority, so "already" answers come from what it returns.

/// Cancelling a run that finished meanwhile.
pub const ALREADY_FINISHED: &str = "the run has already finished";
/// Running again with a job selection the run does not have.
pub const NO_MATCHING_JOB: &str = "no job of the run matches";
/// Approving a run that does not (or no longer) wait for approval.
pub const NOT_AWAITING_APPROVAL: &str = "the run does not wait for approval";
/// Reviewing jobs that stopped waiting for review meanwhile.
pub const NO_LONGER_REVIEWING: &str = "these jobs no longer wait for review";
/// Longest review comment, in characters.
pub const MAX_REVIEW_COMMENT_CHARS: usize = 65_536;

/// Whether a run may run again: it has jobs (it is no workflow error) and has finished.
pub fn check_rerun(is_workflow_error: bool, finished: bool) -> Result<(), &'static str> {
    if is_workflow_error {
        return Err("a workflow error has no jobs to run again");
    }
    if !finished {
        return Err("only a finished run can be run again");
    }
    Ok(())
}

/// Whether a run may be approved: it waits for approval.
pub fn check_approval(awaiting_approval: bool) -> Result<(), &'static str> {
    if awaiting_approval { Ok(()) } else { Err(NOT_AWAITING_APPROVAL) }
}

/// A review comment is at most [`MAX_REVIEW_COMMENT_CHARS`] characters.
pub fn check_review_comment(comment: &str) -> Result<(), &'static str> {
    if comment.chars().count() > MAX_REVIEW_COMMENT_CHARS { Err("the comment is too long") } else { Ok(()) }
}

/// The jobs a review decides: those of each named environment waiting for review, in the
/// order named. `pending` lists the run's environments that have jobs waiting, with their
/// jobs; names compare without case. `may_review` answers for one environment; its refusal
/// is passed through. `Err` names what was asked that cannot be.
pub fn review_selection<'a, Id: Copy, R>(
    names: &[String],
    pending: &'a [(&'a str, Vec<Id>)],
    mut may_review: impl FnMut(&'a str) -> Result<(), R>,
) -> Result<Vec<Id>, ReviewSelection<R>> {
    let mut jobs = Vec::new();
    for name in names {
        let Some((environment, waiting)) = pending.iter().find(|(environment, _)| environment.eq_ignore_ascii_case(name)) else {
            return Err(ReviewSelection::Invalid(format!("no job of this run waits for {name}")));
        };
        may_review(environment).map_err(ReviewSelection::Refused)?;
        jobs.extend(waiting.iter().copied());
    }
    if jobs.is_empty() {
        return Err(ReviewSelection::Invalid("choose an environment to review".to_owned()));
    }
    Ok(jobs)
}

/// Why a review selection was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewSelection<R> {
    /// The request names nothing reviewable; the message says so.
    Invalid(String),
    /// The reviewer may not review one of the environments.
    Refused(R),
}
