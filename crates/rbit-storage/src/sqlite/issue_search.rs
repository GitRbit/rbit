//! Issue and pull request lists: filters, sorting, counts per tab and full-text search (FTS5).
//!
//! Milestone 2 workstream I (issues) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! One static query takes every filter as a parameter (an unused filter is `NULL` or
//! `0`), so sqlx gets `'static` SQL; the five sort orders are five copies of it. Label
//! lists are bound as JSON arrays for `json_each(?)`. Free text goes through
//! [`fts_expression`], which turns user input into a safe FTS5 query.

use rbit_core::{AccountId, Issue, IssueKind, IssueState, RepoId, issue_query::IssueSort};
use sqlx::SqlitePool;

use crate::StorageError;

/// The filters of one list. Accounts and names are already resolved by the caller: an
/// author or assignee that does not exist is an id no row has (such as `AccountId(0)`),
/// so the list is empty.
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent filters")]
pub struct SearchFilter {
    pub repo: RepoId,
    pub kind: Option<IssueKind>,
    pub state: Option<IssueState>,
    /// Closed as merged (pull requests).
    pub merged: bool,
    /// Draft pull requests. Draft state lives in the pull request workstream's tables;
    /// until it adopts this store, a draft filter matches nothing.
    pub draft: bool,
    pub locked: bool,
    pub author: Option<AccountId>,
    pub assignee: Option<AccountId>,
    pub no_assignee: bool,
    /// Label names that must all be present (case-insensitive).
    pub labels: Vec<String>,
    /// Label names that must be absent.
    pub not_labels: Vec<String>,
    pub no_label: bool,
    /// A milestone title (case-insensitive).
    pub milestone: Option<String>,
    pub no_milestone: bool,
    /// Free-text words (title and body), the last one matched as a prefix.
    pub text: Vec<String>,
    pub sort: IssueSort,
}

impl SearchFilter {
    /// Everything in `repo`, newest first.
    pub fn new(repo: RepoId) -> Self {
        Self {
            repo,
            kind: None,
            state: None,
            merged: false,
            draft: false,
            locked: false,
            author: None,
            assignee: None,
            no_assignee: false,
            labels: Vec::new(),
            not_labels: Vec::new(),
            no_label: false,
            milestone: None,
            no_milestone: false,
            text: Vec::new(),
            sort: IssueSort::default(),
        }
    }
}

/// The FTS5 query for `words`: each word's letter-and-digit runs as quoted terms, all
/// required, the very last one as a prefix. `None` when nothing searchable is left.
pub fn fts_expression(words: &[String]) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in words {
        terms.extend(word.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(|t| format!("\"{t}\"")));
    }
    let last = terms.last_mut()?;
    last.push('*');
    Some(terms.join(" "))
}

/// `WHERE` clause of every list query, with numbered parameters:
/// `?1` repo, `?2` kind, `?3` state, `?4` merged, `?5` draft, `?6` locked, `?7` author,
/// `?8` assignee, `?9` no assignee, `?10` labels (JSON), `?11` excluded labels (JSON),
/// `?12` no label, `?13` milestone title, `?14` no milestone, `?15` FTS expression.
macro_rules! search_where {
    () => {
        "WHERE i.repo_id = ?1 \
         AND (?2 IS NULL OR i.kind = ?2) \
         AND (?3 IS NULL OR i.state = ?3) \
         AND (?4 = 0 OR i.state_reason = 'merged') \
         AND (?5 = 0) \
         AND (?6 = 0 OR i.locked = 1) \
         AND (?7 IS NULL OR i.author_id = ?7) \
         AND (?8 IS NULL OR EXISTS (SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.user_id = ?8)) \
         AND (?9 = 0 OR NOT EXISTS (SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id)) \
         AND (SELECT COUNT(*) FROM json_each(?10) j WHERE EXISTS (SELECT 1 FROM issue_labels il JOIN labels l ON l.id = il.label_id \
              WHERE il.issue_id = i.id AND l.name = j.value)) = json_array_length(?10) \
         AND NOT EXISTS (SELECT 1 FROM issue_labels il JOIN labels l ON l.id = il.label_id \
              WHERE il.issue_id = i.id AND l.name IN (SELECT value FROM json_each(?11))) \
         AND (?12 = 0 OR NOT EXISTS (SELECT 1 FROM issue_labels il WHERE il.issue_id = i.id)) \
         AND (?13 IS NULL OR EXISTS (SELECT 1 FROM issue_milestones im JOIN milestones m ON m.id = im.milestone_id \
              WHERE im.issue_id = i.id AND m.title = ?13)) \
         AND (?14 = 0 OR NOT EXISTS (SELECT 1 FROM issue_milestones im WHERE im.issue_id = i.id)) \
         AND (?15 IS NULL OR i.id IN (SELECT rowid FROM issues_fts WHERE issues_fts MATCH ?15)) "
    };
}

/// One page in the given order: `?16` limit, `?17` offset.
macro_rules! search_page {
    ($order:literal) => {
        concat!(
            "SELECT i.id, i.repo_id, i.number, i.kind, i.title, i.body, i.author_id, i.state, i.state_reason, i.closed_at, \
             i.closed_by, i.locked, i.comment_count, i.created_at, i.updated_at FROM issues i ",
            search_where!(),
            "ORDER BY ",
            $order,
            " LIMIT ?16 OFFSET ?17"
        )
    };
}

/// Binds `?1`–`?15` of [`search_where!`] (with `state` overridden by the caller).
macro_rules! bind_filter {
    ($query:expr, $filter:expr, $state:expr, $fts:expr) => {
        $query
            .bind($filter.repo.0)
            .bind($filter.kind.map(IssueKind::as_str))
            .bind($state.map(IssueState::as_str))
            .bind($filter.merged)
            .bind($filter.draft)
            .bind($filter.locked)
            .bind($filter.author.map(|a| a.0))
            .bind($filter.assignee.map(|a| a.0))
            .bind($filter.no_assignee)
            .bind(json_strings(&$filter.labels))
            .bind(json_strings(&$filter.not_labels))
            .bind($filter.no_label)
            .bind($filter.milestone.as_deref())
            .bind($filter.no_milestone)
            .bind($fts)
    };
}

fn json_strings(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

/// The issue row as selected by [`search_page!`]: the same columns as `IssueStore`'s,
/// read through its conversion.
#[derive(Debug, sqlx::FromRow)]
struct Row {
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

impl TryFrom<Row> for Issue {
    type Error = StorageError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        use rbit_core::{IssueId, IssueNumber, IssueText, IssueTitle, StateReason, Timestamp};
        let corrupt = |what: &str, detail: String| StorageError::Corrupt(format!("{what}: {detail}"));
        Ok(Self {
            id: IssueId(row.id),
            repo_id: RepoId(row.repo_id),
            number: IssueNumber::try_from(row.number).map_err(|e| corrupt("issue number", e.to_string()))?,
            kind: IssueKind::parse(&row.kind).ok_or_else(|| corrupt("issue kind", row.kind.clone()))?,
            title: IssueTitle::parse(&row.title).map_err(|e| corrupt("issue title", e.to_string()))?,
            body: IssueText::body(&row.body).map_err(|e| corrupt("issue body", e.to_string()))?,
            author: row.author_id.map(AccountId),
            state: IssueState::parse(&row.state).ok_or_else(|| corrupt("issue state", row.state.clone()))?,
            state_reason: row
                .state_reason
                .map(|reason| StateReason::parse(&reason).ok_or_else(|| corrupt("state reason", reason.clone())))
                .transpose()?,
            closed_at: row.closed_at.map(Timestamp),
            closed_by: row.closed_by.map(AccountId),
            locked: row.locked,
            comment_count: u32::try_from(row.comment_count).map_err(|e| corrupt("comment count", e.to_string()))?,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        })
    }
}

#[derive(Debug, Clone)]
pub struct IssueSearchStore {
    pool: SqlitePool,
}

impl IssueSearchStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// One page of the threads matching `filter`, in its order (ties by number).
    pub async fn search(&self, filter: &SearchFilter, limit: u32, offset: u64) -> Result<Vec<Issue>, StorageError> {
        let sql = match filter.sort {
            IssueSort::CreatedDesc => search_page!("i.created_at DESC, i.number DESC"),
            IssueSort::CreatedAsc => search_page!("i.created_at ASC, i.number ASC"),
            IssueSort::UpdatedDesc => search_page!("i.updated_at DESC, i.number DESC"),
            IssueSort::UpdatedAsc => search_page!("i.updated_at ASC, i.number ASC"),
            IssueSort::CommentsDesc => search_page!("i.comment_count DESC, i.number DESC"),
        };
        let fts = fts_expression(&filter.text);
        let rows: Vec<Row> = bind_filter!(sqlx::query_as(sql), filter, filter.state, fts)
            .bind(i64::from(limit))
            .bind(i64::try_from(offset).unwrap_or(i64::MAX))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Issue::try_from).collect()
    }

    /// How many threads match `filter`, ignoring its state: `(open, closed)`, for the tabs.
    pub async fn state_counts(&self, filter: &SearchFilter) -> Result<(u64, u64), StorageError> {
        let fts = fts_expression(&filter.text);
        let (open, closed): (i64, i64) = bind_filter!(
            sqlx::query_as(concat!(
                "SELECT COALESCE(SUM(i.state = 'open'), 0), COALESCE(SUM(i.state = 'closed'), 0) FROM issues i ",
                search_where!()
            )),
            filter,
            None::<IssueState>,
            fts
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((count(open), count(closed)))
    }

    /// How many threads match `filter` (its state included).
    pub async fn count(&self, filter: &SearchFilter) -> Result<u64, StorageError> {
        let (open, closed) = self.state_counts(filter).await?;
        Ok(match filter.state {
            Some(IssueState::Open) => open,
            Some(IssueState::Closed) => closed,
            None => open + closed,
        })
    }

    /// Whether this SQLite build has FTS5 (the bundled one does; migrations need it).
    pub async fn has_fts5(&self) -> Result<bool, StorageError> {
        let enabled: i64 = sqlx::query_scalar("SELECT sqlite_compileoption_used('ENABLE_FTS5')").fetch_one(&self.pool).await?;
        Ok(enabled == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(input: &[&str]) -> Vec<String> {
        input.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn fts_expressions_quote_terms_and_prefix_the_last() {
        assert_eq!(fts_expression(&words(&[])), None);
        assert_eq!(fts_expression(&words(&["--", "\"*"])), None);
        assert_eq!(fts_expression(&words(&["crash"])).as_deref(), Some("\"crash\"*"));
        assert_eq!(fts_expression(&words(&["boot", "cra"])).as_deref(), Some("\"boot\" \"cra\"*"));
        assert_eq!(fts_expression(&words(&["NEAR(a", "b)", "OR"])).as_deref(), Some("\"NEAR\" \"a\" \"b\" \"OR\"*"));
        assert_eq!(fts_expression(&words(&["über-crash"])).as_deref(), Some("\"über\" \"crash\"*"));
    }
}
