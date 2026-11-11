//! Runs of scratch refs (agent plan steps 3.1 and 3.3): a commit on no branch, pushed to
//! `refs/scratch/<owner>/<name>` (as `rbit ci run` does), runs the workflows a branch of
//! that name would, is recorded so the sweep can expire it, and may ask for a subset of
//! jobs with an `Rbit-CI-Jobs: lint, tests` trailer in its own message.
//!
//! A subset is honoured on scratch refs only, so the commit that carries it is never a
//! branch head whose required checks a partial run could satisfy. The subset keeps the
//! named jobs and every job they need; a workflow with none of them starts no run.

use std::collections::BTreeSet;

use rbit_core::Timestamp;
use rbit_git::{CommitSummary, RefUpdate};

use super::runs::Candidate;
use crate::{ActionsService, ServiceError};

/// The trailer that names the jobs of a scratch run.
pub const JOBS_TRAILER: &str = "Rbit-CI-Jobs:";
/// How long a scratch ref lives before the sweep deletes it.
pub const SCRATCH_LIFETIME_SECONDS: i64 = 24 * 60 * 60;

impl ActionsService {
    /// The candidates pushed ref `update` (head commit `head`) runs: all of them, or for a
    /// scratch ref the subset its trailer asks for. The push check records scratch refs.
    pub(crate) fn scratch_run(update: &RefUpdate, head: &CommitSummary, candidates: Vec<Candidate>) -> Vec<Candidate> {
        match requested_jobs(head).filter(|_| update.name.starts_with(rbit_git::scratch::PREFIX)) {
            Some(jobs) => candidates.into_iter().filter_map(|candidate| restrict(candidate, &jobs)).collect(),
            None => candidates,
        }
    }

    /// Deletes scratch refs past their lifetime, from Git and from the record; how many.
    pub(crate) async fn expire_scratch(&self, now: Timestamp) -> Result<usize, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let expired = store.expired_scratch(Timestamp(now.0 - SCRATCH_LIFETIME_SECONDS), 100).await?;
        let mut deleted = 0;
        for scratch in expired {
            if let Some(owned) = self.ctx().database.admin().find_repository(scratch.repo).await? {
                let reader = self.reader(&owned);
                match reader.delete_scratch(&self.ctx().config.repository.git_binary, &scratch.name, &scratch.sha).await {
                    Ok(true) => deleted += 1,
                    Ok(false) => {}
                    Err(error) => tracing::warn!(%error, r#ref = %scratch.name, "cannot delete an expired scratch ref"),
                }
            }
            store.forget_scratch(scratch.repo, &scratch.name, &scratch.sha).await?;
        }
        Ok(deleted)
    }
}

/// The jobs an `Rbit-CI-Jobs:` trailer of `head`'s message names, if it has one.
fn requested_jobs(head: &CommitSummary) -> Option<BTreeSet<String>> {
    let body = head.body.as_deref()?;
    let line = body.lines().rev().find_map(|line| line.trim().strip_prefix(JOBS_TRAILER))?;
    let jobs: BTreeSet<String> = line.split(',').map(str::trim).filter(|job| !job.is_empty()).map(str::to_owned).collect();
    (!jobs.is_empty()).then_some(jobs)
}

/// `candidate` with only the `wanted` jobs and every job they need, by job key; `None` when
/// it has none of them. A workflow that failed to plan stays as it is: its error shows.
fn restrict(mut candidate: Candidate, wanted: &BTreeSet<String>) -> Option<Candidate> {
    let Ok(plan) = &mut candidate.outcome else { return Some(candidate) };
    let mut keep: BTreeSet<String> = plan.jobs.iter().filter(|job| wanted.contains(&job.job_id)).map(|job| job.job_id.clone()).collect();
    if keep.is_empty() {
        return None;
    }
    loop {
        let needed: Vec<String> = plan
            .jobs
            .iter()
            .filter(|job| keep.contains(&job.job_id))
            .flat_map(|job| job.needs.iter().cloned())
            .filter(|need| !keep.contains(need))
            .collect();
        if needed.is_empty() {
            break;
        }
        keep.extend(needed);
    }
    plan.jobs.retain(|job| keep.contains(&job.job_id));
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(body: Option<&str>) -> CommitSummary {
        CommitSummary {
            id: "a".repeat(40),
            short_id: "aaaaaaa".into(),
            summary: "Try it".into(),
            body: body.map(str::to_owned),
            author_name: "Ada".into(),
            author_email: "ada@example.test".into(),
            authored_at: Timestamp(0),
            parent_ids: Vec::new(),
        }
    }

    #[test]
    fn the_trailer_names_the_jobs() {
        assert_eq!(requested_jobs(&head(Some("Details\n\nRbit-CI-Jobs: lint, tests\n"))), Some(BTreeSet::from(["lint".to_owned(), "tests".to_owned()])));
        assert_eq!(requested_jobs(&head(Some("No trailer"))), None);
        assert_eq!(requested_jobs(&head(Some("Rbit-CI-Jobs: ,"))), None);
        assert_eq!(requested_jobs(&head(None)), None);
    }
}
