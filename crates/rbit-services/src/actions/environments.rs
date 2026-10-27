//! Deployment environments (Actions: environments; `docs/design/actions.md` §12.2.2).
//!
//! A job that names an environment with `environment:` runs in it:
//!
//! * It gets the environment's secrets and variables over the repository's and owner's
//!   (trusted runs only, as all secrets).
//! * When the environment has reviewers, the job waits for one of them before any runner
//!   may take it (the storage gate, `actions_runs::settle`), on every attempt. A reviewer
//!   approves or rejects the waiting jobs of a run per environment; with
//!   `prevent_self_review`, not on a run they started. A rejected job fails, and the jobs
//!   that need it are skipped.
//! * It runs only from the refs the branch policy allows (any; branches with a protection
//!   rule; or branches and tags matching GitHub filter patterns), checked when a runner
//!   takes it, so a changed policy applies at once.
//!
//! An environment is created, unprotected, the first time a run names it, as on GitHub.
//! Repository administrators configure and delete environments; reviewers must be able to
//! read the repository. Changes are audited, and jobs left waiting for an environment that
//! no longer has reviewers run without review.

use rbit_core::{
    RepoAccess, Timestamp, User,
    actions::{BranchPolicy, Environment, Job, ReviewDecision, Run, SecretScope},
};
use rbit_storage::sqlite::actions_environments::{EnvironmentChanges, MAX_REVIEWERS};
use rbit_workflow::pattern::Patterns;

use super::ActionsService;
use crate::{
    RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry},
};

/// What an administrator sets on an environment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentInput {
    /// Reviewers' usernames; at most six.
    pub reviewers: Vec<String>,
    pub prevent_self_review: bool,
    pub branch_policy: BranchPolicy,
}

/// The jobs of a run that wait for one environment's reviewers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDeployment {
    pub environment: Environment,
    pub jobs: Vec<Job>,
    /// Whether the viewer may approve or reject them.
    pub can_review: bool,
}

/// The secrets and variables a job's environment gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentValues {
    pub secrets: Vec<(String, String)>,
    pub variables: Vec<(String, String)>,
}

impl ActionsService {
    /// The environments of `repo`, by name, for anyone who may read its Actions.
    pub async fn environments(&self, repo: &RepoContext) -> Result<Vec<Environment>, ServiceError> {
        self.require_pages(repo).await?;
        Ok(self.ctx().database.actions_environments().list(repo.repo.repo.id).await?)
    }

    /// Environment `name` of `repo`.
    pub async fn environment(&self, repo: &RepoContext, name: &str) -> Result<Environment, ServiceError> {
        self.require_pages(repo).await?;
        self.ctx().database.actions_environments().find(repo.repo.repo.id, name).await?.ok_or(ServiceError::NotFound)
    }

    /// The usernames of the user accounts among `ids`, for an API that names reviewers by id;
    /// `Invalid` names the first id that is no user.
    pub async fn usernames(&self, ids: &[rbit_core::AccountId]) -> Result<Vec<String>, ServiceError> {
        let accounts = self.ctx().database.accounts().find_accounts(ids).await?;
        ids.iter()
            .map(|id| {
                accounts
                    .iter()
                    .find(|account| account.id == *id && account.kind == rbit_core::AccountKind::User)
                    .map(|account| account.name.to_string())
                    .ok_or_else(|| ServiceError::Invalid(format!("there is no user {}", id.0)))
            })
            .collect()
    }

    /// The accounts of `environment`'s reviewers, by name.
    pub async fn environment_reviewers(&self, environment: &Environment) -> Result<Vec<rbit_core::Account>, ServiceError> {
        let mut accounts = self.ctx().database.accounts().find_accounts(&environment.reviewers).await?;
        accounts.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        Ok(accounts)
    }

    /// Creates or changes environment `name` of `repo` for `actor`, an administrator
    /// (audited). Jobs waiting for it run at once if it no longer has reviewers.
    pub async fn put_environment(
        &self,
        actor: &User,
        repo: &RepoContext,
        name: &str,
        input: &EnvironmentInput,
    ) -> Result<Environment, ServiceError> {
        require_admin(repo)?;
        repo.ensure_writable()?;
        let name = name.trim();
        if !rbit_workflow::spec::valid_environment_name(name) {
            return Err(ServiceError::Invalid(
                "an environment name is 1 to 255 characters, without `/`, `\\` or control characters".to_owned(),
            ));
        }
        if let BranchPolicy::Selected(patterns) = &input.branch_policy {
            if patterns.is_empty() || patterns.len() > 50 {
                return Err(ServiceError::Invalid("choose 1 to 50 branch or tag patterns".to_owned()));
            }
            let compiled = ref_patterns(patterns).map_err(|error| ServiceError::Invalid(format!("branch pattern: {error}")))?;
            if compiled.iter().all(rbit_workflow::pattern::Pattern::is_negative) {
                return Err(ServiceError::Invalid("at least one branch or tag pattern must not start with `!`".to_owned()));
            }
        }
        if input.reviewers.len() > MAX_REVIEWERS {
            return Err(ServiceError::Invalid(format!("an environment has at most {MAX_REVIEWERS} reviewers")));
        }
        let mut reviewers = Vec::new();
        for username in &input.reviewers {
            let (account, access) = self.access_of(repo, username).await?;
            if !access.can_read() {
                return Err(ServiceError::Invalid(format!(
                    "{} cannot read this repository, so cannot review its deployments",
                    account.name
                )));
            }
            if !reviewers.contains(&account.id) {
                reviewers.push(account.id);
            }
        }
        let changes = EnvironmentChanges {
            reviewers: &reviewers,
            prevent_self_review: input.prevent_self_review,
            branch_policy: &input.branch_policy,
        };
        let environment =
            self.ctx().database.actions_environments().put(repo.repo.repo.id, name, &changes, Timestamp::now()).await.map_err(|error| {
                match error {
                    rbit_storage::StorageError::Conflict(what) => ServiceError::Invalid(what.to_owned()),
                    other => other.into(),
                }
            })?;
        let detail = format!("{} ({} reviewers, branches: {})", environment.name, reviewers.len(), environment.branch_policy.as_str());
        self.audit()
            .record(AuditEntry::new(AuditAction::ActionsEnvironmentChanged).by(actor).on(repo.repo.full_name()).detail(detail))
            .await;
        self.release_reviews(repo).await?;
        Ok(environment)
    }

    /// Deletes environment `name` of `repo` with its secrets and variables, for `actor`, an
    /// administrator (audited). Jobs waiting for it run without review.
    pub async fn delete_environment(&self, actor: &User, repo: &RepoContext, name: &str) -> Result<(), ServiceError> {
        require_admin(repo)?;
        repo.ensure_writable()?;
        if !self.ctx().database.actions_environments().delete(repo.repo.repo.id, name).await? {
            return Err(ServiceError::NotFound);
        }
        let entry = AuditEntry::new(AuditAction::ActionsEnvironmentDeleted).by(actor).on(repo.repo.full_name()).detail(name.to_owned());
        self.audit().record(entry).await;
        self.release_reviews(repo).await
    }

    /// Lets the jobs of `repo` waiting for an environment that no longer has reviewers run.
    async fn release_reviews(&self, repo: &RepoContext) -> Result<(), ServiceError> {
        for settled in self.ctx().database.actions_runs().release_reviews(repo.repo.repo.id, Timestamp::now()).await? {
            self.settled(&settled).await?;
        }
        Ok(())
    }

    /// The jobs of run `run` waiting for review, by environment, and whether `viewer` may
    /// review each environment's.
    pub async fn pending_deployments(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        run: &Run,
    ) -> Result<Vec<PendingDeployment>, ServiceError> {
        self.require_pages(repo).await?;
        let jobs = self.ctx().database.actions_runs().jobs(run.id).await?;
        let store = self.ctx().database.actions_environments();
        let mut pending: Vec<PendingDeployment> = Vec::new();
        for job in jobs.into_iter().filter(|job| job.awaiting_review) {
            let Some(name) = job.environment.clone() else { continue };
            if let Some(existing) = pending.iter_mut().find(|pending| pending.environment.name.eq_ignore_ascii_case(&name)) {
                existing.jobs.push(job);
                continue;
            }
            let Some(environment) = store.find(repo.repo.repo.id, &name).await? else { continue };
            let can_review = viewer.is_some_and(|viewer| may_review(viewer, repo, run, &environment).is_ok());
            pending.push(PendingDeployment { environment, jobs: vec![job], can_review });
        }
        Ok(pending)
    }

    /// Approves or rejects, for `actor`, the jobs of run `run` that wait for the reviewers of
    /// `environments` (audited). Each environment must have `actor` as a reviewer.
    pub async fn review_deployments(
        &self,
        actor: &User,
        repo: &RepoContext,
        run: &Run,
        environments: &[String],
        decision: ReviewDecision,
        comment: &str,
    ) -> Result<(), ServiceError> {
        repo.ensure_writable()?;
        if comment.chars().count() > 65_536 {
            return Err(ServiceError::Invalid("the comment is too long".to_owned()));
        }
        let pending = self.pending_deployments(Some(actor), repo, run).await?;
        let mut jobs = Vec::new();
        for name in environments {
            let Some(deployment) = pending.iter().find(|pending| pending.environment.name.eq_ignore_ascii_case(name)) else {
                return Err(ServiceError::Invalid(format!("no job of this run waits for {name}")));
            };
            may_review(actor, repo, run, &deployment.environment)?;
            jobs.extend(deployment.jobs.iter().map(|job| job.id));
        }
        if jobs.is_empty() {
            return Err(ServiceError::Invalid("choose an environment to review".to_owned()));
        }
        let store = self.ctx().database.actions_runs();
        let Some(settled) = store.review_jobs(run.id, &jobs, actor.id(), decision, comment.trim(), Timestamp::now()).await? else {
            return Err(ServiceError::Invalid("these jobs no longer wait for review".to_owned()));
        };
        self.settled(&settled).await?;
        let action = match decision {
            ReviewDecision::Approved => AuditAction::ActionsDeploymentApproved,
            ReviewDecision::Rejected => AuditAction::ActionsDeploymentRejected,
        };
        let detail = format!("run #{} of {} to {}", run.number, run.workflow_path, environments.join(", "));
        self.audit().record(AuditEntry::new(action).by(actor).on(repo.repo.full_name()).detail(detail)).await;
        Ok(())
    }

    /// For a claimed job that runs in an environment: `Err` with the reason when its branch
    /// policy refuses the run's ref, else its secrets and variables (trusted runs only).
    pub(super) async fn environment_values(&self, run: &Run, name: &str) -> Result<Result<EnvironmentValues, String>, ServiceError> {
        let database = &self.ctx().database;
        let Some(environment) = database.actions_environments().find(run.repo_id, name).await? else {
            return Ok(Ok(EnvironmentValues::default()));
        };
        let (kind, short) = match run.reference.strip_prefix("refs/heads/") {
            Some(branch) => ("branch", branch),
            None => ("tag", run.reference.strip_prefix("refs/tags/").unwrap_or(&run.reference)),
        };
        let allowed = match &environment.branch_policy {
            BranchPolicy::All => true,
            BranchPolicy::Protected => {
                kind == "branch"
                    && database.branch_protection().list(run.repo_id).await?.iter().any(|protection| protection.rule.matches(short))
            }
            BranchPolicy::Selected(patterns) => selected(patterns, &run.reference),
        };
        if !allowed {
            return Ok(Err(format!(
                "{kind} `{short}` is not allowed to deploy to {} due to environment protection rules",
                environment.name
            )));
        }
        if !run.trusted {
            return Ok(Ok(EnvironmentValues::default()));
        }
        let scope = SecretScope::Environment(environment.id);
        let variables = database
            .actions_secrets()
            .variables(scope)
            .await?
            .into_iter()
            .map(|variable| (variable.name.to_string(), variable.value))
            .collect();
        let stored = database.actions_secrets().secrets_for_environment(environment.id).await?;
        let mut secrets = Vec::new();
        if !stored.is_empty() {
            let key = self.secrets_key().await.map_err(|problem| ServiceError::Invalid(format!("secrets are unavailable: {problem}")))?;
            for secret in stored {
                let name = secret.info.name.to_string();
                let Some(value) = key.open(secret.info.scope, secret.info.id, &secret.sealed) else {
                    return Ok(Err(format!("secret {name} of environment {} cannot be opened with the current key", environment.name)));
                };
                secrets.push((name, value.as_str().to_owned()));
            }
        }
        Ok(Ok(EnvironmentValues { secrets, variables }))
    }

    /// The access the user called `username` has to `repo`, and their account.
    async fn access_of(&self, repo: &RepoContext, username: &str) -> Result<(rbit_core::Account, RepoAccess), ServiceError> {
        let database = &self.ctx().database;
        let account = match database.accounts().find_account_by_name(username.trim()).await? {
            Some(account) if account.kind == rbit_core::AccountKind::User => account,
            _ => return Err(ServiceError::Invalid(format!("there is no user called {}", username.trim()))),
        };
        let user = database.accounts().find_user(account.id).await?.ok_or(ServiceError::NotFound)?;
        let grants = crate::repositories::viewer_grants(&database.repositories(), Some(&user), &repo.repo).await?;
        Ok((account, rbit_auth::repo_access(Some(&user), &repo.repo, &grants)))
    }
}

/// Whether `actor` may review `environment`'s jobs of `run`: a reviewer, who did not start
/// the run when the environment prevents self-review.
fn may_review(actor: &User, repo: &RepoContext, run: &Run, environment: &Environment) -> Result<(), ServiceError> {
    if !repo.access.can_read() || !environment.reviewers.contains(&actor.id()) {
        return Err(ServiceError::Forbidden);
    }
    if environment.prevent_self_review && [run.trigger_actor, run.actor].contains(&Some(actor.id())) {
        return Err(ServiceError::Invalid(format!("{} does not let whoever started a run approve it", environment.name)));
    }
    Ok(())
}

/// An environment's selected patterns as patterns over full refs: `release/*` is a branch
/// pattern (`refs/heads/release/*`), `refs/tags/v*` a tag pattern, so a tag never passes for a
/// branch of the same name or the other way round. A `!` pattern excludes what it matches,
/// and the last pattern that matches a ref decides, as in workflow `branches:` filters.
fn ref_patterns(patterns: &[String]) -> Result<Patterns, rbit_workflow::pattern::PatternError> {
    Patterns::compile(patterns.iter().map(|raw| {
        let (bang, pattern) = raw.strip_prefix('!').map_or(("", raw.as_str()), |rest| ("!", rest));
        if pattern.starts_with("refs/heads/") || pattern.starts_with("refs/tags/") {
            format!("{bang}{pattern}")
        } else {
            format!("{bang}refs/heads/{pattern}")
        }
    }))
}

/// Whether `reference` (a full ref) may deploy under these selected patterns.
fn selected(patterns: &[String], reference: &str) -> bool {
    ref_patterns(patterns).is_ok_and(|patterns| !patterns.skip([reference]))
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::Forbidden) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|pattern| (*pattern).to_owned()).collect()
    }

    #[test]
    fn selected_patterns_tell_branches_from_tags() {
        let branches = patterns(&["main", "release/*"]);
        assert!(selected(&branches, "refs/heads/main") && selected(&branches, "refs/heads/release/1.0"));
        assert!(!selected(&branches, "refs/tags/main"), "a tag named like an allowed branch");
        assert!(!selected(&branches, "refs/heads/release/1.0/hotfix") && !selected(&branches, "refs/pull/1/merge"));
        let tags = patterns(&["refs/tags/v*"]);
        assert!(selected(&tags, "refs/tags/v1.2") && !selected(&tags, "refs/heads/v1.2"), "a branch named like a tag");
        let explicit = patterns(&["refs/heads/main"]);
        assert!(selected(&explicit, "refs/heads/main") && !selected(&explicit, "refs/tags/main"));
    }

    #[test]
    fn the_last_matching_selected_pattern_decides() {
        let all_but_release = patterns(&["**", "!release/**"]);
        assert!(selected(&all_but_release, "refs/heads/main"));
        assert!(!selected(&all_but_release, "refs/heads/release/x"), "excluded");
        let readmitted = patterns(&["**", "!release/**", "release/ok"]);
        assert!(selected(&readmitted, "refs/heads/release/ok") && !selected(&readmitted, "refs/heads/release/no"));
        assert!(!selected(&patterns(&["**", "!refs/tags/**"]), "refs/tags/v1"), "`**` names branches only anyway");
    }
}
