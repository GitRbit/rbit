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

pub mod hooks;
pub mod json;

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
use rbit_storage::sqlite::issue_meta::{LabelFields, MilestoneFields};
pub use rbit_storage::sqlite::{
    issue_meta::{Label, LabelUsage, Milestone, MilestoneProgress},
    issue_search::SearchFilter,
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

/// Most labels in one repository.
pub const MAX_LABELS_PER_REPO: u64 = 200;
/// Most labels on one thread.
pub const MAX_LABELS_PER_ISSUE: usize = 25;
/// Most assignees of one thread.
pub const MAX_ASSIGNEES: usize = 10;
/// Most users [`IssueService::assignable`] lists.
pub const MAX_ASSIGNABLE: u32 = 100;
/// Threads per page of a list.
pub const ISSUES_PER_PAGE: u32 = 25;
/// Longest label name, in characters.
pub const MAX_LABEL_NAME_CHARS: usize = 50;
/// Longest label description, in characters.
pub const MAX_LABEL_DESCRIPTION_CHARS: usize = 100;
/// Longest milestone title, in characters.
pub const MAX_MILESTONE_TITLE_CHARS: usize = 100;
/// Longest milestone description, in bytes.
pub const MAX_MILESTONE_DESCRIPTION_BYTES: usize = 10_000;

/// How long shutdown waits for the subscriber.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// The labels "Create default labels" adds: name, colour, description.
pub const DEFAULT_LABELS: [(&str, &str, &str); 9] = [
    ("bug", "#d73a4a", "Something does not work as intended"),
    ("documentation", "#0075ca", "Docs need writing or fixing"),
    ("duplicate", "#cfd3d7", "Already reported elsewhere"),
    ("enhancement", "#a2eeef", "A new feature or an improvement"),
    ("good first issue", "#7057ff", "A gentle start for newcomers"),
    ("help wanted", "#008672", "Extra hands are welcome"),
    ("invalid", "#e4e669", "Not a real problem"),
    ("question", "#d876e3", "More information is needed"),
    ("wontfix", "#ffffff", "Will not be worked on"),
];

/// Labels, assignees and milestone of one issue or pull request, for lists, payloads and
/// the API.
#[derive(Debug, Clone, Default)]
pub struct IssueMetadata {
    pub labels: Vec<LabelSnapshot>,
    pub assignees: Vec<Account>,
    pub milestone: Option<MilestoneRef>,
}

/// The milestone an issue belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneRef {
    pub id: MilestoneId,
    pub title: String,
}

/// A new issue as submitted. Labels, assignees and the milestone are applied only when
/// the author may triage (they are ignored otherwise, as on GitHub).
#[derive(Debug, Clone, Default)]
pub struct NewIssueInput {
    pub title: String,
    pub body: String,
    pub labels: Vec<LabelId>,
    pub assignees: Vec<AccountId>,
    pub milestone: Option<MilestoneId>,
}

/// A label as submitted (colour with or without `#`, any case).
#[derive(Debug, Clone, Default)]
pub struct LabelInput {
    pub name: String,
    pub color: String,
    pub description: String,
}

/// A milestone as submitted (`due_on`: `YYYY-MM-DD` or empty).
#[derive(Debug, Clone, Default)]
pub struct MilestoneInput {
    pub title: String,
    pub description: String,
    pub due_on: String,
}

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
    let trimmed = color.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("colour must be six hexadecimal digits, like #d73a4a"));
    }
    Ok(format!("#{}", hex.to_ascii_lowercase()))
}

fn label_fields(input: &LabelInput) -> Result<LabelFields, ServiceError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("label name must not be empty"));
    }
    if name.chars().count() > MAX_LABEL_NAME_CHARS {
        return Err(invalid(format!("label name must be at most {MAX_LABEL_NAME_CHARS} characters")));
    }
    if name.chars().any(char::is_control) {
        return Err(invalid("label name must not contain control characters"));
    }
    let description = input.description.trim();
    if description.chars().count() > MAX_LABEL_DESCRIPTION_CHARS {
        return Err(invalid(format!("label description must be at most {MAX_LABEL_DESCRIPTION_CHARS} characters")));
    }
    if description.chars().any(char::is_control) {
        return Err(invalid("label description must not contain control characters"));
    }
    Ok(LabelFields { name: name.to_owned(), color: normalise_color(&input.color)?, description: description.to_owned() })
}

/// Whether `value` is a real calendar day written `YYYY-MM-DD`.
pub fn is_calendar_day(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| value.get(range).filter(|s| s.bytes().all(|b| b.is_ascii_digit()))?.parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else { return false };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=days).contains(&day)
}

fn milestone_fields(input: &MilestoneInput) -> Result<MilestoneFields, ServiceError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("milestone title must not be empty"));
    }
    if title.chars().count() > MAX_MILESTONE_TITLE_CHARS {
        return Err(invalid(format!("milestone title must be at most {MAX_MILESTONE_TITLE_CHARS} characters")));
    }
    if title.chars().any(char::is_control) {
        return Err(invalid("milestone title must not contain control characters"));
    }
    let description = input.description.replace("\r\n", "\n");
    if description.len() > MAX_MILESTONE_DESCRIPTION_BYTES {
        return Err(invalid(format!("milestone description must be at most {MAX_MILESTONE_DESCRIPTION_BYTES} bytes")));
    }
    let due_on = input.due_on.trim();
    let due_on = if due_on.is_empty() {
        None
    } else if is_calendar_day(due_on) {
        Some(due_on.to_owned())
    } else {
        return Err(invalid("due date must be a day written YYYY-MM-DD"));
    };
    Ok(MilestoneFields { title: title.to_owned(), description, due_on })
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

    /// Labels, assignees and milestone of each of `issues` (missing ids are left out of
    /// nothing: every id gets an entry, empty when it has no metadata). One batch query
    /// per kind of metadata.
    pub async fn metadata(&self, issues: &[IssueId]) -> Result<HashMap<IssueId, IssueMetadata>, ServiceError> {
        let store = self.ctx.database.issue_meta();
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

    // ------------------------------------------------------------------ issues

    /// Opens an issue by `actor` in `repo` (which they can read). Writers' labels,
    /// assignees and milestone are validated first, attached in one transaction after the
    /// row and recorded on the timeline; then `Opened` is published once.
    pub async fn create(&self, actor: &User, repo: &RepoContext, input: NewIssueInput) -> Result<Issue, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let triage = repo.access.can_write();
        let (labels, assignees, milestone) = if triage {
            let labels = self.repo_labels(repo.repo.repo.id, &distinct(&input.labels)).await?;
            if labels.len() > MAX_LABELS_PER_ISSUE {
                return Err(invalid(format!("an issue can have at most {MAX_LABELS_PER_ISSUE} labels")));
            }
            let assignees = self.validated_assignees(&repo.repo, &distinct(&input.assignees)).await?;
            if assignees.len() > MAX_ASSIGNEES {
                return Err(invalid(format!("an issue can have at most {MAX_ASSIGNEES} assignees")));
            }
            let milestone = match input.milestone {
                Some(id) => Some(
                    self.ctx
                        .database
                        .issue_meta()
                        .find_milestone(repo.repo.repo.id, id)
                        .await?
                        .ok_or_else(|| invalid("unknown milestone"))?,
                ),
                None => None,
            };
            (labels, assignees, milestone)
        } else {
            (Vec::new(), Vec::new(), None)
        };

        let issue = self.timeline.create(actor, repo, IssueKind::Issue, &input.title, &input.body).await?;
        if !labels.is_empty() || !assignees.is_empty() || milestone.is_some() {
            let label_ids: Vec<LabelId> = labels.iter().map(|l| l.id).collect();
            let assignee_ids: Vec<AccountId> = assignees.iter().map(User::id).collect();
            self.ctx
                .database
                .issue_meta()
                .attach(issue.id, &label_ids, &assignee_ids, milestone.as_ref().map(|m| m.id), actor.id(), Timestamp::now())
                .await?;
            for label in &labels {
                self.timeline.record(issue.id, Some(actor.id()), TimelineEvent::Labeled { label: label.snapshot() }).await?;
            }
            for assignee in &assignees {
                self.timeline.record(issue.id, Some(actor.id()), TimelineEvent::Assigned { assignee: assignee.id() }).await?;
            }
            if let Some(milestone) = &milestone {
                let event = TimelineEvent::Milestoned { milestone: milestone.id, title: milestone.title.clone() };
                self.timeline.record(issue.id, Some(actor.id()), event).await?;
            }
        }
        self.publish(&issue, Some(actor.id()), IssueAction::Opened);
        Ok(issue)
    }

    /// Deletes an issue (repository administrators only; pull requests cannot be
    /// deleted): audited, and `Deleted` is published with what it was.
    pub async fn delete(&self, actor: &User, ctx: &IssueContext) -> Result<(), ServiceError> {
        if !issue_permissions(Some(actor), ctx.repo.access, &ctx.issue).can_delete {
            return Err(ServiceError::Forbidden);
        }
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
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let mut hints: Vec<String> = query.ignored.iter().map(|term| format!("“{term}” is not understood and was ignored")).collect();
        let mut filter = SearchFilter::new(repo.repo.repo.id);
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

        let store = self.ctx.database.issue_search();
        let (open, closed) = store.state_counts(&filter).await?;
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
        };
        results.page = results.page.min(results.page_count());
        let offset = u64::from(results.page - 1) * u64::from(ISSUES_PER_PAGE);
        results.issues = store.search(&filter, ISSUES_PER_PAGE, offset).await?;
        let ids: Vec<IssueId> = results.issues.iter().map(|issue| issue.id).collect();
        let author_ids: Vec<AccountId> = distinct(&results.issues.iter().filter_map(|issue| issue.author).collect::<Vec<_>>());
        let (authors, metadata) = tokio::try_join!(
            async { Ok::<_, ServiceError>(self.ctx.database.accounts().find_accounts(&author_ids).await?) },
            self.metadata(&ids)
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
        if issue_permissions(Some(actor), ctx.repo.access, &ctx.issue).can_triage { Ok(()) } else { Err(ServiceError::Forbidden) }
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

    // ------------------------------------------------------------------ labels

    fn require_writer(repo: &RepoContext) -> Result<(), ServiceError> {
        if repo.access.can_write() { Ok(()) } else { Err(ServiceError::Forbidden) }
    }

    /// The labels of `repo`, by name.
    pub async fn labels(&self, repo: &RepoContext) -> Result<Vec<Label>, ServiceError> {
        Ok(self.ctx.database.issue_meta().labels(repo.repo.repo.id).await?)
    }

    /// The labels of `repo` with their open issues.
    pub async fn label_usage(&self, repo: &RepoContext) -> Result<Vec<LabelUsage>, ServiceError> {
        Ok(self.ctx.database.issue_meta().label_usage(repo.repo.repo.id, IssueKind::Issue.as_str()).await?)
    }

    /// The label of `repo` called `name` (case-insensitively).
    pub async fn label_named(&self, repo: &RepoContext, name: &str) -> Result<Label, ServiceError> {
        self.ctx.database.issue_meta().find_label_by_name(repo.repo.repo.id, name).await?.ok_or(ServiceError::NotFound)
    }

    /// The labels of `repo` among `names`; unknown names are invalid.
    pub async fn labels_named(&self, repo: &RepoContext, names: &[String]) -> Result<Vec<Label>, ServiceError> {
        let labels = self.ctx.database.issue_meta().labels_named(repo.repo.repo.id, names).await?;
        if let Some(missing) = names.iter().find(|name| !labels.iter().any(|label| label.name.eq_ignore_ascii_case(name))) {
            return Err(invalid(format!("there is no label called “{missing}”")));
        }
        Ok(labels)
    }

    /// Creates a label (writers; at most 200 per repository; names unique ignoring case).
    pub async fn create_label(&self, actor: &User, repo: &RepoContext, input: &LabelInput) -> Result<Label, ServiceError> {
        Self::require_writer(repo)?;
        let fields = label_fields(input)?;
        let store = self.ctx.database.issue_meta();
        if store.count_labels(repo.repo.repo.id).await? >= MAX_LABELS_PER_REPO {
            return Err(invalid(format!("a repository can have at most {MAX_LABELS_PER_REPO} labels")));
        }
        self.spend(actor)?;
        Ok(store.create_label(repo.repo.repo.id, &fields, Timestamp::now()).await?)
    }

    /// Changes label `id` of `repo` (writers).
    pub async fn update_label(&self, actor: &User, repo: &RepoContext, id: LabelId, input: &LabelInput) -> Result<Label, ServiceError> {
        Self::require_writer(repo)?;
        let fields = label_fields(input)?;
        self.spend(actor)?;
        self.ctx.database.issue_meta().update_label(repo.repo.repo.id, id, &fields, Timestamp::now()).await?.ok_or(ServiceError::NotFound)
    }

    /// Deletes label `id` of `repo` (writers): it disappears from every thread, without
    /// timeline events.
    pub async fn delete_label(&self, repo: &RepoContext, id: LabelId) -> Result<Label, ServiceError> {
        Self::require_writer(repo)?;
        self.ctx.database.issue_meta().delete_label(repo.repo.repo.id, id).await?.ok_or(ServiceError::NotFound)
    }

    /// Adds [`DEFAULT_LABELS`] when `repo` has no labels at all; returns how many were added.
    pub async fn create_default_labels(&self, actor: &User, repo: &RepoContext) -> Result<u64, ServiceError> {
        Self::require_writer(repo)?;
        let store = self.ctx.database.issue_meta();
        if store.count_labels(repo.repo.repo.id).await? > 0 {
            return Ok(0);
        }
        self.spend(actor)?;
        let labels: Vec<LabelFields> = DEFAULT_LABELS
            .iter()
            .map(|(name, color, description)| LabelFields {
                name: (*name).to_owned(),
                color: (*color).to_owned(),
                description: (*description).to_owned(),
            })
            .collect();
        Ok(store.create_missing_labels(repo.repo.repo.id, &labels, Timestamp::now()).await?)
    }

    // ------------------------------------------------------------------ milestones

    /// The milestones of `repo` (in `state` if given) with their progress.
    pub async fn milestones(&self, repo: &RepoContext, state: Option<IssueState>) -> Result<Vec<MilestoneProgress>, ServiceError> {
        Ok(self.ctx.database.issue_meta().milestones(repo.repo.repo.id, state).await?)
    }

    /// How many milestones of `repo` are open and closed.
    pub async fn milestone_counts(&self, repo: &RepoContext) -> Result<(u64, u64), ServiceError> {
        Ok(self.ctx.database.issue_meta().milestone_counts(repo.repo.repo.id).await?)
    }

    /// Milestone `id` of `repo` with its progress.
    pub async fn milestone(&self, repo: &RepoContext, id: MilestoneId) -> Result<MilestoneProgress, ServiceError> {
        let all = self.ctx.database.issue_meta().milestones(repo.repo.repo.id, None).await?;
        all.into_iter().find(|progress| progress.milestone.id == id).ok_or(ServiceError::NotFound)
    }

    /// The milestone of `repo` titled `title` (case-insensitively).
    pub async fn milestone_titled(&self, repo: &RepoContext, title: &str) -> Result<Milestone, ServiceError> {
        self.ctx.database.issue_meta().find_milestone_by_title(repo.repo.repo.id, title).await?.ok_or(ServiceError::NotFound)
    }

    /// Creates an open milestone (writers).
    pub async fn create_milestone(&self, actor: &User, repo: &RepoContext, input: &MilestoneInput) -> Result<Milestone, ServiceError> {
        Self::require_writer(repo)?;
        let fields = milestone_fields(input)?;
        self.spend(actor)?;
        Ok(self.ctx.database.issue_meta().create_milestone(repo.repo.repo.id, &fields, Timestamp::now()).await?)
    }

    /// Changes milestone `id` of `repo` (writers).
    pub async fn update_milestone(
        &self,
        actor: &User,
        repo: &RepoContext,
        id: MilestoneId,
        input: &MilestoneInput,
    ) -> Result<Milestone, ServiceError> {
        Self::require_writer(repo)?;
        let fields = milestone_fields(input)?;
        self.spend(actor)?;
        self.ctx
            .database
            .issue_meta()
            .update_milestone(repo.repo.repo.id, id, &fields, Timestamp::now())
            .await?
            .ok_or(ServiceError::NotFound)
    }

    /// Closes or reopens milestone `id` of `repo` (writers).
    pub async fn set_milestone_state(&self, repo: &RepoContext, id: MilestoneId, state: IssueState) -> Result<Milestone, ServiceError> {
        Self::require_writer(repo)?;
        self.ctx
            .database
            .issue_meta()
            .set_milestone_state(repo.repo.repo.id, id, state, Timestamp::now())
            .await?
            .ok_or(ServiceError::NotFound)
    }

    /// Deletes milestone `id` of `repo` (writers); its threads keep no milestone.
    pub async fn delete_milestone(&self, repo: &RepoContext, id: MilestoneId) -> Result<Milestone, ServiceError> {
        Self::require_writer(repo)?;
        self.ctx.database.issue_meta().delete_milestone(repo.repo.repo.id, id).await?.ok_or(ServiceError::NotFound)
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

    /// Reacts to one event: what the background subscriber does, callable directly (tests).
    pub async fn handle_event(&self, event: &DomainEvent) -> Result<(), ServiceError> {
        match event {
            DomainEvent::RepositoryPushed { repo, pusher, updates } => self.scan_push(*repo, *pusher, updates).await,
            DomainEvent::Issue { issue, actor, action: IssueAction::Opened | IssueAction::Edited { .. }, .. } => {
                let Some(source) = self.ctx.database.issues().find(*issue).await? else { return Ok(()) };
                let body = source.body.to_string();
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
                self.references.record_cross_references(&source, Some(*actor), comment.body.as_str()).await
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

    /// A merged pull request closes the issues its body names with closing keywords.
    ///
    /// The base branch is the pull request workstream's data; until it exposes it, a
    /// merge into any branch closes (see the integration notes).
    async fn close_by_merge(&self, pull: IssueId, number: IssueNumber, actor: Option<AccountId>) -> Result<(), ServiceError> {
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
