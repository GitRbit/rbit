//! Branch protection service adapter: maps the `rbit-access` protection applications onto
//! repository contexts, service errors, tracing and the audit log.
//!
//! Hooks other code calls (signatures fixed by `docs/design/milestone-2b.md` §3):
//! * [`ProtectionService::check_push`] — the push gate in both receive paths
//!   (`rbit-web/src/routes/git_http.rs`, `rbit-ssh/src/session.rs`), after the access,
//!   mirror and redirect checks and before git starts;
//! * [`ProtectionService::check_merge`] and [`ProtectionService::may_delete_branch`] — P's merge;
//! * [`ProtectionService::protected_branches`] — the branches page's badges.
//!
//! Rule management (administrators only, validation, allowed users and teams) and the
//! decisions live in [`rbit_access::protection`]; the rule store keeps the rule limit,
//! pattern uniqueness and organisation-only teams atomic. This adapter refuses pushes to
//! archived repositories before any rule is consulted ([`RepoContext::ensure_writable`]),
//! resolves allowed users' write access through the repository access policy, and audits
//! every rule change ("Repository access") and every administrator bypass that changed a
//! push's outcome (`BranchProtectionBypassed`).

use std::future::Future;

use rbit_access::{
    organizations::AccessError,
    protection::{Principal, ProtectionGate, RuleDirectory, RuleDraft, RuleManagement, RuleScope, Writer},
};
use rbit_core::access::repo_access;
pub use rbit_core::protection::{MergeRules, PushActor, PushVerdict, Refusal};
use rbit_core::{
    AccountId, AccountKind, RefUpdate, RepoId, Timestamp, User,
    membership::{Team, TeamId},
    protection::{BranchProtection, ProtectionId, RuleSettings, is_protected},
};
use rbit_git::RefKind;
use rbit_storage::sqlite::protection::BranchProtectionStore;

use crate::{
    AuditService, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    repositories::viewer_grants,
};

/// Scratch refs one person may hold in one repository at a time.
pub const SCRATCH_REFS_PER_OWNER: u64 = 20;

/// The rule's protections in a few words, for the audit log.
pub use rbit_access::protection::summary;

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

    fn draft(&self) -> RuleDraft<'_> {
        RuleDraft { pattern: &self.pattern, settings: self.settings, users: &self.users, teams: &self.teams, contexts: &self.contexts }
    }
}

/// Name lookups for one repository: allowed users need write access to it under the
/// repository access policy, allowed teams belong to its owning organisation.
struct RepoDirectory<'a> {
    ctx: &'a ServiceContext,
    repo: &'a RepoContext,
}

impl RuleDirectory for RepoDirectory<'_> {
    type Error = ServiceError;
    type User = AccountId;
    type Team = TeamId;
    type OwnerTeam = Team;

    async fn writer(&self, name: &str) -> Result<Writer<AccountId>, ServiceError> {
        let accounts = self.ctx.database.accounts();
        let Some(account) = accounts.find_account_by_name(name).await? else { return Ok(Writer::Unknown) };
        if account.kind != AccountKind::User {
            return Ok(Writer::Unknown);
        }
        let Some(user) = accounts.find_user(account.id).await? else { return Ok(Writer::Unknown) };
        let grants = viewer_grants(&self.ctx.database.repositories(), Some(&user), &self.repo.repo).await?;
        if !repo_access(Some(&user), &self.repo.repo, &grants).can_write() {
            return Ok(Writer::WithoutWrite(user.name().to_string()));
        }
        Ok(Writer::Writer(user.id()))
    }

    async fn team_named(&self, name: &str) -> Result<Option<TeamId>, ServiceError> {
        Ok(self.ctx.database.teams().find_by_name(self.repo.repo.owner.id, name).await?.map(|team| team.id))
    }

    fn owner_teams(&self) -> impl Future<Output = Result<Vec<Team>, ServiceError>> + Send {
        let teams = self.ctx.database.teams();
        let owner = self.repo.repo.owner.id;
        async move { Ok(teams.list(owner).await?.into_iter().map(|summary| summary.team).collect()) }
    }
}

fn scope(repo: &RepoContext) -> RuleScope<'_, RepoId> {
    RuleScope {
        repo: repo.repo.repo.id,
        owner: repo.repo.owner.name.as_str(),
        organization: repo.repo.owner.is_organization(),
        can_admin: repo.access.can_admin(),
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
        let (store, directory) = (self.store(), RepoDirectory { ctx: &self.ctx, repo });
        RuleManagement::new(&store, &directory).list(scope(repo)).await.map_err(managed)
    }

    /// Rule `id`; [`ServiceError::NotFound`] if the repository has no such rule.
    pub async fn get(&self, repo: &RepoContext, id: ProtectionId) -> Result<BranchProtection, ServiceError> {
        let (store, directory) = (self.store(), RepoDirectory { ctx: &self.ctx, repo });
        RuleManagement::new(&store, &directory).get(scope(repo), id).await.map_err(managed)
    }

    /// The teams of the owning organisation, by name, to pick allowed teams from (empty
    /// for personal repositories).
    pub async fn teams(&self, repo: &RepoContext) -> Result<Vec<Team>, ServiceError> {
        let (store, directory) = (self.store(), RepoDirectory { ctx: &self.ctx, repo });
        RuleManagement::new(&store, &directory).owner_teams(scope(repo)).await.map_err(managed)
    }

    /// The repository's branch names, for "Matches: …" on the rule pages.
    pub async fn branch_names(&self, repo: &RepoContext) -> Result<Vec<String>, ServiceError> {
        let refs = repo.reader.refs().await?;
        Ok(refs.into_iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name).collect())
    }

    /// Adds a rule.
    pub async fn create(&self, actor: &User, repo: &RepoContext, input: &ProtectionInput) -> Result<BranchProtection, ServiceError> {
        let (store, directory) = (self.store(), RepoDirectory { ctx: &self.ctx, repo });
        let management = RuleManagement::new(&store, &directory);
        let (id, rule) = management.create(scope(repo), actor.id(), input.draft()).await.map_err(managed)?;
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
        let (store, directory) = (self.store(), RepoDirectory { ctx: &self.ctx, repo });
        let management = RuleManagement::new(&store, &directory);
        let (current, rule) = management.update(scope(repo), id, input.draft()).await.map_err(managed)?;
        let renamed = if current.rule.pattern == rule.pattern { String::new() } else { format!(", was {}", current.rule.pattern) };
        tracing::info!(repo = %repo.repo.full_name(), pattern = %rule.pattern, by = %actor.name(), "branch protection rule changed");
        let detail = format!("{} ({}{renamed})", rule.pattern, summary(&rule.settings));
        self.record(AuditAction::BranchProtectionChanged, actor, repo, detail).await;
        self.get(repo, id).await
    }

    /// Deletes rule `id`.
    pub async fn delete(&self, actor: &User, repo: &RepoContext, id: ProtectionId) -> Result<(), ServiceError> {
        let (store, directory) = (self.store(), RepoDirectory { ctx: &self.ctx, repo });
        let current = RuleManagement::new(&store, &directory).delete(scope(repo), id).await.map_err(managed)?;
        tracing::info!(repo = %repo.repo.full_name(), pattern = %current.rule.pattern, by = %actor.name(), "branch protection rule deleted");
        self.record(AuditAction::BranchProtectionDeleted, actor, repo, current.rule.pattern.to_string()).await;
        Ok(())
    }

    async fn record(&self, action: AuditAction, actor: &User, repo: &RepoContext, detail: String) {
        self.audit.record(AuditEntry::new(action).by(actor).on(repo.repo.full_name()).detail(detail)).await;
    }

    // ------------------------------------------------------------ decisions

    /// Decides the ref updates a client asked for, before any object arrives. `repo` is the
    /// resolved repository (never a mirror) and `actor` already has write access. An
    /// archived repository refuses every update ([`RepoContext::ensure_writable`]).
    pub async fn check_push(&self, repo: &RepoContext, actor: &PushActor<'_>, updates: &[RefUpdate]) -> Result<PushVerdict, ServiceError> {
        if let Err(archived) = repo.ensure_writable() {
            tracing::info!(repo = %repo.repo.full_name(), "push to an archived repository refused");
            return Ok(PushVerdict::refuse_all(updates, &archived.to_string()));
        }
        let principal = Principal { user: actor.user.map(User::id), deploy_key: actor.deploy_key, access: actor.access };
        let store = self.store();
        let mut decision = ProtectionGate::new(&store).check_push(repo.repo.repo.id, principal, updates).await?;
        self.check_scratch(repo, actor, updates, &mut decision.verdict).await?;
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

    /// Scratch refs (`refs/scratch/<owner>/<name>`): only a person may push them, only under
    /// their own name, and a new one only while they have fewer than
    /// [`SCRATCH_REFS_PER_OWNER`] in the repository. Deleting one's own is always allowed.
    /// An allowed update is recorded here, so the sweep expires it even if Actions is off; a
    /// push that then fails leaves a record whose ref the sweep finds gone.
    async fn check_scratch(&self, repo: &RepoContext, actor: &PushActor<'_>, updates: &[RefUpdate], verdict: &mut PushVerdict) -> Result<(), ServiceError> {
        for update in updates.iter().filter(|update| update.name.starts_with(rbit_git::scratch::PREFIX)) {
            let owner = rbit_git::scratch::owner(&update.name);
            let reason = match actor.user {
                Some(user) if owner == Some(user.name().as_str()) => {
                    let new = update.before.is_none() && update.after.is_some();
                    let held = if new { self.ctx.database.actions_runs().count_scratch(repo.repo.repo.id, user.id(), &update.name).await? } else { 0 };
                    (held >= SCRATCH_REFS_PER_OWNER).then(|| format!("you have {held} scratch refs here; delete one or wait for them to expire"))
                }
                _ => Some("refs/scratch/<you>/… is for your own scratch commits".to_owned()),
            };
            let store = self.ctx.database.actions_runs();
            match (reason, &update.after, &update.before) {
                (Some(reason), _, _) => verdict.refusals.push(Refusal { name: update.name.clone(), reason }),
                (None, Some(after), _) => store.record_scratch(repo.repo.repo.id, &update.name, actor.user.map(User::id), after, Timestamp::now()).await?,
                (None, None, Some(before)) => store.forget_scratch(repo.repo.repo.id, &update.name, before).await?,
                (None, None, None) => {}
            }
        }
        Ok(())
    }

    /// What merging into `branch` requires of `merger` (`repo` is opened for the merger).
    pub async fn check_merge(&self, repo: &RepoContext, merger: &User, branch: &str) -> Result<MergeRules, ServiceError> {
        let store = self.store();
        Ok(ProtectionGate::new(&store).merge(repo.repo.repo.id, merger.id(), repo.access, branch).await?)
    }

    /// Whether `actor` may delete `branch` (P's "delete head branch"): the same decision as
    /// a push deleting it (`repo` is opened for the actor).
    pub async fn may_delete_branch(&self, repo: &RepoContext, actor: &User, branch: &str) -> Result<bool, ServiceError> {
        let store = self.store();
        Ok(ProtectionGate::new(&store).may_delete(repo.repo.repo.id, actor.id(), repo.access, branch).await?)
    }

    /// Why `actor` may not add commits to the existing `branch` directly, or `None` when
    /// they may: the decision a push updating it gets, without auditing a bypass (E's
    /// editor uses it to offer committing to a new branch instead; the commit itself goes
    /// through [`Self::check_push`]). `repo` is opened for the actor.
    pub async fn branch_update_refusal(&self, repo: &RepoContext, actor: &User, branch: &str) -> Result<Option<String>, ServiceError> {
        if let Err(archived) = repo.ensure_writable() {
            return Ok(Some(archived.to_string()));
        }
        let store = self.store();
        Ok(ProtectionGate::new(&store).branch_update_refusal(repo.repo.repo.id, actor.id(), repo.access, branch).await?)
    }

    /// The names of the repository's existing branches that some rule protects.
    pub async fn protected_branches(&self, repo: &RepoContext) -> Result<Vec<String>, ServiceError> {
        let store = self.store();
        let rules = ProtectionGate::new(&store).rules(repo.repo.repo.id).await?;
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        let branches = self.branch_names(repo).await?;
        Ok(branches.into_iter().filter(|branch| is_protected(&rules, branch)).collect())
    }

    /// Evaluate only the names requested by a paged listing.
    pub async fn protected_branch_names(&self, repo: &RepoContext, branches: &[String]) -> Result<Vec<String>, ServiceError> {
        let store = self.store();
        let rules = ProtectionGate::new(&store).rules(repo.repo.repo.id).await?;
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        Ok(branches.iter().filter(|branch| is_protected(&rules, branch)).cloned().collect())
    }
}

/// Rule outcomes keep their meaning; lookups already speak service errors.
fn managed(error: AccessError<ServiceError>) -> ServiceError {
    match error {
        AccessError::NotFound => ServiceError::NotFound,
        AccessError::Forbidden => ServiceError::Forbidden,
        AccessError::Invalid(message) => ServiceError::Invalid(message),
        AccessError::Store(error) => error,
    }
}
