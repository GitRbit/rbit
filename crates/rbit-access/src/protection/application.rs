//! Rule management (administrators only, validated, names resolved) and the gate that
//! applies a repository's rules to pushes, merges and branch deletion.

use super::{
    BranchPattern, MAX_ALLOWED, MAX_REQUIRED_CONTEXTS, MAX_RULES, MergeRules, ProtectionId, PushDecision, Pusher, RefChange, RuleChange,
    RuleDirectory, RuleRepository, RuleSettings, StoredInput, StoredRule, Writer,
    decision::{BranchChange, decide_push, is_protected, may_delete, merge_rules},
};
use crate::{RepoAccess, organizations::AccessError, teams::Unique};

/// The repository a rule change applies to, as the caller resolved it for the actor.
#[derive(Debug, Clone, Copy)]
pub struct RuleScope<'a, R> {
    pub repo: R,
    /// The owning account's name, for messages.
    pub owner: &'a str,
    /// The owner is an organisation (only those can allow teams).
    pub organization: bool,
    /// The actor administers the repository.
    pub can_admin: bool,
}

/// A rule as submitted: the pattern and names as typed, the flags as ticked.
#[derive(Debug, Clone, Copy)]
pub struct RuleDraft<'a> {
    pub pattern: &'a str,
    pub settings: RuleSettings,
    /// Usernames allowed to push (kept only with `restrict_pushers`).
    pub users: &'a [String],
    /// Team names allowed to push (kept only with `restrict_pushers`).
    pub teams: &'a [String],
    /// Required contexts (kept only with `require_status_checks`).
    pub contexts: &'a [String],
}

/// Who is pushing, as the transports resolved them.
#[derive(Debug, Clone, Copy)]
pub struct Principal<U> {
    /// The signed-in user; `None` for deploy keys.
    pub user: Option<U>,
    pub deploy_key: bool,
    /// Access to the repository after the token scope.
    pub access: RepoAccess,
}

type Managed<T, D> = Result<T, AccessError<<D as RuleDirectory>::Error>>;

/// Administrators list, add, change and delete rules; everyone else is `Forbidden`.
pub struct RuleManagement<'a, S, D> {
    store: &'a S,
    directory: &'a D,
}

impl<S, D> std::fmt::Debug for RuleManagement<'_, S, D> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RuleManagement").finish_non_exhaustive()
    }
}

impl<'a, S, D> RuleManagement<'a, S, D>
where
    S: RuleRepository,
    D: RuleDirectory<User = S::User, Team = S::Team>,
    D::Error: From<S::Error>,
{
    pub fn new(store: &'a S, directory: &'a D) -> Self {
        Self { store, directory }
    }

    /// The repository's rules, by pattern.
    pub async fn list(&self, scope: RuleScope<'_, S::Repo>) -> Managed<Vec<S::Record>, D> {
        require_admin(scope)?;
        Ok(self.store.list(scope.repo).await.map_err(D::Error::from)?)
    }

    /// Rule `id`; `NotFound` if the repository has no such rule.
    pub async fn get(&self, scope: RuleScope<'_, S::Repo>, id: ProtectionId) -> Managed<S::Record, D> {
        require_admin(scope)?;
        self.store.get(scope.repo, id).await.map_err(D::Error::from)?.ok_or(AccessError::NotFound)
    }

    /// The owning organisation's teams to pick from (none for personal repositories).
    pub async fn owner_teams(&self, scope: RuleScope<'_, S::Repo>) -> Managed<Vec<D::OwnerTeam>, D> {
        require_admin(scope)?;
        if !scope.organization {
            return Ok(Vec::new());
        }
        Ok(self.directory.owner_teams().await?)
    }

    /// Adds a rule; returns its id and the validated values.
    pub async fn create(
        &self,
        scope: RuleScope<'_, S::Repo>,
        actor: S::User,
        draft: RuleDraft<'_>,
    ) -> Managed<(ProtectionId, StoredInput<S>), D> {
        require_admin(scope)?;
        let rule = self.validate(scope, draft).await?;
        match pattern_taken(self.store.create(scope.repo, &rule, actor).await.map_err(D::Error::from)?)? {
            RuleChange::Saved(id) => Ok((id, rule)),
            RuleChange::TooManyRules => Err(invalid(format!("a repository can have at most {MAX_RULES} rules"))),
            RuleChange::TeamOutsideOrganization | RuleChange::NotFound => Err(team_outside(scope)),
        }
    }

    /// Replaces rule `id`; returns the rule as it was and the validated new values.
    pub async fn update(
        &self,
        scope: RuleScope<'_, S::Repo>,
        id: ProtectionId,
        draft: RuleDraft<'_>,
    ) -> Managed<(S::Record, StoredInput<S>), D> {
        let current = self.get(scope, id).await?;
        let rule = self.validate(scope, draft).await?;
        match pattern_taken(self.store.update(scope.repo, id, &rule).await.map_err(D::Error::from)?)? {
            RuleChange::Saved(_) => Ok((current, rule)),
            RuleChange::NotFound => Err(AccessError::NotFound),
            RuleChange::TeamOutsideOrganization | RuleChange::TooManyRules => Err(team_outside(scope)),
        }
    }

    /// Deletes rule `id`; returns the rule as it was.
    pub async fn delete(&self, scope: RuleScope<'_, S::Repo>, id: ProtectionId) -> Managed<S::Record, D> {
        let current = self.get(scope, id).await?;
        if !self.store.delete(scope.repo, id).await.map_err(D::Error::from)? {
            return Err(AccessError::NotFound);
        }
        Ok(current)
    }

    /// Checks `draft` and resolves its names.
    async fn validate(&self, scope: RuleScope<'_, S::Repo>, draft: RuleDraft<'_>) -> Managed<StoredInput<S>, D> {
        let pattern = BranchPattern::parse(draft.pattern).map_err(|error| invalid(error.to_string()))?;
        draft.settings.validate().map_err(|error| invalid(error.to_string()))?;
        let mut rule = StoredInput::<S> { pattern, settings: draft.settings, users: Vec::new(), teams: Vec::new(), contexts: Vec::new() };
        if draft.settings.require_status_checks {
            rule.contexts = required_contexts(draft.contexts)?;
        }
        if !draft.settings.restrict_pushers {
            return Ok(rule);
        }
        let users = distinct(draft.users);
        let teams = distinct(draft.teams);
        if users.len() > MAX_ALLOWED || teams.len() > MAX_ALLOWED {
            return Err(invalid(format!("a rule can allow at most {MAX_ALLOWED} users and {MAX_ALLOWED} teams")));
        }
        for name in users {
            rule.users.push(match self.directory.writer(name).await? {
                Writer::Writer(user) => user,
                Writer::Unknown => return Err(invalid(format!("there is no user called {name}"))),
                Writer::WithoutWrite(user) => return Err(invalid(format!("{user} does not have write access to this repository"))),
            });
        }
        if !teams.is_empty() && !scope.organization {
            return Err(invalid("only repositories of organizations can allow teams".to_owned()));
        }
        for name in teams {
            let team = self.directory.team_named(name).await?;
            rule.teams.push(team.ok_or_else(|| invalid(format!("{} has no team called {name}", scope.owner)))?);
        }
        Ok(rule)
    }
}

/// Applies a repository's rules. Archived repositories are refused by the caller first.
pub struct ProtectionGate<'a, S> {
    store: &'a S,
}

impl<S> std::fmt::Debug for ProtectionGate<'_, S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ProtectionGate").finish_non_exhaustive()
    }
}

impl<'a, S: RuleRepository> ProtectionGate<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    /// The rules the decisions need.
    pub async fn rules(&self, repo: S::Repo) -> Result<Vec<StoredRule<S>>, S::Error> {
        Ok(self.store.list(repo).await?.into_iter().map(S::into_rule).collect())
    }

    /// `user` as the decisions see them, with their teams when some rule names teams.
    async fn pusher(
        &self,
        repo: S::Repo,
        rules: &[StoredRule<S>],
        user: S::User,
        access: RepoAccess,
    ) -> Result<Pusher<S::User, S::Team>, S::Error> {
        let names_teams = rules.iter().any(|rule| rule.settings.restrict_pushers && !rule.allowed_teams.is_empty());
        let teams = if names_teams { self.store.teams_of(repo, user).await? } else { Vec::new() };
        Ok(Pusher::User { id: user, access, teams })
    }

    /// Decides the ref updates a client asked for, before any object arrives.
    pub async fn check_push(
        &self,
        repo: S::Repo,
        principal: Principal<S::User>,
        updates: &[impl RefChange + Sync],
    ) -> Result<PushDecision, S::Error> {
        let rules = self.rules(repo).await?;
        let pusher = match principal.user {
            Some(user) if !principal.deploy_key => self.pusher(repo, &rules, user, principal.access).await?,
            // Deploy keys (and anything without an account) are never listed nor admins.
            _ => Pusher::DeployKey { access: principal.access },
        };
        Ok(decide_push(&rules, &pusher, updates))
    }

    /// What merging into `branch` requires of `merger`.
    pub async fn merge(
        &self,
        repo: S::Repo,
        merger: S::User,
        access: RepoAccess,
        branch: &str,
    ) -> Result<MergeRules<S::Context>, S::Error> {
        let rules = self.rules(repo).await?;
        if !is_protected(&rules, branch) {
            return Ok(MergeRules::unprotected());
        }
        let pusher = self.pusher(repo, &rules, merger, access).await?;
        Ok(merge_rules(&rules, &pusher, branch))
    }

    /// Whether `actor` may delete `branch`: the decision a push deleting it gets.
    pub async fn may_delete(&self, repo: S::Repo, actor: S::User, access: RepoAccess, branch: &str) -> Result<bool, S::Error> {
        let rules = self.rules(repo).await?;
        let pusher = self.pusher(repo, &rules, actor, access).await?;
        Ok(may_delete(&rules, &pusher, branch))
    }

    /// Why `actor` may not add commits to the existing `branch` directly, or `None`: the
    /// decision a push updating it gets, without reporting a bypass.
    pub async fn branch_update_refusal(
        &self,
        repo: S::Repo,
        actor: S::User,
        access: RepoAccess,
        branch: &str,
    ) -> Result<Option<String>, S::Error> {
        let rules = self.rules(repo).await?;
        let pusher = self.pusher(repo, &rules, actor, access).await?;
        let decision = decide_push(&rules, &pusher, &[BranchChange::update(branch)]);
        Ok(decision.verdict.refusals.into_iter().next().map(|refusal| refusal.reason))
    }
}

fn require_admin<E, R: Copy>(scope: RuleScope<'_, R>) -> Result<(), AccessError<E>> {
    if scope.can_admin { Ok(()) } else { Err(AccessError::Forbidden) }
}

fn invalid<E>(message: String) -> AccessError<E> {
    AccessError::Invalid(message)
}

fn pattern_taken<E>(write: Unique<RuleChange>) -> Result<RuleChange, AccessError<E>> {
    match write {
        Unique::Done(change) => Ok(change),
        Unique::Taken => Err(invalid("a rule for this pattern already exists".to_owned())),
    }
}

fn team_outside<E, R: Copy>(scope: RuleScope<'_, R>) -> AccessError<E> {
    invalid(format!("allowed teams must be teams of {}", scope.owner))
}

/// The trimmed, non-empty, distinct entries of `names`, in order.
pub(crate) fn distinct(names: &[String]) -> Vec<&str> {
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
pub(crate) fn required_contexts<C, E>(contexts: &[String]) -> Result<Vec<C>, AccessError<E>>
where
    C: std::str::FromStr<Err: std::fmt::Display> + PartialEq,
{
    let mut parsed: Vec<C> = Vec::new();
    for context in contexts.iter().filter(|context| !context.trim().is_empty()) {
        let context: C = context.parse().map_err(|error| invalid(format!("required check: {error}")))?;
        if !parsed.contains(&context) {
            parsed.push(context);
        }
    }
    if parsed.is_empty() {
        return Err(invalid("requiring status checks needs at least one required check".to_owned()));
    }
    if parsed.len() > MAX_REQUIRED_CONTEXTS {
        return Err(invalid(format!("a rule can require at most {MAX_REQUIRED_CONTEXTS} status checks")));
    }
    Ok(parsed)
}
