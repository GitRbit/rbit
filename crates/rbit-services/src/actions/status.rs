//! "Where am I": the state of one branch in one answer (agent plan step 1.6, `rbit status`).
//!
//! For the branch's head on the server: how far it is from its base (merge base, ahead,
//! behind, without diffing), the CI of exactly that commit, the newest CI of the base, the
//! open pull request from the branch, and the issue the branch works on (`feature/<n>-…`).
//!
//! CI states, so "nothing yet" is never confused with "nothing expected": `none_expected`
//! (the commit has no workflow files, or Actions is off), `not_started` (workflows exist
//! but no run has appeared), `queued`, `running`, `failure` (as soon as one job failed,
//! even while others run), `indeterminate` (every failed job's account is incomplete: a
//! lost runner, an unexplained failure, a log cut short), `cancelled` and `success`.

use rbit_core::actions::{Job, Run, Status, TerminationReason};
use rbit_git::divergence::Divergence;

use super::{browse::RunQuery, logs::FailedJob};
use crate::{ActionsService, RepoContext, ServiceError};

/// The CI of one commit.
#[derive(Debug, Clone)]
pub struct CommitCi {
    pub state: &'static str,
    /// Why, for `none_expected` and `not_started`.
    pub reason: Option<&'static str>,
    pub runs: Vec<Run>,
    /// Jobs by state: success, failure, running, queued.
    pub counts: [usize; 4],
    /// The failed jobs, with how they ended (newest run first).
    pub failed: Vec<FailedJob>,
}

/// A branch's open pull request.
#[derive(Debug, Clone)]
pub struct BranchPull {
    pub number: i64,
    pub draft: bool,
    pub base: String,
    pub head_sha: String,
}

/// Everything `rbit status` shows.
#[derive(Debug, Clone)]
pub struct BranchStatus {
    pub branch: String,
    pub head: String,
    pub base: String,
    pub divergence: Option<Divergence>,
    pub ci: CommitCi,
    pub base_sha: Option<String>,
    /// The state of the base's newest run, `None` when it has none.
    pub base_ci: Option<&'static str>,
    pub pull: Option<BranchPull>,
    /// The issue the branch works on, from `feature/<n>-…` (or `<n>-…`).
    pub issue: Option<i64>,
}

impl ActionsService {
    /// The status of `branch` of `repo` (the default branch when `None`).
    pub async fn branch_status(&self, repo: &RepoContext, branch: Option<&str>) -> Result<BranchStatus, ServiceError> {
        let branch = branch.map_or_else(|| repo.repo.repo.default_branch.clone(), str::to_owned);
        let head = repo.reader.commit_id(format!("refs/heads/{branch}")).await?;
        let database = &self.ctx().database;
        let pull = database
            .pulls()
            .open_with_head(repo.repo.repo.id, &branch)
            .await?
            .into_iter()
            .find(|(issue, _)| issue.repo_id == repo.repo.repo.id)
            .map(|(issue, pull)| BranchPull { number: i64::from(issue.number.get()), draft: pull.is_draft, base: pull.base_branch, head_sha: pull.head_sha });
        let base = pull.as_ref().map_or_else(|| repo.repo.repo.default_branch.clone(), |pull| pull.base.clone());
        let divergence = if base == branch { None } else { repo.reader.divergence(format!("refs/heads/{base}"), head.clone()).await.ok() };
        let ci = self.commit_ci(repo, &head).await?;
        let base_sha = repo.reader.commit_id(format!("refs/heads/{base}")).await.ok();
        let base_ci = match &base_sha {
            Some(sha) if *sha != head => self.commit_ci(repo, sha).await.ok().map(|ci| ci.state),
            Some(_) => Some(ci.state),
            None => None,
        };
        Ok(BranchStatus { issue: issue_of(&branch), branch, head, base, divergence, ci, base_sha, base_ci, pull })
    }

    /// The CI of commit `sha` of `repo`.
    pub async fn commit_ci(&self, repo: &RepoContext, sha: &str) -> Result<CommitCi, ServiceError> {
        let query = RunQuery { sha: Some(sha.to_owned()), ..RunQuery::default() };
        let page = self.page_runs(repo, &query, &[], 0, 20).await?;
        if page.runs.is_empty() {
            let enabled = self.require_pages(repo).await.is_ok();
            let none = !enabled || self.workflow_paths_at(repo, sha).await?.is_empty();
            let (state, reason) = if none {
                ("none_expected", if enabled { "the commit has no workflow files" } else { "Actions is off for this repository" })
            } else {
                ("not_started", "workflow files exist but no run has appeared (a `paths` filter may skip this commit)")
            };
            return Ok(CommitCi { state, reason: Some(reason), runs: Vec::new(), counts: [0; 4], failed: Vec::new() });
        }
        let mut jobs: Vec<Job> = Vec::new();
        for run in &page.runs {
            jobs.extend(self.run_by_id(repo, run.id).await?.jobs);
        }
        let mut counts = [0; 4];
        for job in &jobs {
            let slot = match job.status {
                Status::Success | Status::Skipped => 0,
                Status::Failure | Status::Cancelled => 1,
                Status::Running => 2,
                _ => 3,
            };
            counts[slot] += 1;
        }
        let mut failed = Vec::new();
        for run in page.runs.iter().filter(|run| jobs.iter().any(|job| job.run_id == run.id && job.status == Status::Failure)) {
            failed.extend(self.failure_digest(repo, run.id).await?.failed.into_iter().filter(|job| job.job.status == Status::Failure));
        }
        let state = ci_state(&page.runs, &counts, &failed);
        Ok(CommitCi { state, reason: None, runs: page.runs, counts, failed })
    }
}

/// The state of a commit with runs: a failure as soon as one job failed (unless nobody can
/// say why: `indeterminate`), then what is still to come, then how it ended.
fn ci_state(runs: &[Run], counts: &[usize; 4], failed: &[FailedJob]) -> &'static str {
    if !failed.is_empty() {
        let untrusted = |job: &FailedJob| {
            !job.log_complete || matches!(job.termination.reason, TerminationReason::RunnerLost | TerminationReason::Unknown)
        };
        return if failed.iter().all(untrusted) { "indeterminate" } else { "failure" };
    }
    if counts[2] > 0 || runs.iter().any(|run| run.status == Status::Running) {
        return "running";
    }
    if counts[3] > 0 || runs.iter().any(|run| !run.status.is_done()) {
        return "queued";
    }
    if runs.iter().any(|run| run.status == Status::Cancelled) {
        return "cancelled";
    }
    if runs.iter().any(|run| run.status == Status::Failure) { "failure" } else { "success" }
}

/// The issue number a branch name carries: `feature/31-run-watch`, `31-fix`, `fix/31`.
fn issue_of(branch: &str) -> Option<i64> {
    let last = branch.rsplit('/').next()?;
    let digits: String = last.chars().take_while(char::is_ascii_digit).collect();
    let rest = &last[digits.len()..];
    (!digits.is_empty() && (rest.is_empty() || rest.starts_with('-'))).then(|| digits.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_name_carries_its_issue() {
        assert_eq!(issue_of("feature/31-run-watch"), Some(31));
        assert_eq!(issue_of("31-fix"), Some(31));
        assert_eq!(issue_of("fix/7"), Some(7));
        assert_eq!(issue_of("main"), None);
        assert_eq!(issue_of("v2-release"), None);
        assert_eq!(issue_of("feature/2024edition"), None);
    }
}
