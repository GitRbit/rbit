//! Issues: creation with metadata, labels, milestones, assignees, commit references and
//! search. The comments, state changes and timeline shared with pull requests live in
//! [`crate::timeline`].
//!
//! Milestone 2 workstream I (issues) owns this file and `issues/**`.
//!
//! Hooks other workstreams call (signatures fixed by `docs/design/milestone-2b.md` §3):
//! [`IssueService::metadata`], [`hooks::payload`] and [`json`].
//!
//! Labels, milestones and assignees attach to the shared `issues` row, so they work on
//! pull requests too: every change records its timeline event and publishes
//! `DomainEvent::Issue` with the thread's own kind. Triage (labels, milestone, assignees)
//! needs `can_triage` from [`issue_permissions`], recomputed for the actor on every call.
//! Creating and changing things spends the actor's [`ThrottleScope::Content`] budget.
//!
//! The background subscriber ([`IssueService::spawn_background`]) turns references into
//! timeline entries: `#N` and `owner/repo#N` in threads and comments become
//! `CrossReferenced` events on their targets, `#N` in pushed commits `CommitReferenced`,
//! and closing keywords in commits pushed to the default branch (or in the body of a
//! merged pull request) close the issues they name.

mod creation;
pub mod hooks;
pub mod json;
mod labels;
mod milestones;

use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_auth::repo_access;
use rbit_core::{
    Account, AccountId, Issue, IssueId, IssueKind, IssueNumber, IssueState, LabelId, MilestoneId, RepoAccess, RepoId, StateReason,
    TimelineEvent, Timestamp, User,
    issue::{IssueAction, LabelSnapshot, issue_permissions},
    issue_query::{IssueQuery, KindFilter, StateFilter, Who},
    references,
    repository::OwnedRepository,
};
#[cfg(test)]
use rbit_storage::sqlite::issue_meta::LabelFields;
#[cfg(test)]
use rbit_storage::sqlite::issue_meta::MilestoneFields;
pub use rbit_storage::sqlite::{
    issue_meta::{Label, LabelUsage, Milestone, MilestoneProgress},
    issue_search::{DashboardRelation, DashboardScope, SearchFilter},
};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    DomainEvent, IssueContext, RepoContext, ServiceContext, ServiceError, ThrottleScope, ThrottleService, TimelineService,
    audit::{AuditAction, AuditEntry, AuditService},
    events::RefUpdate,
    references::ReferenceService,
    repositories::viewer_grants,
};

pub use rbit_collaboration::issues::{MAX_ASSIGNEES, MAX_LABELS_PER_ISSUE};
/// Most users [`IssueService::assignable`] lists.
pub const MAX_ASSIGNABLE: u32 = 100;
/// Threads per page of a list.
pub const ISSUES_PER_PAGE: u32 = 25;
pub use rbit_collaboration::milestones::{MAX_MILESTONE_DESCRIPTION_BYTES, MAX_MILESTONE_TITLE_CHARS};

/// How long shutdown waits for the subscriber.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

pub use rbit_collaboration::labels::{DEFAULT_LABELS, MAX_LABEL_DESCRIPTION_CHARS, MAX_LABEL_NAME_CHARS, MAX_LABELS_PER_REPO};

/// Labels, assignees and milestone of one issue or pull request, for lists, payloads and
/// the API.
#[derive(Debug, Clone, Default)]
pub struct IssueMetadata {
    pub labels: Vec<LabelSnapshot>,
    pub assignees: Vec<Account>,
    pub milestone: Option<MilestoneRef>,
}

pub use rbit_collaboration::milestones::MilestoneRef;

pub use rbit_collaboration::issues::NewIssueInput;

pub use rbit_collaboration::labels::LabelInput;

pub use rbit_collaboration::milestones::MilestoneInput;

/// One page of a list with its tab counts.
#[derive(Debug, Clone)]
pub struct IssueSearchResults {
    pub issues: Vec<Issue>,
    /// Authors of the listed threads, by id.
    pub authors: HashMap<AccountId, Account>,
    pub metadata: HashMap<IssueId, IssueMetadata>,
    /// Threads matching the query, in its state.
    pub total: u64,
    /// The query's other filters, open and closed.
    pub open: u64,
    pub closed: u64,
    /// 1-based page actually served.
    pub page: u32,
    pub per_page: u32,
    /// Problems with the query (unknown qualifiers, `@me` signed out).
    pub hints: Vec<String>,
    /// The listed threads' repositories the viewer may read, by id: filled by
    /// [`IssueService::dashboard`] (whose rows come from many repositories), empty
    /// otherwise.
    pub repositories: HashMap<RepoId, OwnedRepository>,
}

/// Which of a listed thread's metadata a page shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MetadataNeeds {
    /// Labels, assignees and milestone.
    #[default]
    All,
    /// Labels only (the pull request list): one statement instead of three.
    Labels,
}

impl IssueSearchResults {
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.total.div_ceil(u64::from(self.per_page)).max(1)).unwrap_or(u32::MAX)
    }
}

// ------------------------------------------------------------------ validation

fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::Invalid(message.into())
}

/// `#rrggbb` (lower case) from `#RRGGBB` or `rrggbb`.
pub fn normalise_color(color: &str) -> Result<String, ServiceError> {
    rbit_collaboration::labels::normalise_color(color).map_err(ServiceError::Invalid)
}

#[cfg(test)]
fn label_fields(input: &LabelInput) -> Result<LabelFields, ServiceError> {
    rbit_collaboration::labels::label_fields(input).map_err(ServiceError::Invalid)
}

pub use rbit_collaboration::milestones::is_calendar_day;

#[cfg(test)]
fn milestone_fields(input: &MilestoneInput) -> Result<MilestoneFields, ServiceError> {
    rbit_collaboration::milestones::milestone_fields(input).map_err(ServiceError::Invalid)
}

/// Distinct items in their first order.
fn distinct<T: Ord + Copy>(items: &[T]) -> Vec<T> {
    let mut seen = BTreeSet::new();
    items.iter().copied().filter(|item| seen.insert(*item)).collect()
}

// ------------------------------------------------------------------ the service

#[derive(Debug)]
struct Background {
    started: AtomicBool,
    shutdown: CancellationToken,
    tasks: TaskTracker,
}

#[derive(Debug, Clone)]
pub struct IssueService {
    ctx: ServiceContext,
    timeline: TimelineService,
    references: ReferenceService,
    throttle: ThrottleService,
    audit: AuditService,
    background: Arc<Background>,
}

impl IssueService {
    pub(crate) fn new(ctx: ServiceContext, timeline: TimelineService, references: ReferenceService, throttle: ThrottleService) -> Self {
        let audit = AuditService::from_store(ctx.database.audit());
        let background =
            Arc::new(Background { started: AtomicBool::new(false), shutdown: CancellationToken::new(), tasks: TaskTracker::new() });
        Self { ctx, timeline, references, throttle, audit, background }
    }

    fn spend(&self, actor: &User) -> Result<(), ServiceError> {
        self.throttle.check(ThrottleScope::Content, &actor.id().0.to_string())
    }

    fn publish(&self, issue: &Issue, actor: Option<AccountId>, action: IssueAction) {
        self.ctx.events.publish(DomainEvent::Issue {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: issue.kind,
            actor,
            action,
        });
    }

    /// `actor`'s access to `repo` right now (for assignees, who are not the viewer).
    async fn access_of(&self, user: &User, repo: &OwnedRepository) -> Result<RepoAccess, ServiceError> {
        let grants = viewer_grants(&self.ctx.database.repositories(), Some(user), repo).await?;
        Ok(repo_access(Some(user), repo, &grants))
    }

    // ------------------------------------------------------------------ the hook

    /// Labels, assignees and milestone of each of `issues`: every id gets an entry, empty
    /// when it has none (or does not exist). One batch query per kind of metadata; a
    /// single thread (its own page, the API) takes one statement for all three.
    pub async fn metadata(&self, issues: &[IssueId]) -> Result<HashMap<IssueId, IssueMetadata>, ServiceError> {
        let store = self.ctx.database.issue_meta();
        if let [issue] = issues {
            let one = store.metadata_of(*issue).await?;
            let milestone = one.milestone.map(|milestone| MilestoneRef { id: milestone.id, title: milestone.title });
            return Ok(HashMap::from([(*issue, IssueMetadata { labels: one.labels, assignees: one.assignees, milestone })]));
        }
        let (labels, assignees, milestones) =
            tokio::try_join!(store.labels_of(issues), store.assignees_of(issues), store.milestones_of(issues))?;
        let mut map: HashMap<IssueId, IssueMetadata> = issues.iter().map(|id| (*id, IssueMetadata::default())).collect();
        for (issue, label) in labels {
            map.entry(issue).or_default().labels.push(label);
        }
        for (issue, account) in assignees {
            map.entry(issue).or_default().assignees.push(account);
        }
        for milestone in milestones {
            map.entry(milestone.issue).or_default().milestone = Some(MilestoneRef { id: milestone.id, title: milestone.title });
        }
        Ok(map)
    }

    /// [`Self::metadata`] limited to what `needs` names.
    async fn metadata_for(&self, issues: &[IssueId], needs: MetadataNeeds) -> Result<HashMap<IssueId, IssueMetadata>, ServiceError> {
        if needs == MetadataNeeds::All {
            return self.metadata(issues).await;
        }
        let labels = self.ctx.database.issue_meta().labels_of(issues).await?;
        let mut map: HashMap<IssueId, IssueMetadata> = issues.iter().map(|id| (*id, IssueMetadata::default())).collect();
        for (issue, label) in labels {
            map.entry(issue).or_default().labels.push(label);
        }
        Ok(map)
    }

    // ------------------------------------------------------------------ issues

    /// Deletes an issue (repository administrators only; pull requests cannot be
    /// deleted): audited, and `Deleted` is published with what it was.
    pub async fn delete(&self, actor: &User, ctx: &IssueContext) -> Result<(), ServiceError> {
        if !issue_permissions(Some(actor), ctx.repo.access, &ctx.issue).can_delete {
            return Err(ServiceError::Forbidden);
        }
        ctx.repo.ensure_writable()?;
        let Some(snapshot) = self.ctx.database.issues().delete(ctx.issue.id).await? else { return Err(ServiceError::NotFound) };
        let target = format!("{}#{}", ctx.repo.repo.full_name(), snapshot.number.get());
        self.audit.record(AuditEntry::new(AuditAction::IssueDeleted).by(actor).on(target).detail(snapshot.title.to_string())).await;
        self.publish(&snapshot.clone(), Some(actor.id()), IssueAction::Deleted { snapshot: Box::new(snapshot) });
        Ok(())
    }

    /// One page (1-based; clamped to the last) of `repo`'s threads matching `query` for
    /// `viewer`, with tab counts, authors and metadata.
    pub async fn search(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        self.search_with(viewer, repo, query, page, MetadataNeeds::All).await
    }

    /// [`Self::search`], reading only the metadata the page shows (`needs`); the rest of
    /// each entry of [`IssueSearchResults::metadata`] stays empty.
    pub async fn search_with(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        query: &IssueQuery,
        page: u32,
        needs: MetadataNeeds,
    ) -> Result<IssueSearchResults, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let (filter, hints) = self.search_filter(viewer, repo.repo.repo.id, query).await?;
        self.search_results(&filter, hints, page, needs).await
    }

    async fn search_filter(
        &self,
        viewer: Option<&User>,
        repo: RepoId,
        query: &IssueQuery,
    ) -> Result<(SearchFilter, Vec<String>), ServiceError> {
        let mut hints: Vec<String> = query.ignored.iter().map(|term| format!("“{term}” is not understood and was ignored")).collect();
        let mut filter = SearchFilter::new(repo);
        filter.kind = query.kind.map(|kind| match kind {
            KindFilter::Issue => IssueKind::Issue,
            KindFilter::Pull => IssueKind::Pull,
        });
        filter.state = query.state.map(|state| match state {
            StateFilter::Open => IssueState::Open,
            StateFilter::Closed => IssueState::Closed,
        });
        filter.merged = query.merged;
        filter.draft = query.draft;
        filter.locked = query.locked;
        filter.author = self.resolve_who(viewer, query.author.as_ref(), &mut hints).await?;
        filter.assignee = self.resolve_who(viewer, query.assignee.as_ref(), &mut hints).await?;
        filter.no_assignee = query.no_assignee;
        filter.labels.clone_from(&query.labels);
        filter.not_labels.clone_from(&query.not_labels);
        filter.no_label = query.no_label;
        filter.milestone.clone_from(&query.milestone);
        filter.no_milestone = query.no_milestone;
        filter.text.clone_from(&query.text);
        filter.sort = query.sort;

        Ok((filter, hints))
    }

    /// Personal threads across every repository the current active viewer can read.
    pub async fn dashboard(
        &self,
        viewer: &User,
        kind: IssueKind,
        relation: DashboardRelation,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        let viewer = self.ctx.database.accounts().find_active_user(viewer.id()).await?.ok_or(ServiceError::NotFound)?;
        let (mut filter, hints) = self.search_filter(Some(&viewer), RepoId(0), query).await?;
        filter.kind = Some(kind);
        filter.dashboard = Some(DashboardScope { viewer: viewer.id(), is_admin: viewer.is_admin, relation });
        let mut results = self.search_results(&filter, hints, page, MetadataNeeds::All).await?;
        // Each row's repository, access rechecked for the viewer just loaded: one batch
        // over the page's distinct repositories.
        let ids: Vec<RepoId> = distinct(&results.issues.iter().map(|issue| issue.repo_id).collect::<Vec<_>>());
        results.repositories = self.ctx.database.repositories().readable_among(&ids, viewer.id(), viewer.is_admin).await?;
        Ok(results)
    }

    async fn search_results(
        &self,
        filter: &SearchFilter,
        hints: Vec<String>,
        page: u32,
        needs: MetadataNeeds,
    ) -> Result<IssueSearchResults, ServiceError> {
        let store = self.ctx.database.issue_search();
        let (open, closed) = store.state_counts(filter).await?;
        let total = match filter.state {
            Some(IssueState::Open) => open,
            Some(IssueState::Closed) => closed,
            None => open + closed,
        };
        let mut results = IssueSearchResults {
            issues: Vec::new(),
            authors: HashMap::new(),
            metadata: HashMap::new(),
            total,
            open,
            closed,
            page: page.max(1),
            per_page: ISSUES_PER_PAGE,
            hints,
            repositories: HashMap::new(),
        };
        results.page = results.page.min(results.page_count());
        let offset = u64::from(results.page - 1) * u64::from(ISSUES_PER_PAGE);
        results.issues = store.search(filter, ISSUES_PER_PAGE, offset).await?;
        let ids: Vec<IssueId> = results.issues.iter().map(|issue| issue.id).collect();
        let author_ids: Vec<AccountId> = distinct(&results.issues.iter().filter_map(|issue| issue.author).collect::<Vec<_>>());
        let (authors, metadata) = tokio::try_join!(
            async { Ok::<_, ServiceError>(self.ctx.database.accounts().find_accounts(&author_ids).await?) },
            self.metadata_for(&ids, needs)
        )?;
        results.authors = authors.into_iter().map(|account| (account.id, account)).collect();
        results.metadata = metadata;
        Ok(results)
    }

    /// One page (1-based, not clamped) of `per_page` threads of `repo` matching `filter`
    /// (its `repo` is replaced by `repo`'s), with authors and metadata: the REST API's list.
    pub async fn list(
        &self,
        repo: &RepoContext,
        mut filter: SearchFilter,
        page: u32,
        per_page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        filter.repo = repo.repo.repo.id;
        filter.dashboard = None;
        let store = self.ctx.database.issue_search();
        let (open, closed) = store.state_counts(&filter).await?;
        let total = match filter.state {
            Some(IssueState::Open) => open,
            Some(IssueState::Closed) => closed,
            None => open + closed,
        };
        let per_page = per_page.max(1);
        let offset = u64::from(page.max(1) - 1) * u64::from(per_page);
        let issues = store.search(&filter, per_page, offset).await?;
        let ids: Vec<IssueId> = issues.iter().map(|issue| issue.id).collect();
        let author_ids: Vec<AccountId> = distinct(&issues.iter().filter_map(|issue| issue.author).collect::<Vec<_>>());
        let authors = self.ctx.database.accounts().find_accounts(&author_ids).await?;
        Ok(IssueSearchResults {
            metadata: self.metadata(&ids).await?,
            authors: authors.into_iter().map(|account| (account.id, account)).collect(),
            issues,
            total,
            open,
            closed,
            page: page.max(1),
            per_page,
            hints: Vec::new(),
            repositories: HashMap::new(),
        })
    }

    /// The users among `names` (case-insensitively); an unknown name is invalid.
    pub async fn users_named(&self, names: &[String]) -> Result<Vec<Account>, ServiceError> {
        let accounts = self.ctx.database.issue_meta().accounts_named(names).await?;
        if let Some(missing) =
            names.iter().find(|name| !accounts.iter().any(|a| !a.is_organization() && a.name.as_str().eq_ignore_ascii_case(name)))
        {
            return Err(invalid(format!("there is no user called “{missing}”")));
        }
        Ok(accounts.into_iter().filter(|a| !a.is_organization()).collect())
    }

    /// The number of the thread of `repo` that comment `id` belongs to.
    pub async fn comment_thread(&self, repo: &RepoContext, id: rbit_core::CommentId) -> Result<IssueNumber, ServiceError> {
        let found = self.ctx.database.issue_meta().comment_thread(repo.repo.repo.id, id).await?;
        found.map(|(_, number)| number).ok_or(ServiceError::NotFound)
    }

    /// The account id `who` names: unknown names match nothing (`AccountId(0)`), `@me`
    /// without a viewer is ignored with a hint.
    async fn resolve_who(
        &self,
        viewer: Option<&User>,
        who: Option<&Who>,
        hints: &mut Vec<String>,
    ) -> Result<Option<AccountId>, ServiceError> {
        Ok(match who {
            None => None,
            Some(Who::Me) => {
                if viewer.is_none() {
                    hints.push("Sign in to use @me".to_owned());
                }
                viewer.map(User::id)
            }
            Some(Who::Name(name)) => {
                Some(self.ctx.database.accounts().find_account_by_name(name).await?.map_or(AccountId(0), |account| account.id))
            }
        })
    }

    // ------------------------------------------------------------------ triage

    fn require_triage(actor: &User, ctx: &IssueContext) -> Result<(), ServiceError> {
        if !issue_permissions(Some(actor), ctx.repo.access, &ctx.issue).can_triage {
            return Err(ServiceError::Forbidden);
        }
        ctx.repo.ensure_writable()
    }

    /// The labels of `repo` with these ids; an id that is not one of them is invalid.
    async fn repo_labels(&self, repo: RepoId, ids: &[LabelId]) -> Result<Vec<Label>, ServiceError> {
        let mut labels = Vec::with_capacity(ids.len());
        let store = self.ctx.database.issue_meta();
        for id in ids {
            labels.push(store.find_label(repo, *id).await?.ok_or_else(|| invalid("unknown label"))?);
        }
        Ok(labels)
    }

    /// The users with these ids, each able to push to `repo`; anyone else is invalid.
    async fn validated_assignees(&self, repo: &OwnedRepository, ids: &[AccountId]) -> Result<Vec<User>, ServiceError> {
        let mut users = Vec::with_capacity(ids.len());
        for id in ids {
            let user = self.ctx.database.accounts().find_active_user(*id).await?.ok_or_else(|| invalid("unknown user"))?;
            if !self.access_of(&user, repo).await?.can_write() {
                return Err(invalid(format!("{} cannot be assigned: only people who can push to the repository can", user.name())));
            }
            users.push(user);
        }
        Ok(users)
    }

    /// Adds and removes labels (`add` wins over `remove` for the same label). Each real
    /// change is recorded and published once; at most 25 labels remain.
    pub async fn change_labels(&self, actor: &User, ctx: &IssueContext, add: &[LabelId], remove: &[LabelId]) -> Result<(), ServiceError> {
        Self::require_triage(actor, ctx)?;
        let repo = ctx.repo.repo.repo.id;
        let add = self.repo_labels(repo, &distinct(add)).await?;
        let remove: Vec<LabelId> = distinct(remove).into_iter().filter(|id| !add.iter().any(|l| l.id == *id)).collect();
        let store = self.ctx.database.issue_meta();
        let current = store.labels_of(&[ctx.issue.id]).await?;
        let kept = current.iter().filter(|(_, label)| !remove.contains(&label.id)).count();
        let added = add.iter().filter(|label| !current.iter().any(|(_, l)| l.id == label.id)).count();
        if kept + added > MAX_LABELS_PER_ISSUE {
            return Err(invalid(format!("an issue can have at most {MAX_LABELS_PER_ISSUE} labels")));
        }
        self.spend(actor)?;
        let now = Timestamp::now();
        for (_, label) in current.iter().filter(|(_, label)| remove.contains(&label.id)) {
            if store.remove_label(ctx.issue.id, label.id).await? {
                self.timeline.record(ctx.issue.id, Some(actor.id()), TimelineEvent::Unlabeled { label: label.clone() }).await?;
                self.publish(&ctx.issue, Some(actor.id()), IssueAction::Unlabeled { label: label.clone() });
            }
        }
        for label in &add {
            if store.add_label(ctx.issue.id, label.id, now).await? {
                let snapshot = label.snapshot();
                self.timeline.record(ctx.issue.id, Some(actor.id()), TimelineEvent::Labeled { label: snapshot.clone() }).await?;
                self.publish(&ctx.issue, Some(actor.id()), IssueAction::Labeled { label: snapshot });
            }
        }
        Ok(())
    }

    /// Makes the thread's labels exactly `labels`.
    pub async fn set_labels(&self, actor: &User, ctx: &IssueContext, labels: &[LabelId]) -> Result<(), ServiceError> {
        Self::require_triage(actor, ctx)?;
        let current = self.ctx.database.issue_meta().labels_of(&[ctx.issue.id]).await?;
        let remove: Vec<LabelId> = current.iter().map(|(_, l)| l.id).filter(|id| !labels.contains(id)).collect();
        self.change_labels(actor, ctx, labels, &remove).await
    }

    /// Assigns and unassigns (`add` wins over `remove`). Assignees must be able to push;
    /// at most 10 remain.
    pub async fn change_assignees(
        &self,
        actor: &User,
        ctx: &IssueContext,
        add: &[AccountId],
        remove: &[AccountId],
    ) -> Result<(), ServiceError> {
        Self::require_triage(actor, ctx)?;
        let add_ids = distinct(add);
        let remove: Vec<AccountId> = distinct(remove).into_iter().filter(|id| !add_ids.contains(id)).collect();
        let store = self.ctx.database.issue_meta();
        let current: Vec<AccountId> = store.assignees_of(&[ctx.issue.id]).await?.into_iter().map(|(_, a)| a.id).collect();
        let new: Vec<AccountId> = add_ids.iter().copied().filter(|id| !current.contains(id)).collect();
        let users = self.validated_assignees(&ctx.repo.repo, &new).await?;
        let kept = current.iter().filter(|id| !remove.contains(id)).count();
        if kept + users.len() > MAX_ASSIGNEES {
            return Err(invalid(format!("an issue can have at most {MAX_ASSIGNEES} assignees")));
        }
        self.spend(actor)?;
        let now = Timestamp::now();
        for id in current.iter().filter(|id| remove.contains(id)) {
            if store.remove_assignee(ctx.issue.id, *id).await? {
                self.timeline.record(ctx.issue.id, Some(actor.id()), TimelineEvent::Unassigned { assignee: *id }).await?;
                self.publish(&ctx.issue, Some(actor.id()), IssueAction::Unassigned { assignee: *id });
            }
        }
        for user in &users {
            if store.add_assignee(ctx.issue.id, user.id(), actor.id(), now).await? {
                self.timeline.record(ctx.issue.id, Some(actor.id()), TimelineEvent::Assigned { assignee: user.id() }).await?;
                self.publish(&ctx.issue, Some(actor.id()), IssueAction::Assigned { assignee: user.id() });
            }
        }
        Ok(())
    }

    /// Makes the thread's assignees exactly `assignees`.
    pub async fn set_assignees(&self, actor: &User, ctx: &IssueContext, assignees: &[AccountId]) -> Result<(), ServiceError> {
        Self::require_triage(actor, ctx)?;
        let current = self.ctx.database.issue_meta().assignees_of(&[ctx.issue.id]).await?;
        let remove: Vec<AccountId> = current.iter().map(|(_, a)| a.id).filter(|id| !assignees.contains(id)).collect();
        self.change_assignees(actor, ctx, assignees, &remove).await
    }

    /// People who may be assigned in `repo`: names starting with `prefix`, at most 100.
    pub async fn assignable(&self, repo: &RepoContext, prefix: &str) -> Result<Vec<Account>, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        Ok(self.ctx.database.issue_meta().assignable(repo.repo.repo.id, prefix.trim(), MAX_ASSIGNABLE).await?)
    }

    /// Puts the thread in `milestone` (`None`: in none). A change records `Demilestoned`
    /// for the old one and `Milestoned` for the new one.
    pub async fn set_milestone(&self, actor: &User, ctx: &IssueContext, milestone: Option<MilestoneId>) -> Result<(), ServiceError> {
        Self::require_triage(actor, ctx)?;
        let store = self.ctx.database.issue_meta();
        let new = match milestone {
            Some(id) => Some(store.find_milestone(ctx.repo.repo.repo.id, id).await?.ok_or_else(|| invalid("unknown milestone"))?),
            None => None,
        };
        let current = store.milestones_of(&[ctx.issue.id]).await?.pop();
        if current.as_ref().map(|m| m.id) == new.as_ref().map(|m| m.id) {
            return Ok(());
        }
        self.spend(actor)?;
        store.set_milestone(ctx.issue.id, new.as_ref().map(|m| m.id)).await?;
        if let Some(old) = current {
            let event = TimelineEvent::Demilestoned { milestone: old.id, title: old.title.clone() };
            self.timeline.record(ctx.issue.id, Some(actor.id()), event).await?;
            self.publish(&ctx.issue, Some(actor.id()), IssueAction::Demilestoned { milestone: old.id, title: old.title });
        }
        if let Some(new) = new {
            let event = TimelineEvent::Milestoned { milestone: new.id, title: new.title.clone() };
            self.timeline.record(ctx.issue.id, Some(actor.id()), event).await?;
            self.publish(&ctx.issue, Some(actor.id()), IssueAction::Milestoned { milestone: new.id });
        }
        Ok(())
    }

    // ------------------------------------------------------------------ references in the background

    /// Starts this service's background work (cross-references, commit references and
    /// closing keywords). Called once from [`crate::Services::spawn_background`].
    pub fn spawn_background(&self) {
        if self.background.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let events = self.ctx.events.subscribe();
        let service = self.clone();
        self.background.tasks.spawn(async move { service.listen(events).await });
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.background.shutdown.cancel();
        self.background.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.background.tasks.wait()).await.is_err() {
            tracing::warn!("issue references did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn listen(self, mut events: tokio::sync::broadcast::Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.background.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(event) => {
                    if let Err(error) = self.handle_event(&event).await {
                        tracing::warn!(%error, ?event, "cannot record issue references");
                    }
                }
                Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "issue reference subscriber fell behind"),
                Err(RecvError::Closed) => break,
            }
        }
    }

    async fn record_mentions(&self, issue: IssueId, text: &str) -> Result<(), ServiceError> {
        let names = references::scan_markdown(text).mentions;
        if !names.is_empty() {
            let accounts = self.ctx.database.issue_meta().accounts_named(&names).await?;
            let ids: Vec<_> = accounts.iter().filter(|account| !account.is_organization()).map(|account| account.id).collect();
            self.ctx.database.issue_search().record_mentions(issue, &ids).await?;
        }
        Ok(())
    }

    /// One-time Markdown-aware upgrade of existing issue, comment and submitted review bodies.
    pub(crate) async fn index_existing_mentions(&self) -> Result<(), ServiceError> {
        let store = self.ctx.database.issue_search();
        if store.mentions_indexed().await? {
            return Ok(());
        }
        for source in 0..4 {
            let mut after = 0;
            loop {
                let rows = store.mention_sources(source, after).await?;
                if rows.is_empty() {
                    break;
                }
                for (id, issue, body) in rows {
                    self.record_mentions(IssueId(issue), &body).await?;
                    after = id;
                }
            }
        }
        store.finish_mentions_index().await?;
        Ok(())
    }

    /// Reacts to one event: what the background subscriber does, callable directly (tests).
    pub async fn handle_event(&self, event: &DomainEvent) -> Result<(), ServiceError> {
        // Events can be processed after a later edit/deletion. Preserve mentions from
        // the event's historical content before reading the latest stored body.
        match event {
            DomainEvent::Issue { issue, action: IssueAction::Edited { previous_body: Some(body), .. }, .. }
            | DomainEvent::IssueComment { issue, action: rbit_core::issue::CommentAction::Edited { previous_body: body }, .. } => {
                self.record_mentions(*issue, body).await?;
            }
            DomainEvent::IssueComment { issue, action: rbit_core::issue::CommentAction::Deleted { snapshot }, .. } => {
                self.record_mentions(*issue, snapshot.body.as_str()).await?;
            }
            _ => {}
        }
        match event {
            DomainEvent::PullRequestReview { pull, review, action, .. } => {
                let store = self.ctx.database.reviews();
                if let Some(row) = store.find(*pull, *review).await?
                    && row.state != rbit_core::issue::ReviewState::Pending
                {
                    if let rbit_core::issue::ReviewAction::Edited { previous_body } = action {
                        self.record_mentions(*pull, previous_body).await?;
                    }
                    self.record_mentions(*pull, row.body.as_str()).await?;
                    for comment in store.comments(*pull).await?.iter().filter(|comment| comment.review_id == *review) {
                        self.record_mentions(*pull, comment.body.as_str()).await?;
                    }
                }
                Ok(())
            }
            DomainEvent::RepositoryPushed { repo, source, updates } => self.scan_push(*repo, source.pusher(), updates).await,
            DomainEvent::Issue { issue, actor, action: IssueAction::Opened | IssueAction::Edited { .. }, .. } => {
                let Some(source) = self.ctx.database.issues().find(*issue).await? else { return Ok(()) };
                let body = source.body.to_string();
                self.record_mentions(*issue, &body).await?;
                self.references.record_cross_references(&source, *actor, &body).await
            }
            DomainEvent::IssueComment {
                issue,
                comment,
                actor,
                action: rbit_core::issue::CommentAction::Created | rbit_core::issue::CommentAction::Edited { .. },
                ..
            } => {
                let store = self.ctx.database.issues();
                let (Some(source), Some(comment)) = (store.find(*issue).await?, store.find_comment(*issue, *comment).await?) else {
                    return Ok(());
                };
                self.record_mentions(*issue, comment.body.as_str()).await?;
                // Actions: task tokens. rbit Actions has no account a cross-reference could
                // show; the referenced thread would name a ghost.
                match actor {
                    Some(_) => self.references.record_cross_references(&source, *actor, comment.body.as_str()).await,
                    None => Ok(()),
                }
            }
            DomainEvent::Issue {
                kind: IssueKind::Pull,
                issue,
                number,
                actor,
                action: IssueAction::Closed { reason: StateReason::Merged },
                ..
            } => self.close_by_merge(*issue, *number, *actor).await,
            _ => Ok(()),
        }
    }

    /// Commits pushed to `repo`: `#N` references recorded once per (issue, commit); closing
    /// keywords close issues when the branch is the default branch. Mirrors are skipped.
    async fn scan_push(&self, repo: RepoId, pusher: Option<AccountId>, updates: &[RefUpdate]) -> Result<(), ServiceError> {
        let database = &self.ctx.database;
        if updates.is_empty() || database.mirrors().is_mirror(repo).await? {
            return Ok(());
        }
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(()) };
        let reader = self.ctx.repositories.reader(owned.repo.owner_id, owned.repo.id);
        let snapshot = reader.ref_snapshot().await.ok();
        let limit = self.ctx.config.issues.scan_commit_limit;
        for update in updates {
            let (Some(branch), Some(after)) = (update.branch(), &update.after) else { continue };
            let excluding = match (&update.before, &snapshot) {
                (Some(before), _) => vec![before.clone()],
                (None, Some(snapshot)) => {
                    let mut skipped = false;
                    snapshot
                        .ids()
                        .filter(|tip| {
                            if !skipped && *tip == after.as_str() {
                                skipped = true;
                                return false;
                            }
                            true
                        })
                        .map(str::to_owned)
                        .collect()
                }
                (None, None) => Vec::new(),
            };
            let commits = match reader.new_commits(after.clone(), excluding, limit).await {
                Ok(commits) => commits,
                Err(error) => {
                    tracing::warn!(%error, repo = %owned.full_name(), branch, "cannot list pushed commits for references");
                    continue;
                }
            };
            let default_branch = branch == owned.repo.default_branch;
            // Oldest first, so the timeline reads in commit order.
            for commit in commits.iter().rev() {
                let message = match &commit.body {
                    Some(body) => format!("{}\n\n{body}", commit.summary),
                    None => commit.summary.clone(),
                };
                let scan = references::scan_plain(&message);
                if scan.local.is_empty() {
                    continue;
                }
                let targets = database.issues().find_numbers(repo, &scan.local).await?;
                for target in targets {
                    if database.issue_meta().record_commit_ref(target.id, &commit.id, Timestamp::now()).await? {
                        let event = TimelineEvent::CommitReferenced { commit: commit.id.clone(), summary: commit.summary.clone() };
                        self.timeline.record(target.id, pusher, event).await?;
                    }
                    if default_branch && target.kind == IssueKind::Issue && scan.closes.contains(&target.number) {
                        let event = TimelineEvent::Closed { reason: StateReason::Completed, commit: Some(commit.id.clone()), pull: None };
                        self.timeline.close_as_system(target.id, pusher, StateReason::Completed, event).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// A pull request merged into the default branch closes the issues its body names
    /// with closing keywords (`pulls::lifecycle::merged_into_default_branch`); merges into
    /// other branches close nothing, as on GitHub.
    async fn close_by_merge(&self, pull: IssueId, number: IssueNumber, actor: Option<AccountId>) -> Result<(), ServiceError> {
        if !crate::pulls::lifecycle::merged_into_default_branch(&self.ctx, pull).await? {
            return Ok(());
        }
        let database = &self.ctx.database;
        let Some(pull) = database.issues().find(pull).await? else { return Ok(()) };
        let scan = references::scan_markdown(pull.body.as_str());
        if scan.closes.is_empty() {
            return Ok(());
        }
        for target in database.issues().find_numbers(pull.repo_id, &scan.closes).await? {
            if target.kind == IssueKind::Issue {
                let event = TimelineEvent::Closed { reason: StateReason::Completed, commit: None, pull: Some(number) };
                self.timeline.close_as_system(target.id, actor, StateReason::Completed, event).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_normalised_strictly() {
        assert_eq!(normalise_color("#D73A4A").unwrap(), "#d73a4a");
        assert_eq!(normalise_color(" 0075ca ").unwrap(), "#0075ca");
        for bad in ["", "#fff", "red", "#1234567", "#12345g", "##123456"] {
            assert!(normalise_color(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn calendar_days() {
        for good in ["2026-10-16", "2024-02-29", "2000-02-29", "0001-01-01"] {
            assert!(is_calendar_day(good), "{good}");
        }
        for bad in [
            "2026-02-29",
            "1900-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-04-31",
            "2026-1-1",
            "26-10-16",
            "2026/10/16",
            "0000-01-01",
            "2026-10-1x",
        ] {
            assert!(!is_calendar_day(bad), "{bad}");
        }
    }

    #[test]
    fn label_and_milestone_fields_are_validated() {
        let ok = label_fields(&LabelInput { name: " bug ".into(), color: "D73A4A".into(), description: " Broken ".into() }).unwrap();
        assert_eq!((ok.name.as_str(), ok.color.as_str(), ok.description.as_str()), ("bug", "#d73a4a", "Broken"));
        assert!(label_fields(&LabelInput { name: " ".into(), color: "#000000".into(), description: String::new() }).is_err());
        assert!(label_fields(&LabelInput { name: "x".repeat(51), color: "#000000".into(), description: String::new() }).is_err());
        assert!(label_fields(&LabelInput { name: "x".into(), color: "#000000".into(), description: "d".repeat(101) }).is_err());
        assert!(label_fields(&LabelInput { name: "a\u{7}".into(), color: "#000000".into(), description: String::new() }).is_err());

        let fields =
            milestone_fields(&MilestoneInput { title: " v1 ".into(), description: "a\r\nb".into(), due_on: String::new() }).unwrap();
        assert_eq!((fields.title.as_str(), fields.description.as_str(), fields.due_on), ("v1", "a\nb", None));
        assert!(milestone_fields(&MilestoneInput { title: "v1".into(), due_on: "tomorrow".into(), ..MilestoneInput::default() }).is_err());
        assert!(milestone_fields(&MilestoneInput { title: "x".repeat(101), ..MilestoneInput::default() }).is_err());
        let long = "x".repeat(MAX_MILESTONE_DESCRIPTION_BYTES + 1);
        assert!(milestone_fields(&MilestoneInput { title: "v".into(), description: long, ..MilestoneInput::default() }).is_err());
    }

    #[test]
    fn default_labels_are_valid() {
        for (name, color, description) in DEFAULT_LABELS {
            let input = LabelInput { name: name.into(), color: color.into(), description: description.into() };
            assert_eq!(label_fields(&input).unwrap().color, color);
        }
    }
}
