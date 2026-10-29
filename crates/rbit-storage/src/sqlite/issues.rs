//! The shared core of issues and pull requests: numbered threads, their comments and
//! their timeline events (migration `0017_issues_core.sql`).
//!
//! The integrator owns this file (T0 in `docs/design/milestone-2b.md`). It is the only
//! code that writes `issue_counters`, `issues`, `issue_comments` and `issue_events`;
//! other stores may read them and reference `issues (id)`. Pull requests insert their
//! thread with [`IssueStore::insert_in`] inside their own transaction.
//!
//! Invariants kept here, in the same transaction as the change that needs them:
//! * numbers come from `issue_counters` and are never reused, even after a deletion;
//! * `comment_count` counts the thread's comments, and every comment or event moves
//!   `updated_at` forward (never backwards);
//! * a title change records a [`TimelineEvent::Renamed`], a state or lock change the
//!   event that explains it.

use super::{
    pool::{Conn, Pool},
    rows::AccountRow,
};
use rbit_core::{
    Account, AccountId, AccountKind, Comment, CommentId, CommentSource, EventId, Issue, IssueId, IssueKind, IssueNumber, IssueState,
    IssueText, IssueTitle, RepoId, StateReason, TimelineEvent, Timestamp,
};

use crate::StorageError;

/// The `issues` columns in [`IssueRow`] order.
macro_rules! issue_columns {
    () => {
        "id, repo_id, number, kind, title, body, author_id, state, state_reason, closed_at, closed_by, locked, comment_count, \
         created_at, updated_at"
    };
}

/// The `issue_comments` columns in [`CommentRow`] order.
macro_rules! comment_columns {
    () => {
        "id, issue_id, author_id, body, created_at, updated_at, edited_at, source"
    };
}

/// One page of the timeline: comments (`rank` 0) and events (`rank` 1) of issue `?1`
/// after the cursor `(?2, ?3, ?4)`, each with its author or actor joined, ordered by
/// `(created_at, rank, id)`, at most `?5` rows.
macro_rules! timeline_page {
    () => {
        concat!(
            "SELECT * FROM (",
            "SELECT 0 AS rank, c.id, c.created_at, c.updated_at, c.edited_at, c.body, c.source, NULL AS kind, NULL AS data, ",
            "c.author_id AS account_id, a.kind AS account_kind, a.name AS account_name, a.display_name AS account_display_name, ",
            "a.bio AS account_bio, a.created_at AS account_created_at ",
            "FROM issue_comments c LEFT JOIN accounts a ON a.id = c.author_id ",
            "WHERE c.issue_id = ?1 AND (c.created_at, 0, c.id) > (?2, ?3, ?4) ",
            "UNION ALL ",
            "SELECT 1 AS rank, e.id, e.created_at, NULL, NULL, NULL, NULL, e.kind, e.data, ",
            "e.actor_id, a.kind, a.name, a.display_name, a.bio, a.created_at ",
            "FROM issue_events e LEFT JOIN accounts a ON a.id = e.actor_id ",
            "WHERE e.issue_id = ?1 AND (e.created_at, 1, e.id) > (?2, ?3, ?4)",
            ") ORDER BY created_at, rank, id LIMIT ?5"
        )
    };
}

fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

/// Open threads of a repository per kind. Naming both kinds makes `kind` an equality on
/// `issues_by_repo (repo_id, kind, state, …)`, so only open entries are read; filtering on
/// `repo_id` and `state` alone would read every thread the repository ever had.
const OPEN_COUNTS: &str =
    "SELECT kind, COUNT(*) FROM issues WHERE repo_id = ? AND kind IN ('issue', 'pull') AND state = 'open' GROUP BY kind";

// ------------------------------------------------------------------ rows

#[derive(Debug, sqlx::FromRow)]
struct IssueRow {
    id: i64,
    repo_id: i64,
    number: i64,
    kind: String,
    title: String,
    body: String,
    author_id: Option<i64>,
    state: String,
    state_reason: Option<String>,
    closed_at: Option<i64>,
    closed_by: Option<i64>,
    locked: bool,
    comment_count: i64,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<IssueRow> for Issue {
    type Error = StorageError;

    fn try_from(row: IssueRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: IssueId(row.id),
            repo_id: RepoId(row.repo_id),
            number: IssueNumber::try_from(row.number).map_err(|e| corrupt("issue number", e))?,
            kind: IssueKind::parse(&row.kind).ok_or_else(|| corrupt("issue kind", &row.kind))?,
            title: IssueTitle::parse(&row.title).map_err(|e| corrupt("issue title", e))?,
            body: IssueText::body(&row.body).map_err(|e| corrupt("issue body", e))?,
            author: row.author_id.map(AccountId),
            state: IssueState::parse(&row.state).ok_or_else(|| corrupt("issue state", &row.state))?,
            state_reason: row
                .state_reason
                .map(|reason| StateReason::parse(&reason).ok_or_else(|| corrupt("state reason", &reason)))
                .transpose()?,
            closed_at: row.closed_at.map(Timestamp),
            closed_by: row.closed_by.map(AccountId),
            locked: row.locked,
            comment_count: u32::try_from(row.comment_count).map_err(|e| corrupt("comment count", e))?,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        })
    }
}

/// A thread with its author's account joined (`author_*`, all `NULL` without one).
#[derive(Debug, sqlx::FromRow)]
struct IssueWithAuthorRow {
    #[sqlx(flatten)]
    issue: IssueRow,
    author_kind: Option<String>,
    author_name: Option<String>,
    author_display_name: Option<String>,
    author_bio: Option<String>,
    author_created_at: Option<i64>,
}

/// Thread `?2` of repository `?1` with its author: [`IssueWithAuthorRow`].
///
/// The author's columns are scalar subqueries by primary key rather than a join, so
/// both tables are read by key under any planner statistics (as `REPOSITORY_PAGE`
/// explains); as a join, one-row statistics made `SQLite` scan both.
const ISSUE_WITH_AUTHOR: &str = "SELECT i.id, i.repo_id, i.number, i.kind, i.title, i.body, i.author_id, i.state, i.state_reason, \
     i.closed_at, i.closed_by, i.locked, i.comment_count, i.created_at, i.updated_at, \
     (SELECT kind FROM accounts WHERE id = i.author_id) AS author_kind, \
     (SELECT name FROM accounts WHERE id = i.author_id) AS author_name, \
     (SELECT display_name FROM accounts WHERE id = i.author_id) AS author_display_name, \
     (SELECT bio FROM accounts WHERE id = i.author_id) AS author_bio, \
     (SELECT created_at FROM accounts WHERE id = i.author_id) AS author_created_at \
     FROM issues i WHERE i.repo_id = ?1 AND i.number = ?2";

#[derive(Debug, sqlx::FromRow)]
struct CommentRow {
    id: i64,
    issue_id: i64,
    author_id: Option<i64>,
    body: String,
    created_at: i64,
    updated_at: i64,
    edited_at: Option<i64>,
    source: String,
}

/// A stored comment source.
fn comment_source(value: &str) -> Result<CommentSource, StorageError> {
    CommentSource::parse(value).ok_or_else(|| corrupt("comment source", value))
}

impl TryFrom<CommentRow> for Comment {
    type Error = StorageError;

    fn try_from(row: CommentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: CommentId(row.id),
            issue_id: IssueId(row.issue_id),
            author: row.author_id.map(AccountId),
            body: IssueText::comment(&row.body).map_err(|e| corrupt("comment", e))?,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            edited_at: row.edited_at.map(Timestamp),
            source: comment_source(&row.source)?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RefRow {
    id: i64,
    number: i64,
    kind: String,
    state: String,
    title: String,
}

impl TryFrom<RefRow> for IssueRef {
    type Error = StorageError;

    fn try_from(row: RefRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: IssueId(row.id),
            number: IssueNumber::try_from(row.number).map_err(|e| corrupt("issue number", e))?,
            kind: IssueKind::parse(&row.kind).ok_or_else(|| corrupt("issue kind", &row.kind))?,
            state: IssueState::parse(&row.state).ok_or_else(|| corrupt("issue state", &row.state))?,
            title: row.title,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SourceRow {
    #[sqlx(flatten)]
    issue: RefRow,
    owner_name: String,
    repo_name: String,
}

/// A comment or event row of [`timeline_page!`], with its account columns.
#[derive(Debug, sqlx::FromRow)]
struct TimelineRow {
    rank: i64,
    id: i64,
    created_at: i64,
    updated_at: Option<i64>,
    edited_at: Option<i64>,
    body: Option<String>,
    source: Option<String>,
    kind: Option<String>,
    data: Option<String>,
    account_id: Option<i64>,
    account_kind: Option<String>,
    account_name: Option<String>,
    account_display_name: Option<String>,
    account_bio: Option<String>,
    account_created_at: Option<i64>,
}

impl TimelineRow {
    fn cursor(&self) -> TimelineCursor {
        let item = if self.rank == 0 { TimelineItemId::Comment(CommentId(self.id)) } else { TimelineItemId::Event(EventId(self.id)) };
        TimelineCursor { at: Timestamp(self.created_at), item }
    }

    /// The joined account; `None` when it was deleted (a ghost).
    fn account(&self) -> Result<Option<Account>, StorageError> {
        let (Some(id), Some(kind), Some(name)) = (self.account_id, &self.account_kind, &self.account_name) else {
            return Ok(None);
        };
        Ok(Some(Account {
            id: AccountId(id),
            kind: AccountKind::parse(kind).ok_or_else(|| corrupt("account kind", kind))?,
            name: name.parse().map_err(|e| corrupt("account name", e))?,
            display_name: self.account_display_name.clone(),
            bio: self.account_bio.clone(),
            created_at: Timestamp(self.account_created_at.unwrap_or_default()),
        }))
    }

    /// The item, or `None` for an event of a kind this version does not know.
    fn into_item(self, issue: IssueId) -> Result<Option<StoredTimelineItem>, StorageError> {
        let account = self.account()?;
        let at = Timestamp(self.created_at);
        if self.rank == 0 {
            let body = self.body.unwrap_or_default();
            let comment = Comment {
                id: CommentId(self.id),
                issue_id: issue,
                author: self.account_id.map(AccountId),
                body: IssueText::comment(&body).map_err(|e| corrupt("comment", e))?,
                created_at: at,
                updated_at: Timestamp(self.updated_at.unwrap_or(self.created_at)),
                edited_at: self.edited_at.map(Timestamp),
                source: comment_source(self.source.as_deref().unwrap_or_default())?,
            };
            return Ok(Some(StoredTimelineItem::Comment { comment, author: account }));
        }
        let kind = self.kind.unwrap_or_default();
        let Some(event) = decode_event(&kind, self.data.as_deref().unwrap_or("{}")) else {
            tracing::debug!(event = self.id, kind, "skipping a timeline event of an unknown kind");
            return Ok(None);
        };
        Ok(Some(StoredTimelineItem::Event { id: EventId(self.id), actor: account, actor_id: self.account_id.map(AccountId), event, at }))
    }
}

/// `kind` and `data` columns for `event`: the variant's tag and its fields (`{}` without).
fn encode_event(event: &TimelineEvent) -> Result<(&'static str, String), StorageError> {
    let value = serde_json::to_value(event).map_err(|e| corrupt("timeline event", e))?;
    let data = value.get("data").map_or_else(|| "{}".to_owned(), serde_json::Value::to_string);
    Ok((event.kind(), data))
}

/// The event stored as `kind` + `data`, or `None` when the kind is unknown or the data
/// does not fit it.
fn decode_event(kind: &str, data: &str) -> Option<TimelineEvent> {
    let data: serde_json::Value = serde_json::from_str(data).ok()?;
    let empty = data.as_object().is_some_and(serde_json::Map::is_empty);
    let tagged = if empty { serde_json::json!({ "kind": kind }) } else { serde_json::json!({ "kind": kind, "data": data }) };
    serde_json::from_value(tagged).ok()
}

/// A JSON array of ids or numbers, for `json_each(?)`.
fn json_array(values: impl IntoIterator<Item = i64>) -> String {
    let values: Vec<i64> = values.into_iter().collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned())
}

// ------------------------------------------------------------------ public types

/// An issue or pull request to insert. Its number is assigned on insertion.
#[derive(Debug, Clone)]
pub struct NewIssue {
    pub repo: RepoId,
    pub kind: IssueKind,
    pub title: IssueTitle,
    pub body: IssueText,
    pub author: AccountId,
    pub at: Timestamp,
}

/// What a reference (`#12`) needs to render: enough to link and describe the thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueRef {
    pub id: IssueId,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub state: IssueState,
    pub title: String,
}

/// A thread that mentions another one ([`TimelineEvent::CrossReferenced`]), with the
/// repository it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueSource {
    pub issue: IssueRef,
    /// `owner/name` of the source's repository.
    pub full_name: String,
}

/// Which row a timeline item is: comments and events have separate id spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimelineItemId {
    Comment(CommentId),
    Event(EventId),
}

/// A position in a timeline: items are ordered by `(created_at, comments before events,
/// id)`, and a page starts after its cursor. Written into URLs as `{at}-c{id}` or
/// `{at}-e{id}` ([`Self::to_param`], [`Self::parse`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimelineCursor {
    pub at: Timestamp,
    pub item: TimelineItemId,
}

impl TimelineCursor {
    /// `(created_at, rank, id)` as bound to the query.
    fn key(self) -> (i64, i64, i64) {
        match self.item {
            TimelineItemId::Comment(id) => (self.at.0, 0, id.0),
            TimelineItemId::Event(id) => (self.at.0, 1, id.0),
        }
    }

    /// The URL form, e.g. `1760000000-c42`.
    pub fn to_param(self) -> String {
        match self.item {
            TimelineItemId::Comment(id) => format!("{}-c{}", self.at.0, id.0),
            TimelineItemId::Event(id) => format!("{}-e{}", self.at.0, id.0),
        }
    }

    /// Reads [`Self::to_param`]'s form; `None` for anything else.
    pub fn parse(value: &str) -> Option<Self> {
        let (at, item) = value.split_once('-')?;
        let at = at.parse::<i64>().ok().filter(|at| *at >= 0)?;
        let parse_id = |id: &str| id.parse::<i64>().ok().filter(|id| *id > 0);
        let item = if let Some(id) = item.strip_prefix('c') {
            TimelineItemId::Comment(CommentId(parse_id(id)?))
        } else {
            TimelineItemId::Event(EventId(parse_id(item.strip_prefix('e')?)?))
        };
        Some(Self { at: Timestamp(at), item })
    }
}

/// A comment or event as stored, with its author or actor account (`None`: deleted).
#[derive(Debug, Clone)]
pub enum StoredTimelineItem {
    Comment {
        comment: Comment,
        author: Option<Account>,
    },
    Event {
        id: EventId,
        actor: Option<Account>,
        /// The actor's id even when the account row is gone (it is `NULL` then too, by
        /// `ON DELETE SET NULL`, so this is `None` exactly when `actor` is).
        actor_id: Option<AccountId>,
        event: TimelineEvent,
        at: Timestamp,
    },
}

impl StoredTimelineItem {
    pub fn cursor(&self) -> TimelineCursor {
        match self {
            Self::Comment { comment, .. } => TimelineCursor { at: comment.created_at, item: TimelineItemId::Comment(comment.id) },
            Self::Event { id, at, .. } => TimelineCursor { at: *at, item: TimelineItemId::Event(*id) },
        }
    }
}

/// One page of a timeline. `next` is set when more items follow; events of unknown kinds
/// are left out of `items` but still count towards the page.
#[derive(Debug, Clone, Default)]
pub struct TimelinePage {
    pub items: Vec<StoredTimelineItem>,
    pub next: Option<TimelineCursor>,
}

// ------------------------------------------------------------------ the store

#[derive(Debug, Clone)]
pub struct IssueStore {
    pool: Pool,
}

impl IssueStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Inserts an issue or pull request with the repository's next number.
    pub async fn create(&self, new: &NewIssue) -> Result<Issue, StorageError> {
        let mut tx = self.pool.begin().await?;
        let issue = Self::insert_in(&mut tx, new).await?;
        tx.commit().await?;
        Ok(issue)
    }

    /// [`Self::create`] inside a caller's transaction, so a pull request's own row can be
    /// inserted atomically with its thread. The number is taken by the transaction's
    /// first write, which serialises concurrent creations.
    pub(crate) async fn insert_in(conn: &mut Conn, new: &NewIssue) -> Result<Issue, StorageError> {
        let number: i64 = sqlx::query_scalar(
            "INSERT INTO issue_counters (repo_id, last_number) VALUES (?, 1) \
             ON CONFLICT (repo_id) DO UPDATE SET last_number = last_number + 1 RETURNING last_number",
        )
        .bind(new.repo.0)
        .fetch_one(&mut *conn)
        .await?;
        let row: IssueRow = sqlx::query_as(concat!(
            "INSERT INTO issues (repo_id, number, kind, title, body, author_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING ",
            issue_columns!()
        ))
        .bind(new.repo.0)
        .bind(number)
        .bind(new.kind.as_str())
        .bind(new.title.as_str())
        .bind(new.body.as_str())
        .bind(new.author.0)
        .bind(new.at.0)
        .bind(new.at.0)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| StorageError::from_insert(e, "issue number"))?;
        row.try_into()
    }

    pub async fn find(&self, id: IssueId) -> Result<Option<Issue>, StorageError> {
        let row: Option<IssueRow> =
            sqlx::query_as(concat!("SELECT ", issue_columns!(), " FROM issues WHERE id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(Issue::try_from).transpose()
    }

    /// The issue or pull request numbered `number` in `repo`.
    pub async fn find_by_number(&self, repo: RepoId, number: IssueNumber) -> Result<Option<Issue>, StorageError> {
        let row: Option<IssueRow> = sqlx::query_as(concat!("SELECT ", issue_columns!(), " FROM issues WHERE repo_id = ? AND number = ?"))
            .bind(repo.0)
            .bind(number.get())
            .fetch_optional(&self.pool)
            .await?;
        row.map(Issue::try_from).transpose()
    }

    /// Like [`Self::find_by_number`], with the author's account (`None` when there is no
    /// author, or the account is gone) in the same statement.
    pub async fn find_by_number_with_author(
        &self,
        repo: RepoId,
        number: IssueNumber,
    ) -> Result<Option<(Issue, Option<Account>)>, StorageError> {
        let row: Option<IssueWithAuthorRow> =
            sqlx::query_as(ISSUE_WITH_AUTHOR).bind(repo.0).bind(number.get()).fetch_optional(&self.pool).await?;
        let Some(row) = row else { return Ok(None) };
        let author = match (row.issue.author_id, row.author_kind, row.author_name, row.author_created_at) {
            (Some(id), Some(kind), Some(name), Some(created_at)) => Some(Account::try_from(AccountRow {
                id,
                kind,
                name,
                display_name: row.author_display_name,
                bio: row.author_bio,
                created_at,
            })?),
            _ => None,
        };
        Ok(Some((Issue::try_from(row.issue)?, author)))
    }

    /// The threads of `repo` among `numbers` (missing numbers are left out), ordered by
    /// number. One query, for rendering `#N` references.
    pub async fn find_numbers(&self, repo: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, StorageError> {
        if numbers.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<RefRow> = sqlx::query_as(
            "SELECT id, number, kind, state, title FROM issues \
             WHERE repo_id = ? AND number IN (SELECT value FROM json_each(?)) ORDER BY number",
        )
        .bind(repo.0)
        .bind(json_array(numbers.iter().map(|n| i64::from(n.get()))))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(IssueRef::try_from).collect()
    }

    /// The threads among `ids` whose repository `viewer` may read (the shared
    /// `readable_by!` predicate; `viewer_is_admin` for site administrators), for showing
    /// cross-references. Unknown and unreadable ids are left out.
    pub async fn readable_sources(
        &self,
        ids: &[IssueId],
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> Result<Vec<IssueSource>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<SourceRow> = sqlx::query_as(concat!(
            "SELECT i.id, i.number, i.kind, i.state, i.title, o.name AS owner_name, r.name AS repo_name \
             FROM issues i JOIN repositories r ON r.id = i.repo_id JOIN accounts o ON o.id = r.owner_id \
             WHERE i.id IN (SELECT value FROM json_each(?1)) AND ",
            super::repositories::readable_by!("?2", "?3"),
            " ORDER BY i.id"
        ))
        .bind(json_array(ids.iter().map(|id| id.0)))
        .bind(viewer.map(|id| id.0))
        .bind(viewer_is_admin)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| Ok(IssueSource { full_name: format!("{}/{}", row.owner_name, row.repo_name), issue: row.issue.try_into()? }))
            .collect()
    }

    /// Changes the title and/or the body (`None` keeps it). A changed title records a
    /// [`TimelineEvent::Renamed`] by `actor` in the same transaction; `updated_at` moves
    /// forward only when something changed. `None` when the thread does not exist.
    pub async fn update_text(
        &self,
        id: IssueId,
        title: Option<&IssueTitle>,
        body: Option<&IssueText>,
        actor: AccountId,
        at: Timestamp,
    ) -> Result<Option<Issue>, StorageError> {
        // Reads before it writes: take the write lock first (see `Pool::begin`).
        let mut tx = self.pool.begin_immediate().await?;
        let current: Option<(String, String)> =
            sqlx::query_as("SELECT title, body FROM issues WHERE id = ?").bind(id.0).fetch_optional(&mut *tx).await?;
        let Some((old_title, old_body)) = current else {
            return Ok(None);
        };
        let title = title.filter(|title| title.as_str() != old_title);
        let body = body.filter(|body| body.as_str() != old_body);
        if title.is_some() || body.is_some() {
            sqlx::query(
                "UPDATE issues SET title = COALESCE(?, title), body = COALESCE(?, body), updated_at = MAX(updated_at, ?) WHERE id = ?",
            )
            .bind(title.map(IssueTitle::as_str))
            .bind(body.map(IssueText::as_str))
            .bind(at.0)
            .bind(id.0)
            .execute(&mut *tx)
            .await?;
        }
        if let Some(title) = title {
            let event = TimelineEvent::Renamed { from: old_title, to: title.as_str().to_owned() };
            insert_event(&mut tx, id, Some(actor), &event, at).await?;
        }
        let issue = find_in(&mut tx, id).await?;
        tx.commit().await?;
        Ok(issue)
    }

    /// Opens or closes the thread unless it already is in `state` (`None` then: nothing
    /// changed and no event is recorded). Closing needs a `reason` and sets `closed_at`
    /// and `closed_by` (`actor`); reopening clears all three. `event` is recorded in the
    /// same transaction.
    pub async fn set_state(
        &self,
        id: IssueId,
        state: IssueState,
        reason: Option<StateReason>,
        actor: Option<AccountId>,
        event: &TimelineEvent,
        at: Timestamp,
    ) -> Result<Option<Issue>, StorageError> {
        let closing = state == IssueState::Closed;
        let mut tx = self.pool.begin().await?;
        let row: Option<IssueRow> = sqlx::query_as(concat!(
            "UPDATE issues SET state = ?2, state_reason = ?3, closed_at = CASE WHEN ?4 THEN ?5 END, \
             closed_by = CASE WHEN ?4 THEN ?6 END, updated_at = MAX(updated_at, ?5) \
             WHERE id = ?1 AND state <> ?2 RETURNING ",
            issue_columns!()
        ))
        .bind(id.0)
        .bind(state.as_str())
        .bind(reason.filter(|_| closing).map(StateReason::as_str))
        .bind(closing)
        .bind(at.0)
        .bind(actor.map(|a| a.0))
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        insert_event(&mut tx, id, actor, event, at).await?;
        let issue = Issue::try_from(row)?;
        tx.commit().await?;
        Ok(Some(issue))
    }

    /// Locks or unlocks the conversation, recording [`TimelineEvent::Locked`] or
    /// [`TimelineEvent::Unlocked`]. `false` when it already was in that state.
    pub async fn set_locked(&self, id: IssueId, locked: bool, actor: AccountId, at: Timestamp) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE issues SET locked = ?1, updated_at = MAX(updated_at, ?2) WHERE id = ?3 AND locked <> ?1")
            .bind(locked)
            .bind(at.0)
            .bind(id.0)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            > 0;
        if changed {
            let event = if locked { TimelineEvent::Locked } else { TimelineEvent::Unlocked };
            insert_event(&mut tx, id, Some(actor), &event, at).await?;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Moves `updated_at` forward to `at` (for changes stored elsewhere, such as labels).
    pub async fn touch(&self, id: IssueId, at: Timestamp) -> Result<(), StorageError> {
        touch_in(&mut self.pool.acquire().await?, id, at).await
    }

    /// Deletes an issue with its comments and events and returns what it was. Pull
    /// requests are never deleted (`None`, as for a missing id). The number stays taken.
    pub async fn delete(&self, id: IssueId) -> Result<Option<Issue>, StorageError> {
        let row: Option<IssueRow> =
            sqlx::query_as(concat!("DELETE FROM issues WHERE id = ? AND kind = 'issue' RETURNING ", issue_columns!()))
                .bind(id.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Issue::try_from).transpose()
    }

    /// Open `(issues, pull requests)` of `repo`, in one indexed query that reads only the
    /// open entries of `issues_by_repo`.
    pub async fn open_counts(&self, repo: RepoId) -> Result<(u64, u64), StorageError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(OPEN_COUNTS).bind(repo.0).fetch_all(&self.pool).await?;
        let mut counts = (0, 0);
        for (kind, n) in rows {
            match kind.as_str() {
                "issue" => counts.0 = count(n),
                "pull" => counts.1 = count(n),
                _ => {}
            }
        }
        Ok(counts)
    }

    // ------------------------------------------------------------------ comments

    /// Adds a comment, counting it on the thread. `None` when the thread does not exist.
    pub async fn add_comment(
        &self,
        issue: IssueId,
        author: AccountId,
        body: &IssueText,
        at: Timestamp,
    ) -> Result<Option<Comment>, StorageError> {
        self.insert_comment(issue, Some(author), CommentSource::User, body, at).await
    }

    // Actions: task tokens.
    /// Adds a comment a workflow task wrote, with no author and `source = 'actions'`,
    /// counting it on the thread. `None` when the thread does not exist.
    pub async fn add_actions_comment(&self, issue: IssueId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, StorageError> {
        self.insert_comment(issue, None, CommentSource::Actions, body, at).await
    }

    async fn insert_comment(
        &self,
        issue: IssueId,
        author: Option<AccountId>,
        source: CommentSource,
        body: &IssueText,
        at: Timestamp,
    ) -> Result<Option<Comment>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE issues SET comment_count = comment_count + 1, updated_at = MAX(updated_at, ?) WHERE id = ?")
            .bind(at.0)
            .bind(issue.0)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if updated == 0 {
            return Ok(None);
        }
        let row: CommentRow = sqlx::query_as(concat!(
            "INSERT INTO issue_comments (issue_id, author_id, body, created_at, updated_at, source) VALUES (?, ?, ?, ?, ?, ?) RETURNING ",
            comment_columns!()
        ))
        .bind(issue.0)
        .bind(author.map(|author| author.0))
        .bind(body.as_str())
        .bind(at.0)
        .bind(at.0)
        .bind(source.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let comment = Comment::try_from(row)?;
        tx.commit().await?;
        Ok(Some(comment))
    }

    /// Comment `id`, only if it belongs to `issue`.
    pub async fn find_comment(&self, issue: IssueId, id: CommentId) -> Result<Option<Comment>, StorageError> {
        let row: Option<CommentRow> =
            sqlx::query_as(concat!("SELECT ", comment_columns!(), " FROM issue_comments WHERE id = ? AND issue_id = ?"))
                .bind(id.0)
                .bind(issue.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Comment::try_from).transpose()
    }

    /// Replaces a comment's text, marking it edited at `at`. `None` when it does not exist.
    pub async fn edit_comment(&self, id: CommentId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let row: Option<CommentRow> = sqlx::query_as(concat!(
            "UPDATE issue_comments SET body = ?1, updated_at = MAX(updated_at, ?2), edited_at = ?2 WHERE id = ?3 RETURNING ",
            comment_columns!()
        ))
        .bind(body.as_str())
        .bind(at.0)
        .bind(id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let comment = Comment::try_from(row)?;
        touch_in(&mut tx, comment.issue_id, at).await?;
        tx.commit().await?;
        Ok(Some(comment))
    }

    /// Deletes a comment, uncounting it on the thread, and returns what it was.
    pub async fn delete_comment(&self, id: CommentId) -> Result<Option<Comment>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let row: Option<CommentRow> = sqlx::query_as(concat!("DELETE FROM issue_comments WHERE id = ? RETURNING ", comment_columns!()))
            .bind(id.0)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let comment = Comment::try_from(row)?;
        sqlx::query("UPDATE issues SET comment_count = MAX(comment_count - 1, 0), updated_at = MAX(updated_at, ?) WHERE id = ?")
            .bind(Timestamp::now().0)
            .bind(comment.issue_id.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(comment))
    }

    // ------------------------------------------------------------------ the timeline

    /// Records `event` on the thread (moving `updated_at` forward).
    pub async fn record_event(
        &self,
        issue: IssueId,
        actor: Option<AccountId>,
        event: &TimelineEvent,
        at: Timestamp,
    ) -> Result<EventId, StorageError> {
        let mut tx = self.pool.begin().await?;
        let id = insert_event(&mut tx, issue, actor, event, at).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Up to `limit` comments and events after `after` (from the start when `None`),
    /// merged in `(created_at, comments before events, id)` order with their accounts.
    pub async fn timeline(&self, issue: IssueId, after: Option<TimelineCursor>, limit: u32) -> Result<TimelinePage, StorageError> {
        if limit == 0 {
            return Ok(TimelinePage::default());
        }
        let (at, rank, id) = after.map_or((i64::MIN, 0, 0), TimelineCursor::key);
        let rows: Vec<TimelineRow> = sqlx::query_as(timeline_page!())
            .bind(issue.0)
            .bind(at)
            .bind(rank)
            .bind(id)
            .bind(i64::from(limit) + 1)
            .fetch_all(&self.pool)
            .await?;
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let next = (rows.len() > limit).then(|| rows[limit - 1].cursor());
        let mut items = Vec::with_capacity(rows.len().min(limit));
        for row in rows.into_iter().take(limit) {
            if let Some(item) = row.into_item(issue)? {
                items.push(item);
            }
        }
        Ok(TimelinePage { items, next })
    }
}

async fn find_in(conn: &mut Conn, id: IssueId) -> Result<Option<Issue>, StorageError> {
    let row: Option<IssueRow> =
        sqlx::query_as(concat!("SELECT ", issue_columns!(), " FROM issues WHERE id = ?")).bind(id.0).fetch_optional(&mut *conn).await?;
    row.map(Issue::try_from).transpose()
}

/// Inserts an event and moves the thread's `updated_at` forward.
pub(super) async fn insert_event(
    conn: &mut Conn,
    issue: IssueId,
    actor: Option<AccountId>,
    event: &TimelineEvent,
    at: Timestamp,
) -> Result<EventId, StorageError> {
    let (kind, data) = encode_event(event)?;
    let id: i64 =
        sqlx::query_scalar("INSERT INTO issue_events (issue_id, actor_id, kind, data, created_at) VALUES (?, ?, ?, ?, ?) RETURNING id")
            .bind(issue.0)
            .bind(actor.map(|a| a.0))
            .bind(kind)
            .bind(data)
            .bind(at.0)
            .fetch_one(&mut *conn)
            .await?;
    touch_in(conn, issue, at).await?;
    Ok(EventId(id))
}

async fn touch_in(conn: &mut Conn, issue: IssueId, at: Timestamp) -> Result<(), StorageError> {
    sqlx::query("UPDATE issues SET updated_at = MAX(updated_at, ?) WHERE id = ?").bind(at.0).bind(issue.0).execute(&mut *conn).await?;
    Ok(())
}

// ------------------------------------------------------------------ imported history
// Milestone 3: importing (M). Threads copied from another forge keep their numbers,
// states and timestamps; they are written here so this file stays the only writer of
// the core tables.

/// How an imported thread was closed.
#[derive(Debug, Clone, Copy)]
pub struct ImportedClosing {
    pub at: Timestamp,
    pub reason: StateReason,
    pub by: Option<AccountId>,
}

/// A comment of an imported thread, with its original times.
#[derive(Debug, Clone)]
pub struct ImportedComment {
    pub author: AccountId,
    pub body: IssueText,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A timeline event of an imported thread.
#[derive(Debug, Clone)]
pub struct ImportedEvent {
    pub actor: Option<AccountId>,
    pub event: TimelineEvent,
    pub at: Timestamp,
}

/// An issue or pull request thread as it was on the source, numbered as it was there.
#[derive(Debug, Clone)]
pub struct ImportedThread {
    pub repo: RepoId,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub title: IssueTitle,
    pub body: IssueText,
    pub author: AccountId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub closed: Option<ImportedClosing>,
    pub locked: bool,
    pub comments: Vec<ImportedComment>,
    pub events: Vec<ImportedEvent>,
}

impl IssueStore {
    /// Makes sure numbers handed out from now on are above `highest`, so threads created
    /// here while an import runs never take a number the import still has to write.
    pub async fn reserve_numbers(&self, repo: RepoId, highest: IssueNumber) -> Result<(), StorageError> {
        reserve_in(&mut self.pool.acquire().await?, repo, highest).await
    }

    /// Inserts an imported thread with its number, state, comments and events in one
    /// transaction. [`StorageError::Conflict`] (`"issue number"`) when the number is taken.
    pub async fn insert_imported(&self, thread: &ImportedThread) -> Result<Issue, StorageError> {
        let mut tx = self.pool.begin().await?;
        let issue = Self::insert_imported_in(&mut tx, thread).await?;
        tx.commit().await?;
        Ok(issue)
    }

    /// [`Self::insert_imported`] inside a caller's transaction (an imported pull request
    /// adds its own row in the same transaction).
    pub(crate) async fn insert_imported_in(conn: &mut Conn, thread: &ImportedThread) -> Result<Issue, StorageError> {
        reserve_in(conn, thread.repo, thread.number).await?;
        let closed = thread.closed.as_ref();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO issues (repo_id, number, kind, title, body, author_id, state, state_reason, closed_at, closed_by, locked, \
             created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(thread.repo.0)
        .bind(i64::from(thread.number.get()))
        .bind(thread.kind.as_str())
        .bind(thread.title.as_str())
        .bind(thread.body.as_str())
        .bind(thread.author.0)
        .bind(if closed.is_some() { IssueState::Closed } else { IssueState::Open }.as_str())
        .bind(closed.map(|c| c.reason.as_str()))
        .bind(closed.map(|c| c.at.0))
        .bind(closed.and_then(|c| c.by).map(|a| a.0))
        .bind(thread.locked)
        .bind(thread.created_at.0)
        .bind(thread.created_at.0)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| StorageError::from_insert(e, "issue number"))?;
        let issue = IssueId(id);
        for comment in &thread.comments {
            sqlx::query("INSERT INTO issue_comments (issue_id, author_id, body, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
                .bind(id)
                .bind(comment.author.0)
                .bind(comment.body.as_str())
                .bind(comment.created_at.0)
                .bind(comment.updated_at.0.max(comment.created_at.0))
                .execute(&mut *conn)
                .await?;
        }
        for event in &thread.events {
            insert_event(conn, issue, event.actor, &event.event, event.at).await?;
        }
        let count = i64::try_from(thread.comments.len()).unwrap_or(i64::MAX);
        sqlx::query("UPDATE issues SET comment_count = ?, updated_at = MAX(updated_at, ?) WHERE id = ?")
            .bind(count)
            .bind(thread.updated_at.0)
            .bind(id)
            .execute(&mut *conn)
            .await?;
        find_in(conn, issue).await?.ok_or_else(|| corrupt("imported issue", "vanished"))
    }
}

async fn reserve_in(conn: &mut Conn, repo: RepoId, highest: IssueNumber) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO issue_counters (repo_id, last_number) VALUES (?1, ?2) \
         ON CONFLICT (repo_id) DO UPDATE SET last_number = MAX(last_number, ?2)",
    )
    .bind(repo.0)
    .bind(i64::from(highest.get()))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::plans;

    #[tokio::test]
    async fn a_thread_and_its_author_are_found_by_key() {
        let (_dir, database) = plans::database().await;
        let plan = plans::query_plan(&database, ISSUE_WITH_AUTHOR).await;
        assert!(plan[0].starts_with("SEARCH i USING INDEX ") && plan[0].ends_with(" (repo_id=? AND number=?)"), "{plan:?}");
        let author = plan.iter().filter(|line| *line == "SEARCH accounts USING INTEGER PRIMARY KEY (rowid=?)").count();
        assert_eq!(author, 5, "each author column by key: {plan:?}");
        plans::assert_no_scans(&database, ISSUE_WITH_AUTHOR, &[]).await;
    }

    #[tokio::test]
    async fn open_counts_read_only_open_index_entries() {
        let (_dir, database) = plans::database().await;
        let plan = plans::query_plan(&database, OPEN_COUNTS).await;
        // Before: `SEARCH issues USING COVERING INDEX issues_by_repo (repo_id=?)`.
        assert_eq!(plan.len(), 1, "{plan:?}");
        assert!(plan[0].starts_with("SEARCH issues USING COVERING INDEX "), "{plan:?}");
        assert!(plan[0].ends_with(" (repo_id=? AND kind=? AND state=?)"), "{plan:?}");
    }

    #[test]
    fn cursors_round_trip_through_urls() {
        for cursor in [
            TimelineCursor { at: Timestamp(1_760_000_000), item: TimelineItemId::Comment(CommentId(42)) },
            TimelineCursor { at: Timestamp(0), item: TimelineItemId::Event(EventId(7)) },
        ] {
            assert_eq!(TimelineCursor::parse(&cursor.to_param()), Some(cursor));
        }
        assert_eq!(TimelineCursor::parse("1760000000-c42").unwrap().item, TimelineItemId::Comment(CommentId(42)));
        for bad in ["", "1", "1-", "1-x5", "1-c", "1-c0", "-1-c5", "a-c5", "1-c-5", "1-e5x"] {
            assert_eq!(TimelineCursor::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn events_encode_as_kind_and_data() {
        assert_eq!(encode_event(&TimelineEvent::Reopened).unwrap(), ("reopened", "{}".to_owned()));
        let renamed = TimelineEvent::Renamed { from: "a".to_owned(), to: "b".to_owned() };
        let (kind, data) = encode_event(&renamed).unwrap();
        assert_eq!((kind, data.as_str()), ("renamed", r#"{"from":"a","to":"b"}"#));
        assert_eq!(decode_event(kind, &data), Some(renamed));
        assert_eq!(decode_event("reopened", "{}"), Some(TimelineEvent::Reopened));
        assert_eq!(decode_event("teleported", "{}"), None, "unknown kinds are skipped");
        assert_eq!(decode_event("renamed", r#"{"from":"a"}"#), None, "data that does not fit is skipped");
        assert_eq!(decode_event("renamed", "not json"), None);
    }
}
