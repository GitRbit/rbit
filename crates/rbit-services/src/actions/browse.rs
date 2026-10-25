//! What the Actions pages read: a repository's runs (filtered, newest first, paged by
//! keyset), one run with its jobs, and the workflows that have runs, with the people they
//! name.
//!
//! Every page needs a reader of a repository with Actions on; with Actions off for the
//! repository the pages answer 404 like any unknown page, so a run's existence is never
//! shown where the tab is not.
//!
//! Workstream U (interface and secrets) owns this file.

use std::collections::BTreeMap;

use rbit_core::{
    Account, AccountId, RepoId,
    actions::{Job, Run, RunEvent, RunId, Status},
};
use rbit_storage::sqlite::actions_runs::RunFilter;

use super::ActionsService;
use crate::{RepoContext, ServiceError};

/// Runs on one page of the list.
pub const RUNS_PER_PAGE: u32 = 25;

/// The filters of the runs list, as GET parameters give them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunQuery {
    /// A workflow path (`.rbit/workflows/ci.yml`).
    pub workflow: Option<String>,
    /// A branch name, without `refs/heads/`.
    pub branch: Option<String>,
    pub event: Option<RunEvent>,
    pub status: Option<Status>,
    /// The name of the account that caused the run.
    pub actor: Option<String>,
    /// Runs older than this one (the next page).
    pub before: Option<RunId>,
}

/// One page of runs.
#[derive(Debug, Clone)]
pub struct RunListing {
    pub runs: Vec<Run>,
    /// The accounts the runs name as actors.
    pub people: BTreeMap<AccountId, Account>,
    /// Where the next page starts, when there is one.
    pub next: Option<RunId>,
}

/// One run with its jobs.
#[derive(Debug, Clone)]
pub struct RunDetail {
    pub run: Run,
    /// In creation order: the workflow's order, matrix combinations together.
    pub jobs: Vec<Job>,
    /// The accounts the run names (actor, trigger actor, approver).
    pub people: BTreeMap<AccountId, Account>,
}

impl ActionsService {
    /// Whether `repo` has Actions on; the tab and every page depend on it.
    pub async fn enabled_for(&self, repo: RepoId) -> Result<bool, ServiceError> {
        Ok(self.ctx().database.actions_settings().is_enabled(repo).await?)
    }

    /// `NotFound` unless the viewer may read `repo` and it has Actions on.
    pub(crate) async fn require_pages(&self, repo: &RepoContext) -> Result<(), ServiceError> {
        if repo.access.can_read() && self.enabled_for(repo.repo.repo.id).await? { Ok(()) } else { Err(ServiceError::NotFound) }
    }

    /// Every workflow of `repo` with runs: its path and the name of its newest run.
    pub async fn workflows(&self, repo: &RepoContext) -> Result<Vec<(String, String)>, ServiceError> {
        self.require_pages(repo).await?;
        Ok(self.ctx().database.actions_runs().workflows(repo.repo.repo.id).await?)
    }

    /// One page of `repo`'s runs matching `query`, newest first.
    pub async fn list_runs(&self, repo: &RepoContext, query: &RunQuery) -> Result<RunListing, ServiceError> {
        self.require_pages(repo).await?;
        let database = &self.ctx().database;
        let store = database.actions_runs();
        let repo_id = repo.repo.repo.id;
        let actor = match &query.actor {
            Some(name) => match database.accounts().find_account_by_name(name).await? {
                Some(account) => Some(account.id),
                // Nobody by that name caused a run.
                None => return Ok(RunListing { runs: Vec::new(), people: BTreeMap::new(), next: None }),
            },
            None => None,
        };
        let reference = query.branch.as_ref().map(|branch| format!("refs/heads/{branch}"));
        let filter = RunFilter {
            workflow_path: query.workflow.as_deref(),
            event: query.event,
            status: query.status,
            actor,
            reference: reference.as_deref(),
            before: query.before,
            limit: RUNS_PER_PAGE + 1,
        };
        let mut runs = store.list_runs(repo_id, &filter).await?;
        let next = if runs.len() > RUNS_PER_PAGE as usize {
            runs.truncate(RUNS_PER_PAGE as usize);
            runs.last().map(|run| run.id)
        } else {
            None
        };
        let people = self.people(runs.iter().flat_map(|run| [run.actor, run.trigger_actor])).await?;
        Ok(RunListing { runs, people, next })
    }

    /// Run `#number` of `repo` with its jobs.
    pub async fn run_detail(&self, repo: &RepoContext, number: i64) -> Result<RunDetail, ServiceError> {
        self.require_pages(repo).await?;
        let store = self.ctx().database.actions_runs();
        let run = store.find_run_by_number(repo.repo.repo.id, number).await?.ok_or(ServiceError::NotFound)?;
        let jobs = store.jobs(run.id).await?;
        let people = self.people([run.actor, run.trigger_actor, run.approved_by]).await?;
        Ok(RunDetail { run, jobs, people })
    }

    /// The accounts among `ids`, by id.
    pub(super) async fn people(
        &self,
        ids: impl IntoIterator<Item = Option<AccountId>>,
    ) -> Result<BTreeMap<AccountId, Account>, ServiceError> {
        let mut ids: Vec<AccountId> = ids.into_iter().flatten().collect();
        ids.sort_unstable();
        ids.dedup();
        let accounts = self.ctx().database.accounts().find_accounts(&ids).await?;
        Ok(accounts.into_iter().map(|account| (account.id, account)).collect())
    }
}
