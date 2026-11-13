//! Bisection from recorded results (agent plan step 4.9): before testing anything, read
//! what CI already said about each commit between a good and a bad one.
//!
//! The range is `bad`'s first-parent line down to `good` (at most
//! [`rbit_git::ancestry::MAX_RANGE_COMMITS`] commits). A commit *passed* when it has a
//! successful finished run and no failed one (of `workflow` when given), *failed* when it
//! has a failed run, and is *untested* otherwise. Walking from `good` up, the last commit
//! that passed before the first that failed is `last_good`, that failure `first_bad`; the
//! untested commits between them are what is left to test, `next` the middle one. With no
//! untested commit left, `first_bad` is the commit that broke it.
//!
//! `good` must be on `bad`'s first-parent line (reversed bounds, a commit merged in from a
//! side branch and an unrelated one are refused as invalid). A `good` whose recorded runs
//! failed, or a `bad` whose recorded runs passed, contradicts the bounds: the answer then
//! carries `warnings`.

use rbit_core::actions::Status;

use crate::{ActionsService, RepoContext, ServiceError};

/// What CI recorded for one commit of a bisection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    Passed,
    Failed,
    Untested,
}

impl Recorded {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Untested => "untested",
        }
    }
}

/// A bisection's answer.
#[derive(Debug, Clone)]
pub struct Bisection {
    /// Oldest first, from just above `good` to `bad`.
    pub commits: Vec<(String, Recorded)>,
    pub last_good: String,
    pub first_bad: String,
    /// The untested commits between them, oldest first.
    pub remaining: Vec<String>,
    /// The one to test next; `None` when `first_bad` is the culprit.
    pub next: Option<String>,
    /// The range was longer than the walk reads.
    pub truncated: bool,
    /// Where CI's records contradict the bounds given.
    pub warnings: Vec<String>,
}

/// The narrowing of `commits` (oldest first) between `good` and `bad`.
#[must_use]
pub fn narrow(good: &str, bad: &str, commits: Vec<(String, Recorded)>, truncated: bool) -> Bisection {
    let first_fail = commits.iter().position(|(_, recorded)| *recorded == Recorded::Failed);
    let upto = first_fail.unwrap_or(commits.len().saturating_sub(1));
    let last_pass = commits[..upto.min(commits.len())].iter().rposition(|(_, recorded)| *recorded == Recorded::Passed);
    let last_good = last_pass.map_or_else(|| good.to_owned(), |index| commits[index].0.clone());
    let first_bad = first_fail.map_or_else(|| bad.to_owned(), |index| commits[index].0.clone());
    let start = last_pass.map_or(0, |index| index + 1);
    let end = first_fail.unwrap_or(commits.len().saturating_sub(1));
    let remaining: Vec<String> = commits
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .filter(|(_, recorded)| *recorded == Recorded::Untested)
        .map(|(sha, _)| sha.clone())
        .collect();
    let next = remaining.get(remaining.len() / 2).cloned();
    Bisection { commits, last_good, first_bad, remaining, next, truncated, warnings: Vec::new() }
}

/// What a recorded state says against the bounds: a `good` that failed, a `bad` that passed.
#[must_use]
pub fn contradictions(good: (&str, Recorded), bad: (&str, Recorded)) -> Vec<String> {
    let mut warnings = Vec::new();
    if good.1 == Recorded::Failed {
        warnings.push(format!("good {} has a failed run recorded: the bisection assumes it passed", short(good.0)));
    }
    if bad.1 == Recorded::Passed {
        warnings.push(format!("bad {} has only passing runs recorded: the bisection assumes it failed", short(bad.0)));
    }
    warnings
}

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

/// A commit's state from the statuses of its finished runs.
fn recorded_state(statuses: &[Status]) -> Recorded {
    if statuses.contains(&Status::Failure) {
        Recorded::Failed
    } else if statuses.contains(&Status::Success) {
        Recorded::Passed
    } else {
        Recorded::Untested
    }
}

impl ActionsService {
    /// The bisection of `repo` between `good` and `bad` (branches, tags or commit ids) from
    /// the results of workflow `workflow` (every workflow when `None`).
    pub async fn bisect(&self, repo: &RepoContext, good: &str, bad: &str, workflow: Option<&str>) -> Result<Bisection, ServiceError> {
        self.require_pages(repo).await?;
        let good_sha = repo.reader.commit_id(good.to_owned()).await?;
        let bad_sha = repo.reader.commit_id(bad.to_owned()).await?;
        if good_sha == bad_sha {
            return Err(ServiceError::Invalid("good and bad are the same commit: there is nothing between them".to_owned()));
        }
        if !repo.reader.is_ancestor(good_sha.clone(), bad_sha.clone()).await? {
            return Err(ServiceError::Invalid(format!(
                "good {} is not an ancestor of bad {}: give the older, passing commit first (GOOD, then BAD)",
                short(&good_sha),
                short(&bad_sha)
            )));
        }
        let range = repo.reader.first_parent_range(good_sha.clone(), bad_sha.clone()).await?;
        if !range.reaches_good {
            return Err(ServiceError::Invalid(format!(
                "good {} is not on the first-parent line of bad {} (it was merged in from another branch): give a commit of that line",
                short(&good_sha),
                short(&bad_sha)
            )));
        }
        let mut oldest_first = range.commits;
        oldest_first.reverse();
        let mut asked = oldest_first.clone();
        asked.push(good_sha.clone());
        let recorded = self.ctx().database.actions_runs().finished_runs_of(repo.repo.repo.id, &asked, workflow).await?;
        let state = |sha: &str| recorded_state(recorded.get(sha).map(Vec::as_slice).unwrap_or_default());
        let warnings = contradictions((good_sha.as_str(), state(&good_sha)), (bad_sha.as_str(), state(&bad_sha)));
        let commits = oldest_first
            .into_iter()
            .map(|sha| {
                let recorded = state(&sha);
                (sha, recorded)
            })
            .collect();
        Ok(Bisection { warnings, ..narrow(&good_sha, &bad_sha, commits, range.truncated) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Recorded::{Failed, Passed, Untested};

    fn line(states: &[Recorded]) -> Vec<(String, Recorded)> {
        states.iter().enumerate().map(|(index, state)| (format!("c{index}"), *state)).collect()
    }

    #[test]
    fn recorded_results_narrow_the_range_before_anything_runs() {
        let found = narrow("g", "b", line(&[Passed, Untested, Untested, Untested, Failed, Untested, Failed]), false);
        assert_eq!((found.last_good.as_str(), found.first_bad.as_str()), ("c0", "c4"));
        assert_eq!(found.remaining, ["c1", "c2", "c3"]);
        assert_eq!(found.next.as_deref(), Some("c2"));

        let culprit = narrow("g", "b", line(&[Passed, Passed, Failed, Failed]), false);
        assert_eq!((culprit.first_bad.as_str(), culprit.next), ("c2", None), "nothing left to test: c2 broke it");

        let unknown = narrow("g", "b", line(&[Untested, Untested, Untested]), false);
        assert_eq!((unknown.last_good.as_str(), unknown.first_bad.as_str()), ("g", "b"));
        assert_eq!(unknown.remaining, ["c0", "c1"], "the bad end itself is known bad");
        assert!(narrow("g", "b", Vec::new(), false).next.is_none());
    }

    #[test]
    fn records_that_contradict_the_bounds_are_warned_about() {
        let (good, bad) = ("a".repeat(40), "b".repeat(40));
        assert_eq!(contradictions((good.as_str(), Passed), (bad.as_str(), Failed)), Vec::<String>::new());
        assert_eq!(contradictions((good.as_str(), Untested), (bad.as_str(), Untested)), Vec::<String>::new());
        let both = contradictions((good.as_str(), Failed), (bad.as_str(), Passed));
        assert_eq!(both.len(), 2, "{both:?}");
        assert!(both[0].starts_with("good aaaaaaaaaaaa has a failed run"), "{both:?}");
        assert!(both[1].starts_with("bad bbbbbbbbbbbb has only passing"), "{both:?}");
        assert_eq!(recorded_state(&[Status::Success, Status::Failure]), Failed);
        assert_eq!(recorded_state(&[Status::Cancelled]), Untested);
    }
}
