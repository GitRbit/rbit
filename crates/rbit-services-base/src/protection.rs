//! Branch protection: managing rules, the push gate's decision and merge requirements.
//!
//! Milestone 2 workstream B (branch protection) owns this file.
//!
//! Hooks other code calls (signatures fixed by `docs/design/milestone-2b.md` §3):
//! * [`ProtectionService::check_push`] — the push gate in both receive paths
//!   (`rbit-web/src/routes/git_http.rs`, `rbit-ssh/src/session.rs`), after the access,
//!   mirror and redirect checks and before git starts;
//! * [`ProtectionService::check_merge`] and [`ProtectionService::may_delete_branch`] — P's merge;
//! * [`ProtectionService::protected_branches`] — the branches page's badges.
//!
//! Rules:
//! * Repository administrators list, add, change and delete rules (everyone else gets
//!   [`ServiceError::Forbidden`]; pages and the API answer 404 first when the repository
//!   is hidden). Every change is audited ("Repository access").
//! * Allowed users must have write access; allowed teams must be teams of the owning
//!   organisation; at most [`MAX_ALLOWED`] of each. Without `restrict_pushers` the lists
//!   are not kept.
//! * `require_status_checks` (milestone 3, S) needs 1 to [`MAX_REQUIRED_CONTEXTS`]
//!   distinct contexts; without the flag they are not kept.
//! * Archived repositories refuse every push before any rule is consulted
//!   ([`RepoContext::ensure_writable`]).
//! * The decisions themselves are pure ([`rbit_core::protection`]); this service only
//!   loads the rules and the pusher's teams. Bypasses by administrators that changed a
//!   push's outcome are audited as `BranchProtectionBypassed`.

use rbit_auth::repo_access;
pub use rbit_core::protection::{MergeRules, PushActor, PushVerdict, Refusal};
use rbit_core::{
    AccountKind, RefUpdate, User,
    membership::{Team, TeamId},
    protection::{self, BranchPattern, BranchProtection, MAX_ALLOWED, ProtectionId, ProtectionRule, Pusher, RuleSettings},
    status::{MAX_REQUIRED_CONTEXTS, StatusContext},
};
use rbit_git::RefKind;
use rbit_storage::{
    StorageError,
    sqlite::protection::{BranchProtectionStore, RuleChange, RuleInput},
};

use crate::{
    AuditService, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    repositories::viewer_grants,
};

/// A rule as submitted: the pattern and names as typed, the flags as ticked.
#[derive(Debug, Clone, Default)]
pub struct ProtectionInput {
    pub pattern: String,
    pub settings: RuleSettings,
    /// Usernames allowed to push (with `restrict_pushers`).
    pub users: Vec<String>,
    /// Names of the organisation's teams allowed to push (with `restrict_pushers`).
    pub teams: Vec<String>,
    /// Status check contexts that must pass (with `require_status_checks`).
    pub contexts: Vec<String>,
}

impl ProtectionInput {
    /// The values of a stored rule, for editing.
    pub fn from_rule(rule: &BranchProtection) -> Self {
        Self {
            pattern: rule.rule.pattern.to_string(),
            settings: rule.rule.settings,
            users: rule.users.iter().map(|user| user.name.to_string()).collect(),
            teams: rule.teams.iter().map(|team| team.name.to_string()).collect(),
            contexts: rule.rule.required_contexts.iter().map(ToString::to_string).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProtectionService {
    ctx: ServiceContext,
    audit: AuditService,
}

impl ProtectionService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { audit: AuditService::new(ctx.clone()), ctx }
    }

    fn store(&self) -> BranchProtectionStore {
        self.ctx.database.branch_protection()
    }

    // ------------------------------------------------------------ managing rules

    /// The repository's rules, by pattern. Administrators only.
    pub async fn list(&self, repo: &RepoContext) -> Result<Vec<BranchProtection>, ServiceError> {
        require_admin(repo)?;
        Ok(self.store().list(repo.repo.repo.id).await?)
    }

    /// Rule `id`; [`ServiceError::NotFound`] if the repository has no such rule.
    pub async fn get(&self, repo: &RepoContext, id: ProtectionId) -> Result<BranchProtection, ServiceError> {
        require_admin(repo)?;
        self.store().get(repo.repo.repo.id, id).await?.ok_or(ServiceError::NotFound)
    }

    /// The teams of the owning organisation, by name, to pick allowed teams from (empty
    /// for personal repositories).
    pub async fn teams(&self, repo: &RepoContext) -> Result<Vec<Team>, ServiceError> {
        require_admin(repo)?;
        if !repo.repo.owner.is_organization() {
            return Ok(Vec::new());
        }
        let teams = self.ctx.database.teams().list(repo.repo.owner.id).await?;
        Ok(teams.into_iter().map(|summary| summary.team).collect())
    }

    /// The repository's branch names, for "Matches: …" on the rule pages.
    pub async fn branch_names(&self, repo: &RepoContext) -> Result<Vec<String>, ServiceError> {
        let refs = repo.reader.refs().await?;
        Ok(refs.into_iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name).collect())
    }

    /// Adds a rule.
    pub async fn create(&self, actor: &User, repo: &RepoContext, input: &ProtectionInput) -> Result<BranchProtection, ServiceError> {
        require_admin(repo)?;
        let rule = self.validate(repo, input).await?;
        let id = match self.store().create(repo.repo.repo.id, &rule, actor.id()).await.map_err(pattern_taken)? {
            RuleChange::Saved(id) => id,
            RuleChange::TooManyRules => {
                return Err(ServiceError::Invalid(format!("a repository can have at most {} rules", protection::MAX_RULES)));
            }
            RuleChange::TeamOutsideOrganization | RuleChange::NotFound => return Err(team_outside(repo)),
        };
        tracing::info!(repo = %repo.repo.full_name(), pattern = %rule.pattern, by = %actor.name(), "branch protection rule added");
        self.record(AuditAction::BranchProtectionCreated, actor, repo, format!("{} ({})", rule.pattern, summary(&rule.settings))).await;
        self.get(repo, id).await
    }

    /// Replaces rule `id`.
    pub async fn update(
        &self,
        actor: &User,
        repo: &RepoContext,
        id: ProtectionId,
        input: &ProtectionInput,
    ) -> Result<BranchProtection, ServiceError> {
        let current = self.get(repo, id).await?;
        let rule = self.validate(repo, input).await?;
        match self.store().update(repo.repo.repo.id, id, &rule).await.map_err(pattern_taken)? {
            RuleChange::Saved(_) => {}
            RuleChange::NotFound => return Err(ServiceError::NotFound),
            RuleChange::TeamOutsideOrganization | RuleChange::TooManyRules => return Err(team_outside(repo)),
        }
        let renamed = if current.rule.pattern == rule.pattern { String::new() } else { format!(", was {}", current.rule.pattern) };
        tracing::info!(repo = %repo.repo.full_name(), pattern = %rule.pattern, by = %actor.name(), "branch protection rule changed");
        let detail = format!("{} ({}{renamed})", rule.pattern, summary(&rule.settings));
        self.record(AuditAction::BranchProtectionChanged, actor, repo, detail).await;
        self.get(repo, id).await
    }

    /// Deletes rule `id`.
    pub async fn delete(&self, actor: &User, repo: &RepoContext, id: ProtectionId) -> Result<(), ServiceError> {
        let current = self.get(repo, id).await?;
        if !self.store().delete(repo.repo.repo.id, id).await? {
            return Err(ServiceError::NotFound);
        }
        tracing::info!(repo = %repo.repo.full_name(), pattern = %current.rule.pattern, by = %actor.name(), "branch protection rule deleted");
        self.record(AuditAction::BranchProtectionDeleted, actor, repo, current.rule.pattern.to_string()).await;
        Ok(())
    }

    /// Checks `input` and resolves its names.
    async fn validate(&self, repo: &RepoContext, input: &ProtectionInput) -> Result<RuleInput, ServiceError> {
        let pattern = BranchPattern::parse(&input.pattern).map_err(|error| ServiceError::Invalid(error.to_string()))?;
        input.settings.validate().map_err(|error| ServiceError::Invalid(error.to_string()))?;
        let mut rule = RuleInput { pattern, settings: input.settings, users: Vec::new(), teams: Vec::new(), contexts: Vec::new() };
        if input.settings.require_status_checks {
            rule.contexts = required_contexts(&input.contexts)?;
        }
        if !input.settings.restrict_pushers {
            return Ok(rule);
        }
        let users = distinct(&input.users);
        let teams = distinct(&input.teams);
        if users.len() > MAX_ALLOWED || teams.len() > MAX_ALLOWED {
            return Err(ServiceError::Invalid(format!("a rule can allow at most {MAX_ALLOWED} users and {MAX_ALLOWED} teams")));
        }
        for name in users {
            rule.users.push(self.writer_called(repo, name).await?);
        }
        if !teams.is_empty() && !repo.repo.owner.is_organization() {
            return Err(ServiceError::Invalid("only repositories of organizations can allow teams".to_owned()));
        }
        for name in teams {
            let team = self.ctx.database.teams().find_by_name(repo.repo.owner.id, name).await?;
            let team = team.ok_or_else(|| ServiceError::Invalid(format!("{} has no team called {name}", repo.repo.owner.name)))?;
            rule.teams.push(team.id);
        }
        Ok(rule)
    }

    /// The user called `name`, who must have write access to the repository.
    async fn writer_called(&self, repo: &RepoContext, name: &str) -> Result<rbit_core::AccountId, ServiceError> {
        let unknown = || ServiceError::Invalid(format!("there is no user called {name}"));
        let account = self.ctx.database.accounts().find_account_by_name(name).await?.ok_or_else(unknown)?;
        if account.kind != AccountKind::User {
            return Err(unknown());
        }
        let user = self.ctx.database.accounts().find_user(account.id).await?.ok_or_else(unknown)?;
        let grants = viewer_grants(&self.ctx.database.repositories(), Some(&user), &repo.repo).await?;
        if !repo_access(Some(&user), &repo.repo, &grants).can_write() {
            return Err(ServiceError::Invalid(format!("{} does not have write access to this repository", user.name())));
        }
        Ok(user.id())
    }

    async fn record(&self, action: AuditAction, actor: &User, repo: &RepoContext, detail: String) {
        self.audit.record(AuditEntry::new(action).by(actor).on(repo.repo.full_name()).detail(detail)).await;
    }

    // ------------------------------------------------------------ decisions

    /// The rules the decisions need.
    async fn rules(&self, repo: &RepoContext) -> Result<Vec<ProtectionRule>, ServiceError> {
        Ok(self.store().list(repo.repo.repo.id).await?.into_iter().map(|protection| protection.rule).collect())
    }

    /// `user` as the decisions see them, with their teams when some rule names teams.
    async fn pusher(
        &self,
        repo: &RepoContext,
        rules: &[ProtectionRule],
        user: &User,
        access: rbit_core::RepoAccess,
    ) -> Result<Pusher, ServiceError> {
        let names_teams = rules.iter().any(|rule| rule.settings.restrict_pushers && !rule.allowed_teams.is_empty());
        let teams: Vec<TeamId> = if names_teams { self.store().teams_of(repo.repo.repo.id, user.id()).await? } else { Vec::new() };
        Ok(Pusher::User { id: user.id(), access, teams })
    }

    /// Decides the ref updates a client asked for, before any object arrives. `repo` is the
    /// resolved repository (never a mirror) and `actor` already has write access. An
    /// archived repository refuses every update ([`RepoContext::ensure_writable`]).
    pub async fn check_push(&self, repo: &RepoContext, actor: &PushActor<'_>, updates: &[RefUpdate]) -> Result<PushVerdict, ServiceError> {
        if let Err(archived) = repo.ensure_writable() {
            tracing::info!(repo = %repo.repo.full_name(), "push to an archived repository refused");
            return Ok(PushVerdict::refuse_all(updates, &archived.to_string()));
        }
        let rules = self.rules(repo).await?;
        let pusher = match actor.user {
            Some(user) if !actor.deploy_key => self.pusher(repo, &rules, user, actor.access).await?,
            // Deploy keys (and anything without an account) are never listed nor admins.
            _ => Pusher::DeployKey { access: actor.access },
        };
        let decision = protection::decide_push(&rules, &pusher, updates);
        if let Some(user) = actor.user {
            for bypass in &decision.bypasses {
                tracing::info!(repo = %repo.repo.full_name(), r#ref = %bypass.name, by = %user.name(), "branch protection bypassed");
                let detail = format!("{} (otherwise {})", bypass.name, bypass.instead);
                self.record(AuditAction::BranchProtectionBypassed, user, repo, detail).await;
            }
        }
        for refusal in &decision.verdict.refusals {
            tracing::info!(repo = %repo.repo.full_name(), r#ref = %refusal.name, reason = %refusal.reason, "push refused");
        }
        Ok(decision.verdict)
    }

    /// What merging into `branch` requires of `merger` (`repo` is opened for the merger).
    pub async fn check_merge(&self, repo: &RepoContext, merger: &User, branch: &str) -> Result<MergeRules, ServiceError> {
        let rules = self.rules(repo).await?;
        if !protection::is_protected(&rules, branch) {
            return Ok(MergeRules::unprotected());
        }
        let pusher = self.pusher(repo, &rules, merger, repo.access).await?;
        Ok(protection::merge_rules(&rules, &pusher, branch))
    }

    /// Whether `actor` may delete `branch` (P's "delete head branch"): the same decision as
    /// a push deleting it (`repo` is opened for the actor).
    pub async fn may_delete_branch(&self, repo: &RepoContext, actor: &User, branch: &str) -> Result<bool, ServiceError> {
        let rules = self.rules(repo).await?;
        let pusher = self.pusher(repo, &rules, actor, repo.access).await?;
        Ok(protection::may_delete(&rules, &pusher, branch))
    }

    /// Why `actor` may not add commits to the existing `branch` directly, or `None` when
    /// they may: the decision a push updating it gets, without auditing a bypass (E's
    /// editor uses it to offer committing to a new branch instead; the commit itself goes
    /// through [`Self::check_push`]). `repo` is opened for the actor.
    pub async fn branch_update_refusal(&self, repo: &RepoContext, actor: &User, branch: &str) -> Result<Option<String>, ServiceError> {
        if let Err(archived) = repo.ensure_writable() {
            return Ok(Some(archived.to_string()));
        }
        let rules = self.rules(repo).await?;
        let pusher = self.pusher(repo, &rules, actor, repo.access).await?;
        let update = RefUpdate { name: format!("refs/heads/{branch}"), before: Some(String::new()), after: Some(String::new()) };
        let decision = protection::decide_push(&rules, &pusher, &[update]);
        Ok(decision.verdict.refusals.into_iter().next().map(|refusal| refusal.reason))
    }

    /// The names of the repository's existing branches that some rule protects.
    pub async fn protected_branches(&self, repo: &RepoContext) -> Result<Vec<String>, ServiceError> {
        let rules = self.rules(repo).await?;
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        let branches = self.branch_names(repo).await?;
        Ok(branches.into_iter().filter(|branch| protection::is_protected(&rules, branch)).collect())
    }

    /// Evaluate only the names requested by a paged listing.
    pub async fn protected_branch_names(&self, repo: &RepoContext, branches: &[String]) -> Result<Vec<String>, ServiceError> {
        let rules = self.rules(repo).await?;
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        Ok(branches.iter().filter(|branch| protection::is_protected(&rules, branch)).cloned().collect())
    }
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::Forbidden) }
}

/// The trimmed, non-empty, distinct entries of `names`, in order.
fn distinct(names: &[String]) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for name in names.iter().map(|name| name.trim()).filter(|name| !name.is_empty()) {
        if !seen.contains(&name) {
            seen.push(name);
        }
    }
    seen
}

/// The validated, distinct contexts a rule requires: at least one, at most
/// [`MAX_REQUIRED_CONTEXTS`]. Contexts are case-sensitive and may contain spaces.
fn required_contexts(contexts: &[String]) -> Result<Vec<StatusContext>, ServiceError> {
    let mut parsed: Vec<StatusContext> = Vec::new();
    for context in contexts.iter().filter(|context| !context.trim().is_empty()) {
        let context = StatusContext::parse(context).map_err(|error| ServiceError::Invalid(format!("required check: {error}")))?;
        if !parsed.contains(&context) {
            parsed.push(context);
        }
    }
    if parsed.is_empty() {
        return Err(ServiceError::Invalid("requiring status checks needs at least one required check".to_owned()));
    }
    if parsed.len() > MAX_REQUIRED_CONTEXTS {
        return Err(ServiceError::Invalid(format!("a rule can require at most {MAX_REQUIRED_CONTEXTS} status checks")));
    }
    Ok(parsed)
}

fn pattern_taken(error: StorageError) -> ServiceError {
    match error {
        StorageError::Conflict(_) => ServiceError::Invalid("a rule for this pattern already exists".to_owned()),
        other => other.into(),
    }
}

fn team_outside(repo: &RepoContext) -> ServiceError {
    ServiceError::Invalid(format!("allowed teams must be teams of {}", repo.repo.owner.name))
}

/// The rule's protections in a few words, for the audit log.
pub fn summary(settings: &RuleSettings) -> String {
    let mut parts = Vec::new();
    if settings.deny_force_push {
        parts.push("no force pushes".to_owned());
    }
    if settings.deny_deletion {
        parts.push("no deletion".to_owned());
    }
    if settings.require_pull_request {
        parts.push(match settings.required_approvals {
            0 => "pull request".to_owned(),
            1 => "1 approval".to_owned(),
            n => format!("{n} approvals"),
        });
    }
    if settings.require_status_checks {
        parts.push("status checks".to_owned());
    }
    if settings.restrict_pushers {
        parts.push("restricted".to_owned());
    }
    if settings.allow_admin_bypass {
        parts.push("admins bypass".to_owned());
    }
    if parts.is_empty() { "no restrictions".to_owned() } else { parts.join(", ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_and_names() {
        assert_eq!(summary(&RuleSettings::default()), "no force pushes, no deletion");
        let all = RuleSettings {
            require_pull_request: true,
            required_approvals: 2,
            restrict_pushers: true,
            allow_admin_bypass: true,
            ..RuleSettings::default()
        };
        assert_eq!(summary(&all), "no force pushes, no deletion, 2 approvals, restricted, admins bypass");
        let none = RuleSettings { deny_force_push: false, deny_deletion: false, ..RuleSettings::default() };
        assert_eq!(summary(&none), "no restrictions");
        let names = ["a".to_owned(), " b ".to_owned(), String::new(), "a".to_owned()];
        assert_eq!(distinct(&names), ["a", "b"]);
        assert_eq!(summary(&RuleSettings { require_status_checks: true, ..none }), "status checks");
    }

    #[test]
    fn required_contexts_are_validated() {
        let contexts = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
        let parsed = required_contexts(&contexts(&["ci/build", " ", "lint: all", "ci/build"])).unwrap();
        let names: Vec<_> = parsed.iter().map(StatusContext::as_str).collect();
        assert_eq!(names, ["ci/build", "lint: all"], "in order, distinct, spaces allowed");
        let message = |result: Result<Vec<StatusContext>, ServiceError>| match result {
            Err(ServiceError::Invalid(message)) => message,
            other => panic!("expected a validation error, got {other:?}"),
        };
        assert!(message(required_contexts(&contexts(&["", "  "]))).contains("at least one"));
        let many: Vec<String> = (0..=MAX_REQUIRED_CONTEXTS).map(|n| format!("check-{n}")).collect();
        assert!(message(required_contexts(&many)).contains("at most"));
        assert!(message(required_contexts(&["x".repeat(256)])).contains("required check"));
    }
}
