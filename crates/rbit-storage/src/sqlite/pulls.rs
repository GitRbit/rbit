//! Pull requests: the branch data attached to an issue row, mergeability and merge results.
//!
//! Milestone 2 workstream P (pull requests) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! A pull request is an `issues` row of kind `pull` (written only by
//! [`super::issues::IssueStore`]) plus a `pull_requests` row keyed by it. [`PullStore::create`]
//! inserts both in one transaction, and refuses a second **open** pull request for the
//! same base repository, base branch, head repository and head branch inside that same
//! transaction (its first write takes `SQLite`'s write lock, so concurrent creations are
//! serialised and cannot both pass the check).
//!
//! Lists join the issue rows read-only; the issue row conversion is repeated here because
//! the core's row type is private to its store.

use super::pool::Pool;
use rbit_core::{
    AccountId, Issue, IssueId, IssueKind, IssueNumber, IssueState, IssueText, IssueTitle, RepoId, StateReason, Timestamp,
    issue::MergeStrategy,
    pull::{MAX_CONFLICT_PATHS, Mergeability, PullRequest},
};

use super::issues::{IssueStore, NewIssue};
use crate::StorageError;

/// The `pull_requests` columns in [`PullRow`] order (prefix `p.`).
macro_rules! pull_columns {
    () => {
        "p.issue_id, p.repo_id, p.head_repo_id, p.head_branch, p.base_branch, p.head_sha, p.is_draft, p.mergeable, \
         p.mergeable_base_sha, p.mergeable_head_sha, p.conflict_paths, p.merged_at, p.merged_by, p.merge_strategy, \
         p.merge_commit_sha, p.merged_base_sha, p.merged_head_sha, p.created_at, p.updated_at"
    };
}

/// The joined `issues` columns, aliased for [`IssueColumns`] (prefix `i.`).
macro_rules! issue_columns {
    () => {
        "i.id AS i_id, i.number AS i_number, i.kind AS i_kind, i.title AS i_title, i.body AS i_body, i.author_id AS i_author_id, \
         i.state AS i_state, i.state_reason AS i_state_reason, i.closed_at AS i_closed_at, i.closed_by AS i_closed_by, \
         i.locked AS i_locked, i.comment_count AS i_comment_count, i.created_at AS i_created_at, i.updated_at AS i_updated_at"
    };
}

/// List filter: `?1` repository, `?2` state (`open`, `closed`, `merged`, `all`), `?3`
/// author or NULL, `?4` head repository or NULL, `?5` head branch or NULL, `?6` base
/// branch or NULL.
macro_rules! list_filter {
    () => {
        " FROM pull_requests p JOIN issues i ON i.id = p.issue_id WHERE p.repo_id = ?1 \
          AND (?2 = 'all' OR (?2 = 'open' AND i.state = 'open') OR (?2 = 'closed' AND i.state = 'closed') \
               OR (?2 = 'merged' AND p.merged_at IS NOT NULL)) \
          AND (?3 IS NULL OR i.author_id = ?3) AND (?4 IS NULL OR p.head_repo_id = ?4) \
          AND (?5 IS NULL OR p.head_branch = ?5) AND (?6 IS NULL OR p.base_branch = ?6)"
    };
}

fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

#[derive(Debug, sqlx::FromRow)]
struct PullRow {
    issue_id: i64,
    repo_id: i64,
    head_repo_id: Option<i64>,
    head_branch: String,
    base_branch: String,
    head_sha: String,
    is_draft: bool,
    mergeable: String,
    mergeable_base_sha: Option<String>,
    mergeable_head_sha: Option<String>,
    conflict_paths: String,
    merged_at: Option<i64>,
    merged_by: Option<i64>,
    merge_strategy: Option<String>,
    merge_commit_sha: Option<String>,
    merged_base_sha: Option<String>,
    merged_head_sha: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<PullRow> for PullRequest {
    type Error = StorageError;

    fn try_from(row: PullRow) -> Result<Self, Self::Error> {
        Ok(Self {
            issue_id: IssueId(row.issue_id),
            repo_id: RepoId(row.repo_id),
            head_repo_id: row.head_repo_id.map(RepoId),
            head_branch: row.head_branch,
            base_branch: row.base_branch,
            head_sha: row.head_sha,
            is_draft: row.is_draft,
            mergeable: Mergeability::parse(&row.mergeable).ok_or_else(|| corrupt("mergeability", &row.mergeable))?,
            mergeable_base_sha: row.mergeable_base_sha,
            mergeable_head_sha: row.mergeable_head_sha,
            conflict_paths: serde_json::from_str(&row.conflict_paths).map_err(|e| corrupt("conflict paths", e))?,
            merged_at: row.merged_at.map(Timestamp),
            merged_by: row.merged_by.map(AccountId),
            merge_strategy: row
                .merge_strategy
                .map(|strategy| MergeStrategy::parse(&strategy).ok_or_else(|| corrupt("merge strategy", &strategy)))
                .transpose()?,
            merge_commit_sha: row.merge_commit_sha,
            merged_base_sha: row.merged_base_sha,
            merged_head_sha: row.merged_head_sha,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
#[expect(clippy::struct_field_names, reason = "the columns are aliased with an `i_` prefix in the join")]
struct IssueColumns {
    i_id: i64,
    i_number: i64,
    i_kind: String,
    i_title: String,
    i_body: String,
    i_author_id: Option<i64>,
    i_state: String,
    i_state_reason: Option<String>,
    i_closed_at: Option<i64>,
    i_closed_by: Option<i64>,
    i_locked: bool,
    i_comment_count: i64,
    i_created_at: i64,
    i_updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ListedRow {
    #[sqlx(flatten)]
    pull: PullRow,
    #[sqlx(flatten)]
    issue: IssueColumns,
}

impl ListedRow {
    fn into_pair(self) -> Result<(Issue, PullRequest), StorageError> {
        let row = self.issue;
        let issue = Issue {
            id: IssueId(row.i_id),
            repo_id: RepoId(self.pull.repo_id),
            number: IssueNumber::try_from(row.i_number).map_err(|e| corrupt("issue number", e))?,
            kind: IssueKind::parse(&row.i_kind).ok_or_else(|| corrupt("issue kind", &row.i_kind))?,
            title: IssueTitle::parse(&row.i_title).map_err(|e| corrupt("issue title", e))?,
            body: IssueText::body(&row.i_body).map_err(|e| corrupt("issue body", e))?,
            author: row.i_author_id.map(AccountId),
            state: IssueState::parse(&row.i_state).ok_or_else(|| corrupt("issue state", &row.i_state))?,
            state_reason: row
                .i_state_reason
                .map(|reason| StateReason::parse(&reason).ok_or_else(|| corrupt("state reason", &reason)))
                .transpose()?,
            closed_at: row.i_closed_at.map(Timestamp),
            closed_by: row.i_closed_by.map(AccountId),
            locked: row.i_locked,
            comment_count: u32::try_from(row.i_comment_count).map_err(|e| corrupt("comment count", e))?,
            created_at: Timestamp(row.i_created_at),
            updated_at: Timestamp(row.i_updated_at),
        };
        Ok((issue, self.pull.try_into()?))
    }
}

/// A pull request to open: its thread and its branches.
#[derive(Debug, Clone)]
pub struct NewPull {
    pub issue: NewIssue,
    pub head_repo: RepoId,
    pub head_branch: String,
    pub base_branch: String,
    pub head_sha: String,
    pub is_draft: bool,
}

/// What [`PullStore::create`] did.
#[derive(Debug, Clone)]
pub enum CreatedPull {
    Created(Box<(Issue, PullRequest)>),
    /// Another open pull request has the same base and head; nothing was written.
    Duplicate(IssueNumber),
}

/// Which pull requests a list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PullStateFilter {
    #[default]
    Open,
    Closed,
    Merged,
    All,
}

impl PullStateFilter {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
            Self::All => "all",
        }
    }
}

/// How a list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PullSort {
    #[default]
    Created,
    Updated,
}

/// A list query.
#[derive(Debug, Clone, Default)]
pub struct PullFilter {
    pub state: PullStateFilter,
    pub author: Option<AccountId>,
    pub head_repo: Option<RepoId>,
    pub head_branch: Option<String>,
    pub base_branch: Option<String>,
    pub sort: PullSort,
    pub ascending: bool,
}

/// The merge to record.
#[derive(Debug, Clone)]
pub struct MergeRecord<'a> {
    pub merged_by: Option<AccountId>,
    /// `None`: merged outside rbit.
    pub strategy: Option<MergeStrategy>,
    pub commit: &'a str,
    pub base: &'a str,
    pub head: &'a str,
    pub at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct PullStore {
    pool: Pool,
}

impl PullStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Pull data for a bounded page of issues in one repository.
    pub async fn find_among(&self, repo: RepoId, ids: &[IssueId]) -> Result<Vec<PullRequest>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new(concat!("SELECT ", pull_columns!(), " FROM pull_requests p WHERE p.repo_id = "));
        query.push_bind(repo.0).push(" AND p.issue_id IN (");
        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id.0);
        }
        separated.push_unseparated(")");
        query.build_query_as::<PullRow>().fetch_all(&self.pool).await?.into_iter().map(PullRequest::try_from).collect()
    }

    /// Inserts the thread and pull request unless an open pull has the same base and head.
    pub async fn create(&self, new: &NewPull) -> Result<CreatedPull, StorageError> {
        let mut tx = self.pool.begin().await?;
        // The first write: takes the write lock before the duplicate check below.
        let issue = IssueStore::insert_in(&mut tx, &new.issue).await?;
        let duplicate: Option<i64> = sqlx::query_scalar(
            "SELECT i.number FROM pull_requests p JOIN issues i ON i.id = p.issue_id \
             WHERE p.repo_id = ? AND p.base_branch = ? AND p.head_repo_id = ? AND p.head_branch = ? AND i.state = 'open' \
             ORDER BY i.number LIMIT 1",
        )
        .bind(new.issue.repo.0)
        .bind(&new.base_branch)
        .bind(new.head_repo.0)
        .bind(&new.head_branch)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(number) = duplicate {
            tx.rollback().await?;
            return Ok(CreatedPull::Duplicate(IssueNumber::try_from(number).map_err(|e| corrupt("issue number", e))?));
        }
        sqlx::query(
            "INSERT INTO pull_requests (issue_id, repo_id, head_repo_id, head_branch, base_branch, head_sha, is_draft, \
             created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(issue.id.0)
        .bind(new.issue.repo.0)
        .bind(new.head_repo.0)
        .bind(&new.head_branch)
        .bind(&new.base_branch)
        .bind(&new.head_sha)
        .bind(new.is_draft)
        .bind(new.issue.at.0)
        .bind(new.issue.at.0)
        .execute(&mut *tx)
        .await?;
        let row: PullRow = sqlx::query_as(concat!("SELECT ", pull_columns!(), " FROM pull_requests p WHERE p.issue_id = ?"))
            .bind(issue.id.0)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(CreatedPull::Created(Box::new((issue, row.try_into()?))))
    }

    pub async fn find(&self, issue: IssueId) -> Result<Option<PullRequest>, StorageError> {
        let row: Option<PullRow> = sqlx::query_as(concat!("SELECT ", pull_columns!(), " FROM pull_requests p WHERE p.issue_id = ?"))
            .bind(issue.0)
            .fetch_optional(&self.pool)
            .await?;
        row.map(PullRequest::try_from).transpose()
    }

    /// One page of `repo`'s pull requests matching `filter`, with their threads.
    pub async fn list(
        &self,
        repo: RepoId,
        filter: &PullFilter,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<(Issue, PullRequest)>, StorageError> {
        // Fixed SQL per ordering (sqlx only takes static strings).
        let query = match (filter.sort, filter.ascending) {
            (PullSort::Created, false) => {
                concat!("SELECT ", pull_columns!(), ", ", issue_columns!(), list_filter!(), " ORDER BY i.number DESC LIMIT ?7 OFFSET ?8")
            }
            (PullSort::Created, true) => {
                concat!("SELECT ", pull_columns!(), ", ", issue_columns!(), list_filter!(), " ORDER BY i.number ASC LIMIT ?7 OFFSET ?8")
            }
            (PullSort::Updated, false) => concat!(
                "SELECT ",
                pull_columns!(),
                ", ",
                issue_columns!(),
                list_filter!(),
                " ORDER BY i.updated_at DESC, i.number DESC LIMIT ?7 OFFSET ?8"
            ),
            (PullSort::Updated, true) => concat!(
                "SELECT ",
                pull_columns!(),
                ", ",
                issue_columns!(),
                list_filter!(),
                " ORDER BY i.updated_at ASC, i.number ASC LIMIT ?7 OFFSET ?8"
            ),
        };
        let rows: Vec<ListedRow> = sqlx::query_as(query)
            .bind(repo.0)
            .bind(filter.state.as_str())
            .bind(filter.author.map(|id| id.0))
            .bind(filter.head_repo.map(|id| id.0))
            .bind(filter.head_branch.as_deref())
            .bind(filter.base_branch.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(ListedRow::into_pair).collect()
    }

    /// How many pull requests of `repo` match `filter`.
    pub async fn count(&self, repo: RepoId, filter: &PullFilter) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar(concat!("SELECT COUNT(*)", list_filter!()))
            .bind(repo.0)
            .bind(filter.state.as_str())
            .bind(filter.author.map(|id| id.0))
            .bind(filter.head_repo.map(|id| id.0))
            .bind(filter.head_branch.as_deref())
            .bind(filter.base_branch.as_deref())
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// Open pull requests whose head is `branch` of `head_repo`.
    pub async fn open_with_head(&self, head_repo: RepoId, branch: &str) -> Result<Vec<(Issue, PullRequest)>, StorageError> {
        let rows: Vec<ListedRow> = sqlx::query_as(concat!(
            "SELECT ",
            pull_columns!(),
            ", ",
            issue_columns!(),
            " FROM pull_requests p JOIN issues i ON i.id = p.issue_id \
             WHERE p.head_repo_id = ? AND p.head_branch = ? AND i.state = 'open' ORDER BY i.id"
        ))
        .bind(head_repo.0)
        .bind(branch)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ListedRow::into_pair).collect()
    }

    /// Open pull requests of `repo` whose head is commit `sha` (milestone 3, S: a commit
    /// status arrived for it).
    pub async fn open_with_head_sha(&self, repo: RepoId, sha: &str) -> Result<Vec<(Issue, PullRequest)>, StorageError> {
        let rows: Vec<ListedRow> = sqlx::query_as(concat!(
            "SELECT ",
            pull_columns!(),
            ", ",
            issue_columns!(),
            " FROM pull_requests p JOIN issues i ON i.id = p.issue_id \
             WHERE p.repo_id = ? AND p.head_sha = ? AND i.state = 'open' ORDER BY i.id"
        ))
        .bind(repo.0)
        .bind(sha)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ListedRow::into_pair).collect()
    }

    /// Open pull requests into `branch` of `repo`.
    pub async fn open_with_base(&self, repo: RepoId, branch: &str) -> Result<Vec<(Issue, PullRequest)>, StorageError> {
        let rows: Vec<ListedRow> = sqlx::query_as(concat!(
            "SELECT ",
            pull_columns!(),
            ", ",
            issue_columns!(),
            " FROM pull_requests p JOIN issues i ON i.id = p.issue_id \
             WHERE p.repo_id = ? AND p.base_branch = ? AND i.state = 'open' ORDER BY i.id"
        ))
        .bind(repo.0)
        .bind(branch)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ListedRow::into_pair).collect()
    }

    /// Another open pull request (not `except`) with the same base and head, if any.
    pub async fn open_duplicate(&self, pull: &PullRequest) -> Result<Option<IssueNumber>, StorageError> {
        let Some(head_repo) = pull.head_repo_id else { return Ok(None) };
        let number: Option<i64> = sqlx::query_scalar(
            "SELECT i.number FROM pull_requests p JOIN issues i ON i.id = p.issue_id \
             WHERE p.repo_id = ? AND p.base_branch = ? AND p.head_repo_id = ? AND p.head_branch = ? AND i.state = 'open' \
             AND p.issue_id <> ? ORDER BY i.number LIMIT 1",
        )
        .bind(pull.repo_id.0)
        .bind(&pull.base_branch)
        .bind(head_repo.0)
        .bind(&pull.head_branch)
        .bind(pull.issue_id.0)
        .fetch_optional(&self.pool)
        .await?;
        number.map(|n| IssueNumber::try_from(n).map_err(|e| corrupt("issue number", e))).transpose()
    }

    /// Records a new head commit and forgets the mergeability. `false` if unchanged.
    pub async fn set_head(&self, issue: IssueId, sha: &str, at: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE pull_requests SET head_sha = ?, mergeable = 'unknown', mergeable_base_sha = NULL, mergeable_head_sha = NULL, \
             conflict_paths = '[]', updated_at = MAX(updated_at, ?) WHERE issue_id = ? AND head_sha <> ?",
        )
        .bind(sha)
        .bind(at.0)
        .bind(issue.0)
        .bind(sha)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Forgets the mergeability of the pull request (its base or head moved).
    pub async fn mark_unknown(&self, issue: IssueId) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE pull_requests SET mergeable = 'unknown', mergeable_base_sha = NULL, mergeable_head_sha = NULL, \
             conflict_paths = '[]' WHERE issue_id = ?",
        )
        .bind(issue.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Stores a trial merge's result, unless the head moved since it started (then the
    /// row stays `unknown` and is picked up again). `true` when stored.
    pub async fn set_mergeability(
        &self,
        issue: IssueId,
        state: Mergeability,
        base_sha: &str,
        head_sha: &str,
        conflict_paths: &[String],
    ) -> Result<bool, StorageError> {
        let paths = &conflict_paths[..conflict_paths.len().min(MAX_CONFLICT_PATHS)];
        let paths = serde_json::to_string(paths).map_err(|e| corrupt("conflict paths", e))?;
        let result = sqlx::query(
            "UPDATE pull_requests SET mergeable = ?, mergeable_base_sha = ?, mergeable_head_sha = ?, conflict_paths = ? \
             WHERE issue_id = ? AND head_sha = ? AND merged_at IS NULL",
        )
        .bind(state.as_str())
        .bind(base_sha)
        .bind(head_sha)
        .bind(paths)
        .bind(issue.0)
        .bind(head_sha)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Open, non-draft pull requests whose mergeability is unknown, oldest first.
    pub async fn unknown_mergeability(&self, limit: u32) -> Result<Vec<PullRequest>, StorageError> {
        let rows: Vec<PullRow> = sqlx::query_as(concat!(
            "SELECT ",
            pull_columns!(),
            " FROM pull_requests p JOIN issues i ON i.id = p.issue_id \
             WHERE p.mergeable = 'unknown' AND p.is_draft = 0 AND i.state = 'open' ORDER BY p.updated_at, p.issue_id LIMIT ?"
        ))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PullRequest::try_from).collect()
    }

    /// Marks the pull request draft or ready. `true` when it changed.
    pub async fn set_draft(&self, issue: IssueId, draft: bool, at: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE pull_requests SET is_draft = ?, mergeable = CASE WHEN ? THEN mergeable ELSE 'unknown' END, \
             updated_at = MAX(updated_at, ?) WHERE issue_id = ? AND is_draft <> ? AND merged_at IS NULL",
        )
        .bind(draft)
        .bind(draft)
        .bind(at.0)
        .bind(issue.0)
        .bind(draft)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Points the pull request at another base branch. `true` when it changed.
    pub async fn set_base(&self, issue: IssueId, branch: &str, at: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE pull_requests SET base_branch = ?, mergeable = 'unknown', mergeable_base_sha = NULL, mergeable_head_sha = NULL, \
             conflict_paths = '[]', updated_at = MAX(updated_at, ?) WHERE issue_id = ? AND base_branch <> ? AND merged_at IS NULL",
        )
        .bind(branch)
        .bind(at.0)
        .bind(issue.0)
        .bind(branch)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Records the merge, once. `false` if it was already merged.
    pub async fn record_merge(&self, issue: IssueId, merge: &MergeRecord<'_>) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE pull_requests SET merged_at = ?, merged_by = ?, merge_strategy = ?, merge_commit_sha = ?, merged_base_sha = ?, \
             merged_head_sha = ?, head_sha = ?, mergeable = 'clean', updated_at = MAX(updated_at, ?) \
             WHERE issue_id = ? AND merged_at IS NULL",
        )
        .bind(merge.at.0)
        .bind(merge.merged_by.map(|id| id.0))
        .bind(merge.strategy.map(MergeStrategy::as_str))
        .bind(merge.commit)
        .bind(merge.base)
        .bind(merge.head)
        .bind(merge.head)
        .bind(merge.at.0)
        .bind(issue.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
