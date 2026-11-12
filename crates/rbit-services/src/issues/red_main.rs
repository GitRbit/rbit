//! Red `main` (agent plan step 4.7): one issue per failing workflow on the default branch.
//!
//! A push run of a workflow on the default branch that fails opens an issue titled
//! "CI is failing on `<branch>`: `<workflow file>`" with the run, its commit and its failed jobs,
//! by whoever triggered the run, unless one is open already; then the failure is added to it
//! as a comment. The next successful run of that workflow there closes the issue as
//! completed, saying which commit is green. The workflow is named by its file, which stays
//! the same while its `name:` may not: a workflow that cannot be read has only its file, so a
//! title by name would never match the run that is green again. Nothing is ever reverted on
//! its own: the issue names the commands that find the culprit and propose its revert.

use std::sync::Arc;

use rbit_core::{
    IssueKind, IssueState, RepoId, StateReason,
    actions::{RunEvent, RunId, Status},
};

use super::{IssueService, SearchFilter};
use crate::{RepositoryService, ServiceError, issues::NewIssueInput, timeline::StateChange};

/// Failed jobs named in an issue or comment.
const NAMED_JOBS: usize = 10;

impl IssueService {
    /// Files, updates or closes the red-main issue of run `run` of `repo`, if it was a push
    /// run on the default branch that finished as a success or a failure.
    pub(crate) async fn red_main(&self, repo: RepoId, run: RunId) -> Result<(), ServiceError> {
        let database = &self.ctx.database;
        let Some(run) = database.actions_runs().run(run).await? else { return Ok(()) };
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(()) };
        let branch = owned.repo.default_branch.clone();
        let (failed, passed) = (run.status == Status::Failure, run.status == Status::Success);
        if run.event != RunEvent::Push || run.reference != format!("refs/heads/{branch}") || !(failed || passed) {
            return Ok(());
        }
        let Some(actor) = run.trigger_actor.or(run.actor) else { return Ok(()) };
        let Some(user) = database.accounts().find_user(actor).await? else { return Ok(()) };
        let repositories = RepositoryService::new(
            Arc::clone(&self.ctx.config),
            database.repositories(),
            self.ctx.repositories.clone(),
            self.ctx.events.clone(),
        );
        let ctx = repositories.open(Some(&user), owned.owner.name.as_str(), owned.repo.name.as_str()).await?;
        let title = format!("CI is failing on `{branch}`: `{}`", run.workflow_path);
        let mut filter = SearchFilter::new(repo);
        filter.state = Some(IssueState::Open);
        filter.text = vec!["failing".to_owned()];
        let open = self.list(&ctx, filter, 1, 50).await?;
        let existing =
            open.issues.iter().find(|issue| issue.kind == IssueKind::Issue && issue.title.as_str() == title).map(|issue| issue.number);
        let base = self.ctx.config.server.base_url();
        let link = format!("{base}/{}/actions/runs/{}", owned.full_name(), run.number);
        let sha = &run.sha;
        if passed {
            let Some(number) = existing else { return Ok(()) };
            let thread = self.timeline.open(Some(&user), ctx, number).await?;
            self.timeline.comment(&user, &thread, &format!("Green again: run [#{}]({link}) passed at {sha}.", run.number)).await?;
            self.timeline.set_state(&user, &thread, StateChange::Close(StateReason::Completed)).await?;
            tracing::info!(repo = %owned.full_name(), number = number.get(), "red main issue closed");
            return Ok(());
        }
        let failed_jobs: Vec<String> = database
            .actions_runs()
            .jobs(run.id)
            .await?
            .into_iter()
            .filter(|job| job.status == Status::Failure)
            .take(NAMED_JOBS)
            .map(|job| format!("- `{}`", job.name))
            .collect();
        let report = format!(
            "Run [#{}]({link}) of `{}` failed on `{branch}` at {sha}.\n\nFailed jobs:\n{}\n\nWhat failed, in short: \
             `rbit run failure {}`. Which commit broke it: `rbit bisect <last green commit> {sha}`. A revert is proposed \
             as a pull request with `rbit pr revert <commit>`; nothing is reverted on its own.",
            run.number,
            run.workflow_name,
            if failed_jobs.is_empty() { "- (none recorded)".to_owned() } else { failed_jobs.join("\n") },
            run.id.0,
        );
        if let Some(number) = existing {
            let thread = self.timeline.open(Some(&user), ctx, number).await?;
            self.timeline.comment(&user, &thread, &format!("Still failing.\n\n{report}")).await?;
        } else {
            let input = NewIssueInput { title, body: report, labels: Vec::new(), assignees: Vec::new(), milestone: None };
            let issue = self.create(&user, &ctx, input).await?;
            tracing::info!(repo = %owned.full_name(), number = issue.number.get(), "red main issue opened");
        }
        Ok(())
    }
}
