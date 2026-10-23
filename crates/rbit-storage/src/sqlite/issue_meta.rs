//! Labels, milestones, assignees, and the tables that de-duplicate commit and cross-references
//! (migration `0018_issue_metadata.sql`).
//!
//! Milestone 2 workstream I (issues) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! Everything attaches to `issues (id)`, so it works for issues and pull requests alike.
//! This store never writes the T0 tables: timeline events for these changes are
//! recorded through `IssueStore` by the services, and `updated_at` is moved with
//! `IssueStore::touch`. Validation (name lengths, colours, limits) is the services'; the
//! schema's `CHECK`s are the last line of defence.

use rbit_core::{
    Account, AccountId, CommentId, IssueId, IssueNumber, IssueState, LabelId, MilestoneId, RepoId, Timestamp, issue::LabelSnapshot,
};
use sqlx::SqlitePool;

use super::rows::AccountRow;
use crate::StorageError;

fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

/// A JSON array for `json_each(?)`.
fn json_ids(values: impl IntoIterator<Item = i64>) -> String {
    let values: Vec<i64> = values.into_iter().collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned())
}

fn json_strings(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

/// `?1` repository, `?2` name pattern, `?3` limit. Starts from the accounts that can hold
/// push access at all (the owner, the repository's write collaborators and the owning
/// organisation's members who own it or get `write` from its base permission or a team),
/// each found through an index on the repository or organisation, and only then looks
/// those few up. Checking every account of the instance against the same rules would
/// read the whole `accounts` table on every assignee search.
const ASSIGNABLE: &str = r"WITH candidates (id) AS (
        SELECT r.owner_id FROM repositories r WHERE r.id = ?1
        UNION
        SELECT c.user_id FROM collaborators c WHERE c.repo_id = ?1 AND c.permission IN ('write', 'admin')
        UNION
        SELECT m.user_id FROM repositories r JOIN org_members m ON m.org_id = r.owner_id
          LEFT JOIN org_profiles p ON p.account_id = m.org_id
          WHERE r.id = ?1
            AND (m.role = 'owner' OR IFNULL(p.member_permission, 'write') = 'write'
              OR EXISTS (SELECT 1 FROM team_repositories tr JOIN team_members tm ON tm.team_id = tr.team_id
                 JOIN teams t ON t.id = tr.team_id
                 WHERE tr.repo_id = r.id AND t.org_id = r.owner_id AND tm.user_id = m.user_id
                   AND tr.permission IN ('write', 'admin')))
    )
    SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at FROM candidates JOIN accounts a ON a.id = candidates.id
    WHERE a.kind = 'user' AND a.name LIKE ?2 ESCAPE '\'
      AND NOT EXISTS (SELECT 1 FROM user_logins l WHERE l.account_id = a.id AND l.suspended_at IS NOT NULL)
    ORDER BY a.name LIMIT ?3";

/// Accounts among a JSON array of names. `accounts.name` is `COLLATE NOCASE`, so a plain
/// `IN` matches case-insensitively and seeks its unique index once per name; wrapping the
/// column in `lower()` would read every account instead.
const ACCOUNTS_NAMED: &str =
    "SELECT id, kind, name, display_name, bio, created_at FROM accounts WHERE name IN (SELECT value FROM json_each(?)) ORDER BY name";

// ------------------------------------------------------------------ types

/// A repository's label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub id: LabelId,
    pub repo_id: RepoId,
    pub name: String,
    /// `#rrggbb`, lower case.
    pub color: String,
    pub description: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Label {
    /// The label as timeline events and payloads record it.
    pub fn snapshot(&self) -> LabelSnapshot {
        LabelSnapshot { id: self.id, name: self.name.clone(), color: self.color.clone() }
    }
}

/// A label's fields, already validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelFields {
    pub name: String,
    pub color: String,
    pub description: String,
}

/// A label with the number of open threads carrying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelUsage {
    pub label: Label,
    pub open_issues: u64,
}

/// A repository's milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    pub id: MilestoneId,
    pub repo_id: RepoId,
    pub title: String,
    pub description: String,
    /// `YYYY-MM-DD`.
    pub due_on: Option<String>,
    pub state: IssueState,
    pub closed_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A milestone's fields, already validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneFields {
    pub title: String,
    pub description: String,
    pub due_on: Option<String>,
}

/// A milestone with how many of its threads are open and closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneProgress {
    pub milestone: Milestone,
    pub open_issues: u64,
    pub closed_issues: u64,
}

/// The milestone of one thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueMilestone {
    pub issue: IssueId,
    pub id: MilestoneId,
    pub title: String,
}

// ------------------------------------------------------------------ rows

macro_rules! label_columns {
    () => {
        "l.id, l.repo_id, l.name, l.color, l.description, l.created_at, l.updated_at"
    };
}

/// [`label_columns!`] unqualified, for `RETURNING`.
macro_rules! label_returning {
    () => {
        "id, repo_id, name, color, description, created_at, updated_at"
    };
}

/// [`milestone_columns!`] unqualified, for `RETURNING`.
macro_rules! milestone_returning {
    () => {
        "id, repo_id, title, description, due_on, state, closed_at, created_at, updated_at"
    };
}

macro_rules! milestone_columns {
    () => {
        "m.id, m.repo_id, m.title, m.description, m.due_on, m.state, m.closed_at, m.created_at, m.updated_at"
    };
}

#[derive(Debug, sqlx::FromRow)]
struct LabelRow {
    id: i64,
    repo_id: i64,
    name: String,
    color: String,
    description: String,
    created_at: i64,
    updated_at: i64,
}

impl From<LabelRow> for Label {
    fn from(row: LabelRow) -> Self {
        Self {
            id: LabelId(row.id),
            repo_id: RepoId(row.repo_id),
            name: row.name,
            color: row.color,
            description: row.description,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct LabelUsageRow {
    #[sqlx(flatten)]
    label: LabelRow,
    open_issues: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct IssueLabelRow {
    issue_id: i64,
    id: i64,
    name: String,
    color: String,
}

#[derive(Debug, sqlx::FromRow)]
struct MilestoneRow {
    id: i64,
    repo_id: i64,
    title: String,
    description: String,
    due_on: Option<String>,
    state: String,
    closed_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<MilestoneRow> for Milestone {
    type Error = StorageError;

    fn try_from(row: MilestoneRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: MilestoneId(row.id),
            repo_id: RepoId(row.repo_id),
            title: row.title,
            description: row.description,
            due_on: row.due_on,
            state: IssueState::parse(&row.state).ok_or_else(|| corrupt("milestone state", &row.state))?,
            closed_at: row.closed_at.map(Timestamp),
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct MilestoneProgressRow {
    #[sqlx(flatten)]
    milestone: MilestoneRow,
    open_issues: i64,
    closed_issues: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct AssigneeRow {
    issue_id: i64,
    #[sqlx(flatten)]
    account: AccountRow,
}

/// `LIKE` pattern for names starting with `prefix`, wildcards escaped.
fn prefix_pattern(prefix: &str) -> String {
    let mut pattern = String::with_capacity(prefix.len() + 1);
    for c in prefix.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

// ------------------------------------------------------------------ the store

#[derive(Debug, Clone)]
pub struct IssueMetaStore {
    pool: SqlitePool,
}

impl IssueMetaStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ------------------------------------------------------------------ labels

    /// Creates a label. [`StorageError::Conflict`] (`"label name"`) when the repository
    /// already has one with that name (case-insensitively).
    pub async fn create_label(&self, repo: RepoId, fields: &LabelFields, at: Timestamp) -> Result<Label, StorageError> {
        let row: LabelRow = sqlx::query_as(concat!(
            "INSERT INTO labels (repo_id, name, color, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) RETURNING ",
            label_returning!()
        ))
        .bind(repo.0)
        .bind(&fields.name)
        .bind(&fields.color)
        .bind(&fields.description)
        .bind(at.0)
        .bind(at.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "label name"))?;
        Ok(row.into())
    }

    /// Creates each label whose name the repository does not have yet (for the default
    /// set); returns how many were created.
    pub async fn create_missing_labels(&self, repo: RepoId, labels: &[LabelFields], at: Timestamp) -> Result<u64, StorageError> {
        let mut tx = self.pool.begin().await?;
        let mut created = 0;
        for fields in labels {
            created += sqlx::query(
                "INSERT INTO labels (repo_id, name, color, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) \
                 ON CONFLICT (repo_id, name) DO NOTHING",
            )
            .bind(repo.0)
            .bind(&fields.name)
            .bind(&fields.color)
            .bind(&fields.description)
            .bind(at.0)
            .bind(at.0)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        tx.commit().await?;
        Ok(created)
    }

    /// Changes a label of `repo`; `None` when it does not exist there.
    pub async fn update_label(
        &self,
        repo: RepoId,
        id: LabelId,
        fields: &LabelFields,
        at: Timestamp,
    ) -> Result<Option<Label>, StorageError> {
        let row: Option<LabelRow> = sqlx::query_as(concat!(
            "UPDATE labels SET name = ?, color = ?, description = ?, updated_at = MAX(updated_at, ?) WHERE id = ? AND repo_id = ? \
             RETURNING ",
            label_returning!()
        ))
        .bind(&fields.name)
        .bind(&fields.color)
        .bind(&fields.description)
        .bind(at.0)
        .bind(id.0)
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "label name"))?;
        Ok(row.map(Label::from))
    }

    /// Deletes a label of `repo` (it disappears from every thread, without timeline
    /// events) and returns what it was.
    pub async fn delete_label(&self, repo: RepoId, id: LabelId) -> Result<Option<Label>, StorageError> {
        let row: Option<LabelRow> =
            sqlx::query_as(concat!("DELETE FROM labels WHERE id = ? AND repo_id = ? RETURNING ", label_returning!()))
                .bind(id.0)
                .bind(repo.0)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(Label::from))
    }

    pub async fn find_label(&self, repo: RepoId, id: LabelId) -> Result<Option<Label>, StorageError> {
        let row: Option<LabelRow> = sqlx::query_as(concat!("SELECT ", label_columns!(), " FROM labels l WHERE l.id = ? AND l.repo_id = ?"))
            .bind(id.0)
            .bind(repo.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Label::from))
    }

    /// The label called `name` (case-insensitively).
    pub async fn find_label_by_name(&self, repo: RepoId, name: &str) -> Result<Option<Label>, StorageError> {
        let row: Option<LabelRow> =
            sqlx::query_as(concat!("SELECT ", label_columns!(), " FROM labels l WHERE l.repo_id = ? AND l.name = ?"))
                .bind(repo.0)
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(Label::from))
    }

    /// The labels of `repo` among `names` (case-insensitively), by name.
    pub async fn labels_named(&self, repo: RepoId, names: &[String]) -> Result<Vec<Label>, StorageError> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<LabelRow> = sqlx::query_as(concat!(
            "SELECT ",
            label_columns!(),
            " FROM labels l WHERE l.repo_id = ? AND l.name IN (SELECT value FROM json_each(?)) ORDER BY l.name"
        ))
        .bind(repo.0)
        .bind(json_strings(names))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Label::from).collect())
    }

    /// Every label of `repo`, by name.
    pub async fn labels(&self, repo: RepoId) -> Result<Vec<Label>, StorageError> {
        let rows: Vec<LabelRow> =
            sqlx::query_as(concat!("SELECT ", label_columns!(), " FROM labels l WHERE l.repo_id = ? ORDER BY l.name"))
                .bind(repo.0)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(Label::from).collect())
    }

    /// Every label of `repo` with its open threads of `kind` (`"issue"` or `"pull"`).
    pub async fn label_usage(&self, repo: RepoId, kind: &str) -> Result<Vec<LabelUsage>, StorageError> {
        let rows: Vec<LabelUsageRow> = sqlx::query_as(concat!(
            "SELECT ",
            label_columns!(),
            ", (SELECT COUNT(*) FROM issue_labels il JOIN issues i ON i.id = il.issue_id \
             WHERE il.label_id = l.id AND i.state = 'open' AND i.kind = ?2) AS open_issues \
             FROM labels l WHERE l.repo_id = ?1 ORDER BY l.name"
        ))
        .bind(repo.0)
        .bind(kind)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| LabelUsage { label: row.label.into(), open_issues: count(row.open_issues) }).collect())
    }

    pub async fn count_labels(&self, repo: RepoId) -> Result<u64, StorageError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM labels WHERE repo_id = ?").bind(repo.0).fetch_one(&self.pool).await?;
        Ok(count(n))
    }

    /// The labels of each of `issues`, by name within each thread.
    pub async fn labels_of(&self, issues: &[IssueId]) -> Result<Vec<(IssueId, LabelSnapshot)>, StorageError> {
        if issues.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<IssueLabelRow> = sqlx::query_as(
            "SELECT il.issue_id, l.id, l.name, l.color FROM issue_labels il JOIN labels l ON l.id = il.label_id \
             WHERE il.issue_id IN (SELECT value FROM json_each(?)) ORDER BY il.issue_id, l.name",
        )
        .bind(json_ids(issues.iter().map(|id| id.0)))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (IssueId(row.issue_id), LabelSnapshot { id: LabelId(row.id), name: row.name, color: row.color }))
            .collect())
    }

    /// Puts `label` on `issue`; `false` when it already was there.
    pub async fn add_label(&self, issue: IssueId, label: LabelId, at: Timestamp) -> Result<bool, StorageError> {
        let added = sqlx::query("INSERT INTO issue_labels (issue_id, label_id, created_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
            .bind(issue.0)
            .bind(label.0)
            .bind(at.0)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(added > 0)
    }

    /// Takes `label` off `issue`; `false` when it was not there.
    pub async fn remove_label(&self, issue: IssueId, label: LabelId) -> Result<bool, StorageError> {
        let removed = sqlx::query("DELETE FROM issue_labels WHERE issue_id = ? AND label_id = ?")
            .bind(issue.0)
            .bind(label.0)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(removed > 0)
    }

    // ------------------------------------------------------------------ milestones

    /// Creates an open milestone. [`StorageError::Conflict`] (`"milestone title"`) when the
    /// title is taken (case-insensitively).
    pub async fn create_milestone(&self, repo: RepoId, fields: &MilestoneFields, at: Timestamp) -> Result<Milestone, StorageError> {
        let row: MilestoneRow = sqlx::query_as(concat!(
            "INSERT INTO milestones (repo_id, title, description, due_on, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) \
             RETURNING ",
            milestone_returning!()
        ))
        .bind(repo.0)
        .bind(&fields.title)
        .bind(&fields.description)
        .bind(fields.due_on.as_deref())
        .bind(at.0)
        .bind(at.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "milestone title"))?;
        row.try_into()
    }

    /// Changes a milestone of `repo`; `None` when it does not exist there.
    pub async fn update_milestone(
        &self,
        repo: RepoId,
        id: MilestoneId,
        fields: &MilestoneFields,
        at: Timestamp,
    ) -> Result<Option<Milestone>, StorageError> {
        let row: Option<MilestoneRow> = sqlx::query_as(concat!(
            "UPDATE milestones SET title = ?, description = ?, due_on = ?, updated_at = MAX(updated_at, ?) \
             WHERE id = ? AND repo_id = ? RETURNING ",
            milestone_returning!()
        ))
        .bind(&fields.title)
        .bind(&fields.description)
        .bind(fields.due_on.as_deref())
        .bind(at.0)
        .bind(id.0)
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "milestone title"))?;
        row.map(Milestone::try_from).transpose()
    }

    /// Opens or closes a milestone of `repo`; `None` when it does not exist there.
    pub async fn set_milestone_state(
        &self,
        repo: RepoId,
        id: MilestoneId,
        state: IssueState,
        at: Timestamp,
    ) -> Result<Option<Milestone>, StorageError> {
        let row: Option<MilestoneRow> = sqlx::query_as(concat!(
            "UPDATE milestones SET state = ?1, closed_at = CASE WHEN ?1 = 'closed' THEN COALESCE(closed_at, ?2) END, \
             updated_at = MAX(updated_at, ?2) WHERE id = ?3 AND repo_id = ?4 RETURNING ",
            milestone_returning!()
        ))
        .bind(state.as_str())
        .bind(at.0)
        .bind(id.0)
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Milestone::try_from).transpose()
    }

    /// Deletes a milestone of `repo` (its threads keep no milestone) and returns what it was.
    pub async fn delete_milestone(&self, repo: RepoId, id: MilestoneId) -> Result<Option<Milestone>, StorageError> {
        let row: Option<MilestoneRow> =
            sqlx::query_as(concat!("DELETE FROM milestones WHERE id = ? AND repo_id = ? RETURNING ", milestone_returning!()))
                .bind(id.0)
                .bind(repo.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Milestone::try_from).transpose()
    }

    pub async fn find_milestone(&self, repo: RepoId, id: MilestoneId) -> Result<Option<Milestone>, StorageError> {
        let row: Option<MilestoneRow> =
            sqlx::query_as(concat!("SELECT ", milestone_columns!(), " FROM milestones m WHERE m.id = ? AND m.repo_id = ?"))
                .bind(id.0)
                .bind(repo.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Milestone::try_from).transpose()
    }

    /// The milestone titled `title` (case-insensitively).
    pub async fn find_milestone_by_title(&self, repo: RepoId, title: &str) -> Result<Option<Milestone>, StorageError> {
        let row: Option<MilestoneRow> =
            sqlx::query_as(concat!("SELECT ", milestone_columns!(), " FROM milestones m WHERE m.repo_id = ? AND m.title = ?"))
                .bind(repo.0)
                .bind(title)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Milestone::try_from).transpose()
    }

    /// The milestones of `repo` (only those in `state` if given) with their progress:
    /// open ones by due date (undated last), then title; closed ones most recently
    /// closed first.
    pub async fn milestones(&self, repo: RepoId, state: Option<IssueState>) -> Result<Vec<MilestoneProgress>, StorageError> {
        let rows: Vec<MilestoneProgressRow> = sqlx::query_as(concat!(
            "SELECT ",
            milestone_columns!(),
            ", (SELECT COUNT(*) FROM issue_milestones im JOIN issues i ON i.id = im.issue_id \
                WHERE im.milestone_id = m.id AND i.state = 'open') AS open_issues, \
             (SELECT COUNT(*) FROM issue_milestones im JOIN issues i ON i.id = im.issue_id \
                WHERE im.milestone_id = m.id AND i.state = 'closed') AS closed_issues \
             FROM milestones m WHERE m.repo_id = ?1 AND (?2 IS NULL OR m.state = ?2) \
             ORDER BY m.state = 'closed', CASE WHEN m.state = 'open' THEN m.due_on IS NULL END, \
             CASE WHEN m.state = 'open' THEN m.due_on END, m.closed_at DESC, m.title"
        ))
        .bind(repo.0)
        .bind(state.map(IssueState::as_str))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(MilestoneProgress {
                    milestone: row.milestone.try_into()?,
                    open_issues: count(row.open_issues),
                    closed_issues: count(row.closed_issues),
                })
            })
            .collect()
    }

    /// How many milestones of `repo` are open and closed.
    pub async fn milestone_counts(&self, repo: RepoId) -> Result<(u64, u64), StorageError> {
        let (open, closed): (i64, i64) =
            sqlx::query_as("SELECT COALESCE(SUM(state = 'open'), 0), COALESCE(SUM(state = 'closed'), 0) FROM milestones WHERE repo_id = ?")
                .bind(repo.0)
                .fetch_one(&self.pool)
                .await?;
        Ok((count(open), count(closed)))
    }

    /// The milestone of each of `issues` that has one.
    pub async fn milestones_of(&self, issues: &[IssueId]) -> Result<Vec<IssueMilestone>, StorageError> {
        if issues.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(i64, i64, String)> = sqlx::query_as(
            "SELECT im.issue_id, m.id, m.title FROM issue_milestones im JOIN milestones m ON m.id = im.milestone_id \
             WHERE im.issue_id IN (SELECT value FROM json_each(?)) ORDER BY im.issue_id",
        )
        .bind(json_ids(issues.iter().map(|id| id.0)))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(issue, id, title)| IssueMilestone { issue: IssueId(issue), id: MilestoneId(id), title }).collect())
    }

    /// Sets (or with `None` clears) the milestone of `issue` and returns the previous one.
    pub async fn set_milestone(&self, issue: IssueId, milestone: Option<MilestoneId>) -> Result<Option<IssueMilestone>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let previous: Option<(i64, String)> = sqlx::query_as(
            "SELECT m.id, m.title FROM issue_milestones im JOIN milestones m ON m.id = im.milestone_id WHERE im.issue_id = ?",
        )
        .bind(issue.0)
        .fetch_optional(&mut *tx)
        .await?;
        match milestone {
            Some(milestone) => {
                sqlx::query(
                    "INSERT INTO issue_milestones (issue_id, milestone_id) VALUES (?, ?) \
                     ON CONFLICT (issue_id) DO UPDATE SET milestone_id = excluded.milestone_id",
                )
                .bind(issue.0)
                .bind(milestone.0)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM issue_milestones WHERE issue_id = ?").bind(issue.0).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        Ok(previous.map(|(id, title)| IssueMilestone { issue, id: MilestoneId(id), title }))
    }

    // ------------------------------------------------------------------ assignees

    /// The assignees of each of `issues`, by name within each thread.
    pub async fn assignees_of(&self, issues: &[IssueId]) -> Result<Vec<(IssueId, Account)>, StorageError> {
        if issues.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<AssigneeRow> = sqlx::query_as(
            "SELECT ia.issue_id, a.id, a.kind, a.name, a.display_name, a.bio, a.created_at \
             FROM issue_assignees ia JOIN accounts a ON a.id = ia.user_id \
             WHERE ia.issue_id IN (SELECT value FROM json_each(?)) ORDER BY ia.issue_id, a.name",
        )
        .bind(json_ids(issues.iter().map(|id| id.0)))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|row| Ok((IssueId(row.issue_id), Account::try_from(row.account)?))).collect()
    }

    /// Assigns `user` to `issue`; `false` when they already were.
    pub async fn add_assignee(&self, issue: IssueId, user: AccountId, by: AccountId, at: Timestamp) -> Result<bool, StorageError> {
        let added = sqlx::query(
            "INSERT INTO issue_assignees (issue_id, user_id, assigned_by, created_at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(issue.0)
        .bind(user.0)
        .bind(by.0)
        .bind(at.0)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(added > 0)
    }

    /// Unassigns `user`; `false` when they were not assigned.
    pub async fn remove_assignee(&self, issue: IssueId, user: AccountId) -> Result<bool, StorageError> {
        let removed = sqlx::query("DELETE FROM issue_assignees WHERE issue_id = ? AND user_id = ?")
            .bind(issue.0)
            .bind(user.0)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(removed > 0)
    }

    /// Users (not suspended) who may push to `repo` and so may be assigned: its personal
    /// owner, collaborators with `write` or `admin`, owners of the owning organisation and
    /// members whose base permission or team grant is `write` or `admin`. Names starting
    /// with `prefix` (case-insensitively; empty for all), by name, at most `limit`.
    pub async fn assignable(&self, repo: RepoId, prefix: &str, limit: u32) -> Result<Vec<Account>, StorageError> {
        let rows: Vec<AccountRow> =
            sqlx::query_as(ASSIGNABLE).bind(repo.0).bind(prefix_pattern(prefix)).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter().map(Account::try_from).collect()
    }

    /// Public account names for mention completion, with a fixed result bound.
    pub async fn mention_candidates(&self, prefix: &str) -> Result<Vec<Account>, StorageError> {
        let rows: Vec<AccountRow> = sqlx::query_as(
            "SELECT id, kind, name, display_name, bio, created_at FROM accounts WHERE name LIKE ? ESCAPE '\\' ORDER BY name LIMIT 10",
        )
        .bind(prefix_pattern(prefix))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Account::try_from).collect()
    }

    // ------------------------------------------------------------------ references

    /// Remembers that `commit` mentions `issue`; `false` when it was already recorded.
    pub async fn record_commit_ref(&self, issue: IssueId, commit: &str, at: Timestamp) -> Result<bool, StorageError> {
        let added = sqlx::query("INSERT INTO issue_commit_refs (issue_id, commit_sha, created_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
            .bind(issue.0)
            .bind(commit)
            .bind(at.0)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(added > 0)
    }

    /// Remembers that `source` mentions `target`; `false` when it was already recorded.
    pub async fn record_cross_ref(&self, target: IssueId, source: IssueId, at: Timestamp) -> Result<bool, StorageError> {
        let added = sqlx::query("INSERT INTO issue_cross_refs (target_id, source_id, created_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
            .bind(target.0)
            .bind(source.0)
            .bind(at.0)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(added > 0)
    }

    // ------------------------------------------------------------------ creation and lookups

    /// Gives a new thread its labels, assignees (assigned `by`) and milestone in one
    /// transaction (all already validated by the caller).
    pub async fn attach(
        &self,
        issue: IssueId,
        labels: &[LabelId],
        assignees: &[AccountId],
        milestone: Option<MilestoneId>,
        by: AccountId,
        at: Timestamp,
    ) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        for label in labels {
            sqlx::query("INSERT INTO issue_labels (issue_id, label_id, created_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
                .bind(issue.0)
                .bind(label.0)
                .bind(at.0)
                .execute(&mut *tx)
                .await?;
        }
        for user in assignees {
            sqlx::query(
                "INSERT INTO issue_assignees (issue_id, user_id, assigned_by, created_at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
            )
            .bind(issue.0)
            .bind(user.0)
            .bind(by.0)
            .bind(at.0)
            .execute(&mut *tx)
            .await?;
        }
        if let Some(milestone) = milestone {
            sqlx::query(
                "INSERT INTO issue_milestones (issue_id, milestone_id) VALUES (?, ?) \
                 ON CONFLICT (issue_id) DO UPDATE SET milestone_id = excluded.milestone_id",
            )
            .bind(issue.0)
            .bind(milestone.0)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The thread of `repo` that comment `id` belongs to, as `(issue, number)`: for the
    /// API's repository-wide comment endpoints.
    pub async fn comment_thread(&self, repo: RepoId, id: CommentId) -> Result<Option<(IssueId, IssueNumber)>, StorageError> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT i.id, i.number FROM issue_comments c JOIN issues i ON i.id = c.issue_id WHERE c.id = ? AND i.repo_id = ?",
        )
        .bind(id.0)
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|(issue, number)| Ok((IssueId(issue), IssueNumber::try_from(number).map_err(|e| corrupt("issue number", e))?))).transpose()
    }

    /// The accounts (users and organisations) among `names`, case-insensitively, in one
    /// query: for resolving `@name` mentions.
    pub async fn accounts_named(&self, names: &[String]) -> Result<Vec<Account>, StorageError> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<AccountRow> = sqlx::query_as(ACCOUNTS_NAMED).bind(json_strings(names)).fetch_all(&self.pool).await?;
        rows.into_iter().map(Account::try_from).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::plans;

    #[tokio::test]
    async fn assignee_search_never_scans_accounts() {
        let (_dir, database) = plans::database().await;
        let plan = plans::query_plan(&database, ASSIGNABLE).await;
        // Before: `SCAN a USING INDEX sqlite_autoindex_accounts_1` with correlated
        // permission checks per account.
        assert!(plan.iter().all(|line| !line.starts_with("SCAN") || line == "SCAN candidates"), "{plan:?}");
        assert!(plan.iter().any(|line| line == "SEARCH a USING INTEGER PRIMARY KEY (rowid=?)"), "{plan:?}");
        assert!(plan.iter().any(|line| line.starts_with("SEARCH c USING PRIMARY KEY (repo_id=?")), "{plan:?}");
    }

    #[tokio::test]
    async fn mentions_seek_the_name_index() {
        let (_dir, database) = plans::database().await;
        let plan = plans::query_plan(&database, ACCOUNTS_NAMED).await;
        // Before: `SCAN accounts USING INDEX sqlite_autoindex_accounts_1`.
        assert_eq!(plan[0], "SEARCH accounts USING INDEX sqlite_autoindex_accounts_1 (name=?)", "{plan:?}");
        assert!(plan.iter().all(|line| !line.starts_with("SCAN accounts")), "{plan:?}");
    }

    #[test]
    fn prefix_patterns_escape_wildcards() {
        assert_eq!(prefix_pattern(""), "%");
        assert_eq!(prefix_pattern("a_b%"), r"a\_b\%%");
    }
}
