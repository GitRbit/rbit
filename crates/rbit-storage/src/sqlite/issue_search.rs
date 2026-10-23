//! Issue and pull request lists: filters, sorting, counts per tab and full-text search (FTS5).
//!
//! Milestone 2 workstream I (issues) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! The `WHERE` clause is built per list with [`QueryBuilder`] and holds only the filters
//! in use, each a bound parameter. A catch-all form (`?N IS NULL OR col = ?N`) would keep
//! `SQLite` from using `kind` and `state` as index equalities and evaluate every unused
//! filter's subquery on every row. The default list (one kind, one state, newest first)
//! then walks `issues_by_repo_created` and stops at the page's `LIMIT`. Dashboards start
//! from the viewer's indexed relationship candidates and apply current visibility. Label
//! lists are bound as one JSON array for `json_each(?)` however long they are. Free text
//! goes through [`fts_expression`], which turns user input into a safe FTS5 query.

use rbit_core::{AccountId, Issue, IssueKind, IssueState, RepoId, issue_query::IssueSort};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::repositories::readable_by;
use crate::StorageError;

/// Personal relationship used by the cross-repository dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardRelation {
    Created,
    Assigned,
    Mentioned,
    ReviewRequested,
}

impl DashboardRelation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Assigned => "assigned",
            Self::Mentioned => "mentioned",
            Self::ReviewRequested => "review-requested",
        }
    }
}

/// The signed-in viewer and their selected relationship; visibility is evaluated in SQL.
#[derive(Debug, Clone, Copy)]
pub struct DashboardScope {
    pub viewer: AccountId,
    pub is_admin: bool,
    pub relation: DashboardRelation,
}

/// The filters of one list. Accounts and names are already resolved by the caller: an
/// author or assignee that does not exist is an id no row has (such as `AccountId(0)`),
/// so the list is empty.
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent filters")]
pub struct SearchFilter {
    pub repo: RepoId,
    /// When present, search all readable repositories instead of `repo`.
    pub dashboard: Option<DashboardScope>,
    pub kind: Option<IssueKind>,
    pub state: Option<IssueState>,
    /// Closed as merged (pull requests).
    pub merged: bool,
    /// Draft pull requests (`pull_requests.is_draft`).
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
    /// Only threads with at least one assignee (the API's `assignee=*`).
    pub any_assignee: bool,
    /// Only threads in some milestone (the API's `milestone=*`).
    pub any_milestone: bool,
    /// Only threads updated at or after this time (the API's `since`).
    pub updated_since: Option<rbit_core::Timestamp>,
}

impl SearchFilter {
    /// Everything in `repo`, newest first.
    pub fn new(repo: RepoId) -> Self {
        Self {
            repo,
            dashboard: None,
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
            any_assignee: false,
            any_milestone: false,
            updated_since: None,
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

/// Selected by [`search_query`], in the order of [`Row`].
const ISSUE_COLUMNS: &str = "SELECT i.id, i.repo_id, i.number, i.kind, i.title, i.body, i.author_id, i.state, i.state_reason, \
     i.closed_at, i.closed_by, i.locked, i.comment_count, i.created_at, i.updated_at";

/// Appends `FROM issues i … WHERE …` with only the filters `filter` uses (and `state`
/// instead of its own). Every value is a bound parameter; the SQL text only depends on
/// which filters are present.
fn push_filter(query: &mut QueryBuilder<Sqlite>, filter: &SearchFilter, state: Option<IssueState>) {
    match filter.dashboard {
        Some(scope) => push_dashboard(query, scope),
        None => {
            query.push(" FROM issues i WHERE i.repo_id = ").push_bind(filter.repo.0);
        }
    }
    if let Some(kind) = filter.kind {
        query.push(" AND i.kind = ").push_bind(kind.as_str());
    }
    if let Some(state) = state {
        query.push(" AND i.state = ").push_bind(state.as_str());
    }
    if filter.merged {
        query.push(" AND i.state_reason = 'merged'");
    }
    if filter.draft {
        query.push(" AND EXISTS (SELECT 1 FROM pull_requests p WHERE p.issue_id = i.id AND p.is_draft = 1)");
    }
    if filter.locked {
        query.push(" AND i.locked = 1");
    }
    if let Some(author) = filter.author {
        query.push(" AND i.author_id = ").push_bind(author.0);
    }
    if let Some(assignee) = filter.assignee {
        query
            .push(" AND EXISTS (SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.user_id = ")
            .push_bind(assignee.0)
            .push(")");
    }
    if filter.no_assignee {
        query.push(" AND NOT EXISTS (SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id)");
    }
    if filter.any_assignee {
        query.push(" AND EXISTS (SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id)");
    }
    if !filter.labels.is_empty() {
        // No requested label is missing (`labels.name` compares case-insensitively).
        query.push(" AND NOT EXISTS (SELECT 1 FROM json_each(").push_bind(json_strings(&filter.labels)).push(
            ") j WHERE NOT EXISTS (SELECT 1 FROM issue_labels il JOIN labels l ON l.id = il.label_id \
                 WHERE il.issue_id = i.id AND l.name = j.value))",
        );
    }
    if !filter.not_labels.is_empty() {
        query
            .push(" AND NOT EXISTS (SELECT 1 FROM issue_labels il JOIN labels l ON l.id = il.label_id WHERE il.issue_id = i.id AND l.name IN (SELECT value FROM json_each(")
            .push_bind(json_strings(&filter.not_labels))
            .push(")))");
    }
    if filter.no_label {
        query.push(" AND NOT EXISTS (SELECT 1 FROM issue_labels il WHERE il.issue_id = i.id)");
    }
    if let Some(milestone) = &filter.milestone {
        query
            .push(" AND EXISTS (SELECT 1 FROM issue_milestones im JOIN milestones m ON m.id = im.milestone_id WHERE im.issue_id = i.id AND m.title = ")
            .push_bind(milestone.clone())
            .push(")");
    }
    if filter.no_milestone {
        query.push(" AND NOT EXISTS (SELECT 1 FROM issue_milestones im WHERE im.issue_id = i.id)");
    }
    if filter.any_milestone {
        query.push(" AND EXISTS (SELECT 1 FROM issue_milestones im WHERE im.issue_id = i.id)");
    }
    if let Some(fts) = fts_expression(&filter.text) {
        query.push(" AND i.id IN (SELECT rowid FROM issues_fts WHERE issues_fts MATCH ").push_bind(fts).push(")");
    }
    if let Some(since) = filter.updated_since {
        query.push(" AND i.updated_at >= ").push_bind(since.0);
    }
}

/// Appends `FROM issues i JOIN repositories r … WHERE` for the threads `scope`'s viewer
/// has the selected relationship with, in repositories they can currently read.
fn push_dashboard(query: &mut QueryBuilder<Sqlite>, scope: DashboardScope) {
    // `readable_by!` takes SQL for its two values; mark them and bind each in place.
    const ADMIN: char = '\u{1}';
    const VIEWER: char = '\u{2}';
    query.push(" FROM issues i JOIN repositories r ON r.id = i.repo_id WHERE ");
    let readable = readable_by!("\u{2}", "\u{1}");
    let mut rest = readable;
    while let Some(at) = rest.find([ADMIN, VIEWER]) {
        query.push(&rest[..at]);
        if rest[at..].starts_with(ADMIN) {
            query.push_bind(scope.is_admin);
        } else {
            query.push_bind(scope.viewer.0);
        }
        rest = &rest[at + 1..];
    }
    query.push(rest);
    query
        .push(" AND EXISTS (SELECT 1 FROM user_logins ul WHERE ul.account_id = ")
        .push_bind(scope.viewer.0)
        .push(" AND ul.suspended_at IS NULL)");
    query.push(match scope.relation {
        DashboardRelation::Created => " AND i.author_id = ",
        DashboardRelation::Assigned => " AND i.id IN (SELECT issue_id FROM issue_assignees WHERE user_id = ",
        DashboardRelation::Mentioned => " AND i.id IN (SELECT issue_id FROM issue_mentions WHERE user_id = ",
        DashboardRelation::ReviewRequested => " AND i.id IN (SELECT pull_id FROM pull_review_requests WHERE user_id = ",
    });
    query.push_bind(scope.viewer.0);
    if scope.relation != DashboardRelation::Created {
        query.push(")");
    }
}

/// One page of the threads matching `filter`, in its order (ties by globally unique id).
fn search_query(filter: &SearchFilter, limit: u32, offset: u64) -> QueryBuilder<Sqlite> {
    let mut query = QueryBuilder::new(ISSUE_COLUMNS);
    push_filter(&mut query, filter, filter.state);
    query.push(match filter.sort {
        IssueSort::CreatedDesc => " ORDER BY i.created_at DESC, i.id DESC",
        IssueSort::CreatedAsc => " ORDER BY i.created_at ASC, i.id ASC",
        IssueSort::UpdatedDesc => " ORDER BY i.updated_at DESC, i.id DESC",
        IssueSort::UpdatedAsc => " ORDER BY i.updated_at ASC, i.id ASC",
        IssueSort::CommentsDesc => " ORDER BY i.comment_count DESC, i.id DESC",
    });
    query.push(" LIMIT ").push_bind(i64::from(limit)).push(" OFFSET ").push_bind(i64::try_from(offset).unwrap_or(i64::MAX));
    query
}

/// How many threads match `filter` per state, ignoring its own state.
fn state_counts_query(filter: &SearchFilter) -> QueryBuilder<Sqlite> {
    let mut query = QueryBuilder::new("SELECT i.state, COUNT(*)");
    push_filter(&mut query, filter, None);
    query.push(" GROUP BY i.state");
    query
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

    /// Record known mentions without tying them to inbox retention or subscription state.
    pub async fn record_mentions(&self, issue: rbit_core::IssueId, users: &[AccountId]) -> Result<(), StorageError> {
        let ids: Vec<i64> = users.iter().map(|id| id.0).collect();
        sqlx::query("INSERT OR IGNORE INTO issue_mentions (issue_id, user_id) SELECT ?1, value FROM json_each(?2) WHERE EXISTS (SELECT 1 FROM issues WHERE id = ?1)")
            .bind(issue.0).bind(serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned())).execute(&self.pool).await?;
        Ok(())
    }

    /// Whether the one-time Markdown-aware backfill has completed.
    pub async fn mentions_indexed(&self) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT complete FROM dashboard_mention_index WHERE id = 1").fetch_one(&self.pool).await?)
    }

    pub async fn finish_mentions_index(&self) -> Result<(), StorageError> {
        sqlx::query("UPDATE dashboard_mention_index SET complete = 1 WHERE id = 1").execute(&self.pool).await?;
        Ok(())
    }

    /// Historical bodies in bounded batches; pending reviews remain private.
    pub async fn mention_sources(&self, source: u8, after: i64) -> Result<Vec<(i64, i64, String)>, StorageError> {
        let sql = match source {
            0 => "SELECT id, id, body FROM issues WHERE id > ? ORDER BY id LIMIT 100",
            1 => "SELECT id, issue_id, body FROM issue_comments WHERE id > ? ORDER BY id LIMIT 100",
            2 => "SELECT id, pull_id, body FROM pull_reviews WHERE id > ? AND state <> 'pending' ORDER BY id LIMIT 100",
            _ => {
                "SELECT c.id, c.pull_id, c.body FROM pull_review_comments c JOIN pull_reviews r ON r.id = c.review_id WHERE c.id > ? AND r.state <> 'pending' ORDER BY c.id LIMIT 100"
            }
        };
        Ok(sqlx::query_as(sql).bind(after).fetch_all(&self.pool).await?)
    }

    /// One page of the threads matching `filter`, in its order (ties by globally unique issue id).
    pub async fn search(&self, filter: &SearchFilter, limit: u32, offset: u64) -> Result<Vec<Issue>, StorageError> {
        let rows: Vec<Row> = search_query(filter, limit, offset).build_query_as().fetch_all(&self.pool).await?;
        rows.into_iter().map(Issue::try_from).collect()
    }

    /// How many threads match `filter`, ignoring its state: `(open, closed)`, for the tabs.
    pub async fn state_counts(&self, filter: &SearchFilter) -> Result<(u64, u64), StorageError> {
        let rows: Vec<(String, i64)> = state_counts_query(filter).build_query_as().fetch_all(&self.pool).await?;
        let mut counts = (0, 0);
        for (state, n) in rows {
            match IssueState::parse(&state) {
                Some(IssueState::Open) => counts.0 = count(n),
                Some(IssueState::Closed) => counts.1 = count(n),
                None => {}
            }
        }
        Ok(counts)
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

    /// Whether this `SQLite` build has FTS5 (the bundled one does; migrations need it).
    pub async fn has_fts5(&self) -> Result<bool, StorageError> {
        let enabled: i64 = sqlx::query_scalar("SELECT sqlite_compileoption_used('ENABLE_FTS5')").fetch_one(&self.pool).await?;
        Ok(enabled == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::plans;

    fn words(input: &[&str]) -> Vec<String> {
        input.iter().map(|w| (*w).to_owned()).collect()
    }

    fn open_issues() -> SearchFilter {
        SearchFilter { kind: Some(IssueKind::Issue), state: Some(IssueState::Open), ..SearchFilter::new(RepoId(1)) }
    }

    #[test]
    fn unused_filters_add_no_sql() {
        let sql = search_query(&open_issues(), 25, 0).into_string();
        for absent in ["EXISTS", "json_each", "issues_fts", "IS NULL", "author_id =", "updated_at >="] {
            assert!(!sql.contains(absent), "{absent} in {sql}");
        }
        let filtered = SearchFilter { author: Some(AccountId(2)), labels: words(&["bug"]), ..open_issues() };
        let sql = search_query(&filtered, 25, 0).into_string();
        assert!(sql.contains("i.author_id = ") && sql.contains("json_each("), "{sql}");
        assert!(!sql.contains("issue_milestones"), "{sql}");
    }

    #[test]
    fn dashboard_values_are_all_bound_in_place() {
        for relation in
            [DashboardRelation::Created, DashboardRelation::Assigned, DashboardRelation::Mentioned, DashboardRelation::ReviewRequested]
        {
            let scope = DashboardScope { viewer: AccountId(7), is_admin: false, relation };
            let sql = search_query(&SearchFilter { dashboard: Some(scope), ..open_issues() }, 25, 0).into_string();
            assert!(!sql.contains(['\u{1}', '\u{2}']), "{sql}");
            assert!(!sql.contains("i.repo_id = ?"), "dashboards search every readable repository: {sql}");
            assert!(sql.contains("r.owner_id = ?") && sql.contains("suspended_at IS NULL"), "{sql}");
        }
    }

    #[tokio::test]
    async fn the_default_page_walks_an_index_in_order() {
        let (_dir, database) = plans::database().await;
        // Before (catch-all predicates): `SEARCH i USING INDEX sqlite_autoindex_issues_1
        // (repo_id=?)`, thirteen correlated subqueries and `USE TEMP B-TREE FOR ORDER BY`.
        let expected = ["SEARCH i USING INDEX issues_by_repo_created (repo_id=? AND kind=? AND state=?)"];
        for sort in [IssueSort::CreatedDesc, IssueSort::CreatedAsc] {
            let sql = search_query(&SearchFilter { sort, ..open_issues() }, 25, 0).into_string();
            assert_eq!(plans::query_plan(&database, &sql).await, expected, "{sort:?}");
        }
    }

    #[tokio::test]
    async fn tab_counts_read_one_kind_from_a_covering_index() {
        let (_dir, database) = plans::database().await;
        let sql = state_counts_query(&open_issues()).into_string();
        let plan = plans::query_plan(&database, &sql).await;
        assert_eq!(plan.len(), 1, "{plan:?}");
        assert!(plan[0].starts_with("SEARCH i USING COVERING INDEX ") && plan[0].ends_with(" (repo_id=? AND kind=?)"), "{plan:?}");
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
