//! Pull requests: creation across a fork network, `refs/pull/{n}/head`, head and base
//! updates, mergeability, reviews and merging on a temporary repository.
//!
//! Milestone 2 workstream P (pull requests) owns this file and `pulls/**`.
//!
//! Hooks other code calls (signatures fixed by `docs/design/milestone-2b.md` §3):
//! [`hooks::payload`] (webhooks) and [`lifecycle::check_reopen`] (the timeline's reopen).
//!
//! # Pieces
//!
//! * [`network`](PullService::network) and [`compare`](PullService::compare) — the fork
//!   network and cross-repository comparisons through an object overlay
//!   (`rbit_git::overlay`), so nothing is written before a pull request exists;
//! * [`create`](PullService::create) — the thread and the pull request in one
//!   transaction, then `refs/pull/{n}/head` in the base repository;
//! * `pulls/background.rs` — head and base pushes (`RepositoryPushed`), the mergeability
//!   worker and the start-up sweep of temporary directories;
//! * `pulls/reviews.rs` — line comments, reviews, dismissals and review requests;
//! * `pulls/merging.rs` — the merge box and merging (`rbit_git::merge`), serialised per
//!   base repository and bounded instance-wide;
//! * [`json`] and [`hooks`] — GitHub-shaped objects and the `pull_request` and
//!   `pull_request_review` payloads.
//!
//! Comments, the title, the description, closing, reopening and locking go through the
//! shared [`crate::TimelineService`]; this service never writes the issue tables itself
//! except through `PullStore::create` (which uses `IssueStore::insert_in`).

mod background;
pub mod hooks;
pub mod json;
pub mod lifecycle;
mod merging;
mod reviews;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError, atomic::AtomicBool},
};

use rbit_auth::repo_access;
use rbit_core::{
    Account, AccountId, Issue, IssueKind, IssueNumber, RepoAccess, RepoId, Timestamp, User,
    issue::IssueAction,
    pull::{HeadSpec, PullRequest},
    repository::OwnedRepository,
};
use rbit_git::{CommitSummary, DiffLimits, compare::Comparison, overlay::Overlay, pull_refs};
use rbit_storage::kv::CachedComparisonCounts;
use rbit_storage::sqlite::{
    issues::NewIssue,
    pulls::{CreatedPull, NewPull},
};
use tokio::sync::{Notify, Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub use rbit_storage::sqlite::pulls::{PullFilter, PullSort, PullStateFilter};

/// The file limit of the diffs whose counts are cached: the default one.
fn counted_max_files() -> u64 {
    u64::try_from(DiffLimits::default().max_files).unwrap_or(u64::MAX)
}

/// What a pull request's header counts: commits ahead and changed files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PullCounts {
    pub ahead: usize,
    pub files: usize,
}

impl PullCounts {
    pub fn of(comparison: &Comparison) -> Self {
        Self { ahead: comparison.ahead, files: comparison.files.len() }
    }
}

pub use self::{
    merging::{MergeBox, MergeFailure, MergeInput},
    reviews::{LineComment, ReviewData, ReviewEntry, ReviewVerdict},
};
use crate::{
    DomainEvent, ProtectionService, PushService, RepoContext, RepositoryService, ServiceContext, ServiceError, ThrottleScope,
    ThrottleService, TimelineService, repositories::viewer_grants, timeline::IssueContext,
};

/// Pull requests per page of a list.
pub const PAGE_SIZE: u32 = 25;

/// A pull request opened for one viewer: the thread (with the viewer's permissions), the
/// pull request row and the repository its head comes from (`None` once deleted).
#[derive(Debug, Clone)]
pub struct PullContext {
    pub thread: IssueContext,
    pub pull: PullRequest,
    pub head_repo: Option<OwnedRepository>,
}

impl PullContext {
    pub fn repo(&self) -> &RepoContext {
        &self.thread.repo
    }

    pub fn issue(&self) -> &Issue {
        &self.thread.issue
    }

    /// `owner:branch` for heads in another repository, else `branch` (GitHub's head label).
    pub fn head_label(&self) -> String {
        match (&self.head_repo, self.pull.is_cross_repository()) {
            (Some(head), true) => format!("{}:{}", head.owner.name, self.pull.head_branch),
            (None, true) => format!("unknown:{}", self.pull.head_branch),
            _ => self.pull.head_branch.clone(),
        }
    }

    /// `owner/branch` of the head, for the default merge commit message.
    pub fn head_owner_branch(&self) -> String {
        let owner = self.head_repo.as_ref().map_or_else(|| "unknown".to_owned(), |head| head.owner.name.to_string());
        format!("{owner}/{}", self.pull.head_branch)
    }
}

/// What a new pull request proposes, as submitted.
#[derive(Debug, Clone, Default)]
pub struct NewPullInput {
    pub base_branch: String,
    pub head_branch: String,
    /// Empty: the single commit's summary, or the branch name.
    pub title: String,
    pub body: String,
    pub draft: bool,
}

/// One row of a pull request list.
#[derive(Debug, Clone)]
pub struct PullListItem {
    pub issue: Issue,
    pub pull: PullRequest,
    pub author: Option<Account>,
    pub approvals: usize,
    pub changes_requested: usize,
}

/// One page of a list, with the totals for the Open / Closed tabs.
#[derive(Debug, Clone, Default)]
pub struct PullList {
    pub items: Vec<PullListItem>,
    /// Matching the filter, across pages.
    pub total: u64,
    pub open: u64,
    pub closed: u64,
}

#[derive(Debug, Clone)]
pub struct PullService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    timeline: TimelineService,
    throttle: ThrottleService,
    pushes: PushService,
    protection: ProtectionService,
    repositories: RepositoryService,
    /// One merge at a time per base repository.
    merge_locks: Mutex<HashMap<RepoId, Arc<tokio::sync::Mutex<()>>>>,
    /// Merges running at once, instance-wide.
    merge_slots: Semaphore,
    /// Trial merges running at once.
    trial_slots: Arc<Semaphore>,
    /// Wakes the mergeability worker.
    wake: Notify,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl PullService {
    /// Builds the service on the shared timeline and throttle, so pull requests and
    /// reviews spend the same content budget as comments.
    pub(crate) fn with_shared(ctx: ServiceContext, timeline: TimelineService, throttle: ThrottleService) -> Self {
        let config = &ctx.config.pulls;
        let repositories =
            RepositoryService::new(Arc::clone(&ctx.config), ctx.database.repositories(), ctx.repositories.clone(), ctx.events.clone());
        Self {
            inner: Arc::new(Inner {
                timeline,
                throttle,
                pushes: PushService::new(ctx.clone()),
                protection: ProtectionService::new(ctx.clone()),
                repositories,
                merge_locks: Mutex::new(HashMap::new()),
                merge_slots: Semaphore::new(config.max_concurrent_merges.max(1)),
                trial_slots: Arc::new(Semaphore::new(config.mergeability_concurrency.max(1))),
                wake: Notify::new(),
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
                ctx,
            }),
        }
    }

    fn ctx(&self) -> &ServiceContext {
        &self.inner.ctx
    }

    fn git_binary(&self) -> &std::path::Path {
        &self.inner.ctx.config.repository.git_binary
    }

    // ------------------------------------------------------------- network and compare

    /// The repositories of `repo`'s fork network that `viewer` may read (including `repo`).
    pub async fn network(&self, viewer: Option<&User>, repo: &RepoContext) -> Result<Vec<OwnedRepository>, ServiceError> {
        Ok(self
            .ctx()
            .database
            .forks()
            .network_readable(repo.repo.repo.id, viewer.map(User::id), viewer.is_some_and(|v| v.is_admin))
            .await?)
    }

    /// The head repository a [`HeadSpec`] names, opened for `viewer`: `base` itself, the
    /// spec owner's repository in `base`'s network (the one named like `base` first), or
    /// `owner/name`. `NotFound` for anything outside the network or hidden from `viewer`.
    pub async fn resolve_head(&self, viewer: Option<&User>, base: &RepoContext, spec: &HeadSpec) -> Result<RepoContext, ServiceError> {
        let Some(owner) = &spec.owner else { return Ok(base.clone()) };
        let network = self.network(viewer, base).await?;
        let candidates: Vec<&OwnedRepository> =
            network.iter().filter(|repo| repo.owner.name.as_str().eq_ignore_ascii_case(owner)).collect();
        let found = match &spec.repo {
            Some(name) => candidates.into_iter().find(|repo| repo.repo.name.as_str().eq_ignore_ascii_case(name)),
            None => candidates.iter().find(|repo| repo.repo.name == base.repo.repo.name).or_else(|| candidates.first()).copied(),
        }
        .ok_or(ServiceError::NotFound)?;
        if found.repo.id == base.repo.repo.id {
            return Ok(base.clone());
        }
        self.inner.repositories.open(viewer, found.owner.name.as_str(), found.repo.name.as_str()).await
    }

    /// Compares `head_rev` of `head` against `base_rev` of `base` (branches, tags or ids
    /// in their own repository). Across repositories this reads both through a temporary
    /// object overlay.
    pub async fn compare(
        &self,
        base: &RepoContext,
        base_rev: &str,
        head: &RepoContext,
        head_rev: &str,
        limits: DiffLimits,
        max_commits: usize,
    ) -> Result<Comparison, ServiceError> {
        if base.repo.repo.id == head.repo.repo.id {
            return Ok(base.reader.compare(base_rev.to_owned(), head_rev.to_owned(), limits, max_commits).await?);
        }
        let base_id = base.reader.commit_id(base_rev.to_owned()).await?;
        let head_id = head.reader.commit_id(head_rev.to_owned()).await?;
        let overlay = self.overlay(base, head).await?;
        let comparison = overlay.reader().compare(base_id, head_id, limits, max_commits).await;
        drop(overlay);
        Ok(comparison?)
    }

    async fn overlay(&self, base: &RepoContext, head: &RepoContext) -> Result<Overlay, ServiceError> {
        let storage = &self.ctx().repositories;
        let (base_lease, head_lease) = (storage.lease(base.repo.repo.id).await, storage.lease(head.repo.repo.id).await);
        let overlay = Overlay::create(&self.ctx().config.storage.tmp_dir(), &[&base.disk_path, &head.disk_path]).await?;
        Ok(overlay.holding(base_lease).holding(head_lease))
    }

    // ------------------------------------------------------------- creating

    /// Opens a pull request from `input.head_branch` of `head` into `input.base_branch` of
    /// `base` (both opened for `actor`, in one fork network).
    pub async fn create(
        &self,
        actor: &User,
        base: &RepoContext,
        head: &RepoContext,
        input: NewPullInput,
    ) -> Result<(Issue, PullRequest), ServiceError> {
        if !base.access.can_read() || !head.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let same_repo = base.repo.repo.id == head.repo.repo.id;
        if !same_repo && !self.network(Some(actor), base).await?.iter().any(|repo| repo.repo.id == head.repo.repo.id) {
            return Err(ServiceError::NotFound);
        }
        base.ensure_writable()?;
        if self.ctx().database.mirrors().is_mirror(base.repo.repo.id).await? {
            return Err(ServiceError::Invalid("pull requests cannot be opened against a mirror".to_owned()));
        }
        let base_branch = input.base_branch.trim();
        let head_branch = input.head_branch.trim();
        let (Ok(base_ref), Ok(head_ref)) = (branch_ref(base_branch), branch_ref(head_branch)) else {
            return Err(ServiceError::Invalid("choose existing branches to compare".to_owned()));
        };
        if same_repo && base_branch == head_branch {
            return Err(ServiceError::Invalid("the base and the head branch must differ".to_owned()));
        }
        let missing = |side: &str| ServiceError::Invalid(format!("the {side} branch does not exist"));
        let base_tip = base.reader.commit_id(base_ref.clone()).await.map_err(|_| missing("base"))?;
        let head_tip = head.reader.commit_id(head_ref.clone()).await.map_err(|_| missing("head"))?;
        let comparison = self.compare(base, &base_tip, head, &head_tip, DiffLimits { max_files: 0, ..DiffLimits::default() }, 2).await?;
        if comparison.merge_base.is_none() {
            return Err(ServiceError::Invalid("the branches have entirely different histories".to_owned()));
        }
        if comparison.ahead == 0 {
            return Err(ServiceError::Invalid(format!("{base_branch} already contains every commit of {head_branch}")));
        }
        let title = match input.title.trim() {
            "" if comparison.ahead == 1 => comparison.commits.first().map_or_else(|| head_branch.to_owned(), |c| c.summary.clone()),
            "" => head_branch.to_owned(),
            title => title.to_owned(),
        };
        let title = rbit_core::IssueTitle::parse(&title)?;
        let body = rbit_core::IssueText::body(&input.body)?;
        self.spend(actor)?;

        let now = Timestamp::now();
        let new = NewPull {
            issue: NewIssue { repo: base.repo.repo.id, kind: IssueKind::Pull, title, body, author: actor.id(), at: now },
            head_repo: head.repo.repo.id,
            head_branch: head_branch.to_owned(),
            base_branch: base_branch.to_owned(),
            head_sha: head_tip.clone(),
            is_draft: input.draft,
        };
        let (issue, mut pull) = match self.ctx().database.pulls().create(&new).await? {
            CreatedPull::Created(pair) => *pair,
            CreatedPull::Duplicate(number) => {
                return Err(ServiceError::Invalid(format!("pull request #{} already proposes these branches", number.get())));
            }
        };
        match self.store_head(base, head, &pull, issue.number).await {
            Ok(stored) if stored != pull.head_sha => {
                self.ctx().database.pulls().set_head(issue.id, &stored, Timestamp::now()).await?;
                pull.head_sha = stored;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, pull = issue.id.0, "cannot store refs/pull/{}/head; the next push retries", issue.number.get());
            }
        }
        tracing::info!(actor = %actor.name(), repo = %base.repo.full_name(), number = issue.number.get(), "pull request opened");
        self.publish(&issue, Some(actor.id()), IssueAction::Opened);
        self.inner.wake.notify_one();
        Ok((issue, pull))
    }

    /// Points `refs/pull/{n}/head` of `base` at the head: `update-ref` for the same
    /// repository, a fetch from a fork. Returns the id it now points at.
    async fn store_head(
        &self,
        base: &RepoContext,
        head: &RepoContext,
        pull: &PullRequest,
        number: IssueNumber,
    ) -> Result<String, ServiceError> {
        let storage = &self.ctx().repositories;
        let _base_lease = storage.lease(base.repo.repo.id).await;
        if base.repo.repo.id == head.repo.repo.id {
            pull_refs::set(self.git_binary(), &base.disk_path, number.get(), &pull.head_sha).await?;
            return Ok(pull.head_sha.clone());
        }
        let _head_lease = storage.lease(head.repo.repo.id).await;
        Ok(pull_refs::fetch(self.git_binary(), &base.disk_path, &head.disk_path, &pull.head_branch, number.get()).await?)
    }

    // ------------------------------------------------------------- reading

    /// Pull request `number` of `repo` (already opened for `viewer`). `NotFound` when it
    /// does not exist or is an issue (callers redirect issue numbers themselves).
    pub async fn open(&self, viewer: Option<&User>, repo: RepoContext, number: IssueNumber) -> Result<PullContext, ServiceError> {
        let thread = self.inner.timeline.open(viewer, repo, number).await?;
        if !thread.issue.is_pull() {
            return Err(ServiceError::NotFound);
        }
        self.context(thread).await
    }

    /// The pull request of an already opened thread (of kind `Pull`).
    pub async fn context(&self, thread: IssueContext) -> Result<PullContext, ServiceError> {
        let pull = self.ctx().database.pulls().find(thread.issue.id).await?.ok_or(ServiceError::NotFound)?;
        let head_repo = match pull.head_repo_id {
            Some(id) if id == pull.repo_id => Some(thread.repo.repo.clone()),
            Some(id) => self.ctx().database.admin().find_repository(id).await?,
            None => None,
        };
        Ok(PullContext { thread, pull, head_repo })
    }

    /// One page (1-based) of `repo`'s pull requests matching `filter`.
    pub async fn list(&self, repo: &RepoContext, filter: &PullFilter, page: u32) -> Result<PullList, ServiceError> {
        self.list_page(repo, filter, PAGE_SIZE, page.max(1).saturating_sub(1).saturating_mul(PAGE_SIZE)).await
    }

    /// `limit` pull requests from `offset`, for the API's own pagination.
    pub async fn list_page(&self, repo: &RepoContext, filter: &PullFilter, limit: u32, offset: u32) -> Result<PullList, ServiceError> {
        let store = self.ctx().database.pulls();
        let id = repo.repo.repo.id;
        let open_filter = PullFilter { state: PullStateFilter::Open, ..filter.clone() };
        let closed_filter = PullFilter { state: PullStateFilter::Closed, ..filter.clone() };
        let (rows, total, open, closed) = tokio::try_join!(
            store.list(id, filter, limit, offset),
            store.count(id, filter),
            store.count(id, &open_filter),
            store.count(id, &closed_filter)
        )?;
        let ids: Vec<_> = rows.iter().map(|(issue, _)| issue.id).collect();
        let authors: Vec<AccountId> = rows.iter().filter_map(|(issue, _)| issue.author).collect();
        let (review_store, account_store) = (self.ctx().database.reviews(), self.ctx().database.accounts());
        let (reviews, accounts) = tokio::try_join!(review_store.submitted_among(&ids), account_store.find_accounts(&authors))?;
        let accounts: HashMap<AccountId, Account> = accounts.into_iter().map(|account| (account.id, account)).collect();
        let mut items = Vec::with_capacity(rows.len());
        for (issue, pull) in rows {
            let tally =
                rbit_core::pull::tally(reviews.get(&issue.id).map_or(&[][..], Vec::as_slice), issue.author, &pull.head_sha, false, |_| {
                    true
                });
            items.push(PullListItem {
                author: issue.author.and_then(|id| accounts.get(&id).cloned()),
                approvals: tally.approvals.len(),
                changes_requested: tally.changes_requested.len(),
                issue,
                pull,
            });
        }
        Ok(PullList { items, total, open, closed })
    }

    /// Search pull requests through the same query and metadata filters as issues.
    pub async fn search(&self, repo: &RepoContext, results: &crate::issues::IssueSearchResults) -> Result<Vec<PullListItem>, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let ids: Vec<_> =
            results.issues.iter().filter(|issue| issue.repo_id == repo.repo.repo.id && issue.is_pull()).map(|issue| issue.id).collect();
        let pull_store = self.ctx().database.pulls();
        let review_store = self.ctx().database.reviews();
        let (pulls, reviews) = tokio::try_join!(pull_store.find_among(repo.repo.repo.id, &ids), review_store.submitted_among(&ids))?;
        let pulls: HashMap<_, _> = pulls.into_iter().map(|pull| (pull.issue_id, pull)).collect();
        let mut items = Vec::with_capacity(ids.len());
        for issue in &results.issues {
            let Some(pull) = pulls.get(&issue.id) else { continue };
            let tally =
                rbit_core::pull::tally(reviews.get(&issue.id).map_or(&[][..], Vec::as_slice), issue.author, &pull.head_sha, false, |_| {
                    true
                });
            items.push(PullListItem {
                author: issue.author.and_then(|id| results.authors.get(&id).cloned()),
                approvals: tally.approvals.len(),
                changes_requested: tally.changes_requested.len(),
                issue: issue.clone(),
                pull: pull.clone(),
            });
        }
        Ok(items)
    }

    /// The tip of the base branch, or `None` when it no longer exists.
    pub async fn base_tip(&self, ctx: &PullContext) -> Result<Option<String>, ServiceError> {
        Ok(ctx.repo().reader.commit_id(format!("refs/heads/{}", ctx.pull.base_branch)).await.ok())
    }

    /// The ids this pull request compares: `(base, head)`. Merged pull requests keep
    /// showing what was merged; open ones compare the base branch's tip with the head.
    pub async fn compared_ids(&self, ctx: &PullContext) -> Result<(String, String), ServiceError> {
        if let (Some(base), Some(head)) = (&ctx.pull.merged_base_sha, &ctx.pull.merged_head_sha) {
            return Ok((base.clone(), head.clone()));
        }
        let base = self.base_tip(ctx).await?.ok_or(ServiceError::NotFound)?;
        Ok((base, ctx.pull.head_sha.clone()))
    }

    /// The comparison behind the Commits and Files changed tabs: from the merge base to
    /// the head, inside the base repository (`refs/pull/{n}/head` holds the head).
    ///
    /// With the default file limit the counts are remembered (see [`Self::cached_counts`]),
    /// so the other tabs can show them without diffing.
    pub async fn changes(&self, ctx: &PullContext, limits: DiffLimits) -> Result<Comparison, ServiceError> {
        let (base, head) = self.compared_ids(ctx).await?;
        let max_commits = self.ctx().config.pulls.max_commits;
        let comparison = ctx.repo().reader.compare(base.clone(), head.clone(), limits, max_commits).await?;
        if limits.max_files == DiffLimits::default().max_files {
            self.remember_counts(&base, &head, PullCounts::of(&comparison)).await;
        }
        Ok(comparison)
    }

    /// The header counts of comparing `head` against `base` (full commit ids) when a
    /// Files changed tab or [`Self::counts_between`] computed them before. Cache failures
    /// are logged and read as a miss.
    ///
    /// The cache is shared by every repository: pass only ids the caller already knows
    /// the viewer may see (those of [`Self::compared_ids`]), never ids from a request.
    pub async fn cached_counts(&self, base: &str, head: &str) -> Option<PullCounts> {
        match self.ctx().kv.insights().comparison_counts(base, head).await {
            // Counted under another file limit (an older or newer server): not this answer.
            Ok(counts) => counts.filter(|counts| counts.max_files == counted_max_files()).map(|counts| PullCounts {
                ahead: usize::try_from(counts.ahead).unwrap_or(usize::MAX),
                files: usize::try_from(counts.files).unwrap_or(usize::MAX),
            }),
            Err(error) => {
                tracing::warn!(%error, "comparison cache read failed");
                None
            }
        }
    }

    /// The header counts of comparing `head` against `base` (commit ids) in `ctx`'s
    /// base repository: cached, or computed with the default diff limits and remembered.
    ///
    /// Only the pair this pull request compares ([`Self::compared_ids`]) is answered;
    /// any other pair is [`ServiceError::NotFound`]. The ids come from a URL, and the
    /// cache is shared by every repository: without the check a viewer of one pull
    /// request could read (or confirm) the counts of commits in a repository they cannot
    /// see, or make the server diff arbitrary pairs. Within that check an id-only key is
    /// sound: a pair of commit ids compares the same way wherever both exist.
    pub async fn counts_between(&self, ctx: &PullContext, base: &str, head: &str) -> Result<PullCounts, ServiceError> {
        let (compared_base, compared_head) = self.compared_ids(ctx).await?;
        if base != compared_base || head != compared_head {
            return Err(ServiceError::NotFound);
        }
        if let Some(counts) = self.cached_counts(base, head).await {
            return Ok(counts);
        }
        let comparison = ctx.repo().reader.compare(base.to_owned(), head.to_owned(), DiffLimits::default(), 0).await?;
        let counts = PullCounts::of(&comparison);
        // Keyed by the ids the comparison resolved, so a short id never fills the cache.
        self.remember_counts(&comparison.base.id, &comparison.head.id, counts).await;
        Ok(counts)
    }

    async fn remember_counts(&self, base: &str, head: &str, counts: PullCounts) {
        let cached = CachedComparisonCounts {
            ahead: u64::try_from(counts.ahead).unwrap_or(u64::MAX),
            files: u64::try_from(counts.files).unwrap_or(u64::MAX),
            max_files: counted_max_files(),
        };
        if let Err(error) = self.ctx().kv.insights().put_comparison_counts(base, head, cached).await {
            tracing::warn!(%error, "comparison cache write failed");
        }
    }

    /// The commits of the pull request, newest first (at most `[pulls] max_commits`).
    pub async fn commits(&self, ctx: &PullContext) -> Result<Vec<CommitSummary>, ServiceError> {
        Ok(self.changes(ctx, DiffLimits { max_files: 0, ..DiffLimits::default() }).await?.commits)
    }

    /// Whether the head branch still exists in the head repository.
    pub async fn head_branch_exists(&self, ctx: &PullContext) -> bool {
        let Some(head) = &ctx.head_repo else { return false };
        let reader = self.ctx().repositories.reader(head.repo.owner_id, head.repo.id);
        reader.commit_id(format!("refs/heads/{}", ctx.pull.head_branch)).await.is_ok()
    }

    // ------------------------------------------------------------- drafts and base

    /// Marks the pull request draft or ready for review (author or writers).
    pub async fn set_draft(&self, actor: &User, ctx: &PullContext, draft: bool) -> Result<(), ServiceError> {
        if !(ctx.issue().is_author(actor) || ctx.repo().access.can_write()) || !ctx.repo().access.can_read() {
            return Err(ServiceError::Forbidden);
        }
        ctx.repo().ensure_writable()?;
        if !ctx.issue().is_open() {
            return Err(ServiceError::Invalid("the pull request is closed".to_owned()));
        }
        if self.ctx().database.pulls().set_draft(ctx.issue().id, draft, Timestamp::now()).await? {
            let (event, action) = if draft {
                (rbit_core::TimelineEvent::ConvertedToDraft, IssueAction::ConvertedToDraft)
            } else {
                (rbit_core::TimelineEvent::ReadyForReview, IssueAction::ReadyForReview)
            };
            self.inner.timeline.record(ctx.issue().id, Some(actor.id()), event).await?;
            self.publish(ctx.issue(), Some(actor.id()), action);
            self.inner.wake.notify_one();
        }
        Ok(())
    }

    /// Points the pull request at another existing branch of the base repository (author
    /// or writers).
    pub async fn set_base(&self, actor: &User, ctx: &PullContext, branch: &str) -> Result<(), ServiceError> {
        if !(ctx.issue().is_author(actor) || ctx.repo().access.can_write()) {
            return Err(ServiceError::Forbidden);
        }
        ctx.repo().ensure_writable()?;
        let branch = branch.trim();
        if !ctx.issue().is_open() || ctx.pull.is_merged() {
            return Err(ServiceError::Invalid("the pull request is closed".to_owned()));
        }
        let full = branch_ref(branch).map_err(|_| ServiceError::Invalid("choose an existing branch".to_owned()))?;
        if ctx.repo().reader.commit_id(full).await.is_err() {
            return Err(ServiceError::Invalid("the base branch does not exist".to_owned()));
        }
        if !ctx.pull.is_cross_repository() && branch == ctx.pull.head_branch {
            return Err(ServiceError::Invalid("the base and the head branch must differ".to_owned()));
        }
        let previous = ctx.pull.base_branch.clone();
        if self.ctx().database.pulls().set_base(ctx.issue().id, branch, Timestamp::now()).await? {
            let event = rbit_core::TimelineEvent::BaseChanged { from: previous.clone(), to: branch.to_owned() };
            self.inner.timeline.record(ctx.issue().id, Some(actor.id()), event).await?;
            self.publish(
                ctx.issue(),
                Some(actor.id()),
                IssueAction::Edited { previous_title: None, previous_body: None, previous_base: Some(previous) },
            );
            self.inner.wake.notify_one();
        }
        Ok(())
    }

    // ------------------------------------------------------------- JSON

    /// The `pull_request` object of `ctx`, as the API returns it.
    pub async fn json(&self, ctx: &PullContext, base_url: &str) -> Result<serde_json::Value, ServiceError> {
        json::load(self.ctx(), ctx.issue(), &ctx.pull, base_url).await
    }

    /// Whether the pull request was merged into the repository's default branch.
    pub async fn merged_into_default_branch(&self, pull: rbit_core::IssueId) -> Result<bool, ServiceError> {
        lifecycle::merged_into_default_branch(self.ctx(), pull).await
    }

    /// The `pull_request` object of a listed pull request.
    pub async fn json_of(&self, issue: &Issue, pull: &PullRequest, base_url: &str) -> Result<serde_json::Value, ServiceError> {
        json::load(self.ctx(), issue, pull, base_url).await
    }

    /// The webhook payload [`hooks::payload`] builds for `event` (for tools and tests).
    pub async fn payload(&self, event: &DomainEvent, base_url: &str) -> Result<Option<crate::webhooks::HookPayload>, ServiceError> {
        hooks::payload(self.ctx(), event, base_url).await
    }

    // ------------------------------------------------------------- helpers

    fn spend(&self, actor: &User) -> Result<(), ServiceError> {
        self.inner.throttle.check(ThrottleScope::Content, &actor.id().0.to_string())
    }

    fn publish(&self, issue: &Issue, actor: Option<AccountId>, action: IssueAction) {
        self.ctx().events.publish(DomainEvent::Issue {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: issue.kind,
            actor,
            action,
        });
    }

    /// `account`'s access to `repo` (token scopes do not apply: this is about the
    /// account, e.g. whether a reviewer is a writer).
    async fn access_of(&self, account: AccountId, repo: &OwnedRepository) -> Result<RepoAccess, ServiceError> {
        let Some(user) = self.ctx().database.accounts().find_user(account).await? else { return Ok(RepoAccess::None) };
        let grants = viewer_grants(&self.ctx().database.repositories(), Some(&user), repo).await?;
        Ok(repo_access(Some(&user), repo, &grants))
    }
}

/// `refs/heads/{branch}` for a plausible branch name.
fn branch_ref(branch: &str) -> Result<String, ServiceError> {
    if branch.is_empty()
        || branch.starts_with('-')
        || branch.contains(['\0', '\n', ' ', ':', '~', '^', '?', '*', '[', '\\'])
        || branch.contains("..")
    {
        return Err(ServiceError::Invalid("invalid branch name".to_owned()));
    }
    Ok(format!("refs/heads/{branch}"))
}

/// Whether the `PoisonError`-tolerant lock map hands out the same lock for a repository.
fn merge_lock(locks: &Mutex<HashMap<RepoId, Arc<tokio::sync::Mutex<()>>>>, repo: RepoId) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = locks.lock().unwrap_or_else(PoisonError::into_inner);
    locks.retain(|_, lock| Arc::strong_count(lock) > 1);
    Arc::clone(locks.entry(repo).or_default())
}
