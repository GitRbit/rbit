//! Superseded runs (agent plan step 2.1): a newer push to a branch cancels that branch's
//! older runs of the same workflow, so ten pushes in a row finish one run, not ten.
//!
//! * A repository turns it on ([`ActionsService::set_supersede`], off by default): a push
//!   to any branch but the default one then puts each workflow's run in an implicit
//!   `concurrency` group with `cancel-in-progress` (`rbit-supersede/<workflow>/<ref>`),
//!   unless the workflow declares its own `concurrency`, which wins. Runs on the default
//!   branch are never cancelled this way, so every commit that lands there is tested.
//! * Every run cancelled because a newer one entered its group (this setting or a
//!   workflow's own `cancel-in-progress`) records the commit that superseded it, and the
//!   tasks it was running end as `superseded`: "superseded by 4c1e9a…", not "cancelled".

use std::collections::BTreeMap;

use rbit_core::{
    RepoId, Timestamp, User,
    actions::{Run, RunEvent, RunId, Status, TaskId},
};
use rbit_storage::sqlite::actions_runs::Settled;

use super::runs::Trigger;
use crate::{ActionsService, RepoContext, ServiceError};

impl ActionsService {
    /// The implicit group of a run `trigger` starts for workflow `path`, when the repository
    /// supersedes branch runs and this is a push to a branch other than the default one.
    pub(crate) async fn supersede_group(&self, trigger: &Trigger<'_>, path: &str) -> Result<Option<(String, bool)>, ServiceError> {
        let branch = trigger.reference.strip_prefix("refs/heads/");
        if trigger.event != RunEvent::Push || branch.is_none_or(|branch| branch == trigger.repo.repo.default_branch) {
            return Ok(None);
        }
        if !self.ctx().database.actions_runs().supersede_enabled(trigger.repo.repo.id).await? {
            return Ok(None);
        }
        Ok(Some((format!("rbit-supersede/{path}/{}", trigger.reference), true)))
    }

    /// Cancels run `run` of `repo` because commit `by` superseded it, recording that.
    pub(crate) async fn cancel_superseded(&self, repo: RepoId, run: RunId, by: Option<&str>) -> Result<Option<Settled>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let running: Vec<TaskId> = store.jobs(run).await?.into_iter().filter(|job| job.status == Status::Running).filter_map(|job| job.task_id).collect();
        let settled = store.cancel_run(repo, run, Timestamp::now()).await?;
        if settled.is_some()
            && let Some(by) = by
        {
            store.record_supersession(run, by, &running, Timestamp::now()).await?;
        }
        Ok(settled)
    }

    /// Whether `repo` supersedes branch runs; anyone who reads the repository may ask.
    pub async fn supersede(&self, repo: &RepoContext) -> Result<bool, ServiceError> {
        self.require_pages(repo).await?;
        Ok(self.ctx().database.actions_runs().supersede_enabled(repo.repo.repo.id).await?)
    }

    /// Turns superseding branch runs on or off for `repo`; its administrators only.
    pub async fn set_supersede(&self, actor: &User, repo: &RepoContext, enabled: bool) -> Result<(), ServiceError> {
        if !repo.access.can_admin() {
            return Err(if repo.access.can_read() { ServiceError::Forbidden } else { ServiceError::NotFound });
        }
        self.ctx().database.actions_runs().set_supersede(repo.repo.repo.id, enabled).await?;
        tracing::info!(repo = %repo.repo.full_name(), by = %actor.name(), enabled, "superseding branch runs changed");
        Ok(())
    }

    /// The commit that superseded each of `runs` that was superseded.
    pub async fn superseded_by(&self, runs: &[Run]) -> Result<BTreeMap<RunId, String>, ServiceError> {
        let ids: Vec<RunId> = runs.iter().filter(|run| run.status == Status::Cancelled).map(|run| run.id).collect();
        Ok(self.ctx().database.actions_runs().superseded_by(&ids).await?)
    }
}
