//! Repository imports: the job's record (migration `0038_imports.sql`) and the writes that
//! bring another forge's history in.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file. Keep row types
//! here rather than in `rows.rs`.
//!
//! Imported threads go through [`super::issues::IssueStore::insert_imported_in`] (the
//! core tables have one writer); an imported pull request adds its `pull_requests` row in
//! the same transaction ([`ImportStore::insert_pull`]). Releases are created by
//! [`super::releases::ReleaseStore`] and only back-dated here.

use super::pool::Pool;
use rbit_core::{
    AccountId, Issue, RepoId, Timestamp,
    import::{ImportCounts, ImportOptions, ImportPhase, ImportStatus, RepoImport, SourceKind},
    release::ReleaseId,
};

use super::issues::{ImportedThread, IssueStore};
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct ImportRow {
    repo_id: i64,
    importer_id: Option<i64>,
    source_kind: String,
    source_url: String,
    options: String,
    status: String,
    phase: String,
    counts: String,
    warnings: String,
    error: Option<String>,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
}

fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

impl TryFrom<ImportRow> for RepoImport {
    type Error = StorageError;

    fn try_from(row: ImportRow) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_id: RepoId(row.repo_id),
            importer: row.importer_id.map(AccountId),
            kind: SourceKind::parse(&row.source_kind).ok_or_else(|| corrupt("import source", &row.source_kind))?,
            source_url: row.source_url,
            options: serde_json::from_str(&row.options).map_err(|e| corrupt("import options", e))?,
            status: ImportStatus::parse(&row.status).ok_or_else(|| corrupt("import status", &row.status))?,
            phase: ImportPhase::parse(&row.phase).ok_or_else(|| corrupt("import phase", &row.phase))?,
            counts: serde_json::from_str(&row.counts).map_err(|e| corrupt("import counts", e))?,
            warnings: serde_json::from_str(&row.warnings).map_err(|e| corrupt("import warnings", e))?,
            error: row.error,
            created_at: Timestamp(row.created_at),
            started_at: row.started_at.map(Timestamp),
            finished_at: row.finished_at.map(Timestamp),
        })
    }
}

fn json(value: &impl serde::Serialize) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|e| corrupt("import record", e))
}

/// A new import record.
#[derive(Debug, Clone)]
pub struct NewImport<'a> {
    pub repo: RepoId,
    pub importer: AccountId,
    pub kind: SourceKind,
    pub source_url: &'a str,
    pub options: ImportOptions,
}

/// The branch data of an imported pull request.
#[derive(Debug, Clone)]
pub struct ImportedPull {
    /// `Some(repo)` when the head branch lives in the imported repository itself.
    pub head_repo: Option<RepoId>,
    pub head_branch: String,
    pub base_branch: String,
    pub head_sha: String,
    pub is_draft: bool,
    /// When it was merged and with which commit, for merged pull requests.
    pub merged: Option<(Timestamp, Option<String>)>,
    pub merged_by: Option<AccountId>,
}

#[derive(Debug, Clone)]
pub struct ImportStore {
    pool: Pool,
}

impl ImportStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Records a queued import of `repo` (replacing an earlier record of it).
    pub async fn create(&self, new: &NewImport<'_>) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT OR REPLACE INTO repo_imports (repo_id, importer_id, source_kind, source_url, options, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(new.repo.0)
        .bind(new.importer.0)
        .bind(new.kind.as_str())
        .bind(new.source_url)
        .bind(json(&new.options)?)
        .bind(Timestamp::now().seconds())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find(&self, repo: RepoId) -> Result<Option<RepoImport>, StorageError> {
        let row: Option<ImportRow> = sqlx::query_as(
            "SELECT repo_id, importer_id, source_kind, source_url, options, status, phase, counts, warnings, error, created_at, \
             started_at, finished_at FROM repo_imports WHERE repo_id = ?",
        )
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(RepoImport::try_from).transpose()
    }

    /// Marks the import running from `at`.
    pub async fn start(&self, repo: RepoId, at: Timestamp) -> Result<(), StorageError> {
        sqlx::query("UPDATE repo_imports SET status = 'running', started_at = ? WHERE repo_id = ?")
            .bind(at.seconds())
            .bind(repo.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Saves the progress of a running import.
    pub async fn save_progress(
        &self,
        repo: RepoId,
        phase: ImportPhase,
        counts: &ImportCounts,
        warnings: &[String],
    ) -> Result<(), StorageError> {
        sqlx::query("UPDATE repo_imports SET phase = ?, counts = ?, warnings = ? WHERE repo_id = ?")
            .bind(phase.as_str())
            .bind(json(counts)?)
            .bind(json(&warnings)?)
            .bind(repo.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Ends the import at `at`: succeeded, or failed with `error`.
    pub async fn finish(&self, repo: RepoId, at: Timestamp, error: Option<&str>) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE repo_imports SET status = CASE WHEN ?1 IS NULL THEN 'succeeded' ELSE 'failed' END, error = ?1, \
             phase = CASE WHEN ?1 IS NULL THEN 'finished' ELSE phase END, finished_at = ?2 WHERE repo_id = ?3",
        )
        .bind(error)
        .bind(at.seconds())
        .bind(repo.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fails every queued or running import except those of `running` with `error` (at
    /// start-up: their jobs died with the previous process, and their tokens with them).
    /// Returns how many there were.
    pub async fn fail_unfinished(&self, at: Timestamp, error: &str, running: &[RepoId]) -> Result<u64, StorageError> {
        let running =
            serde_json::to_string(&running.iter().map(|repo| repo.0).collect::<Vec<_>>()).map_err(|e| corrupt("repository ids", e))?;
        let result = sqlx::query(
            "UPDATE repo_imports SET status = 'failed', error = ?, finished_at = ? \
             WHERE status IN ('queued', 'running') AND repo_id NOT IN (SELECT value FROM json_each(?))",
        )
        .bind(error)
        .bind(at.seconds())
        .bind(running)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // ------------------------------------------------------------------ history

    /// Inserts an imported pull request: its thread (through [`IssueStore`]) and its
    /// branch data, in one transaction. Merged ones are recorded as merged outside rbit.
    pub async fn insert_pull(&self, thread: &ImportedThread, pull: &ImportedPull) -> Result<Issue, StorageError> {
        let mut tx = self.pool.begin().await?;
        let issue = IssueStore::insert_imported_in(&mut tx, thread).await?;
        let (merged_at, merge_commit) = pull.merged.as_ref().map_or((None, None), |(at, commit)| (Some(at.0), commit.as_deref()));
        sqlx::query(
            "INSERT INTO pull_requests (issue_id, repo_id, head_repo_id, head_branch, base_branch, head_sha, is_draft, merged_at, \
             merged_by, merge_commit_sha, merged_head_sha, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CASE WHEN ?8 IS NULL THEN NULL ELSE ?6 END, ?11, ?12)",
        )
        .bind(issue.id.0)
        .bind(thread.repo.0)
        .bind(pull.head_repo.map(|r| r.0))
        .bind(&pull.head_branch)
        .bind(&pull.base_branch)
        .bind(&pull.head_sha)
        .bind(pull.is_draft)
        .bind(merged_at)
        .bind(pull.merged_by.map(|a| a.0))
        .bind(merge_commit)
        .bind(thread.created_at.0)
        .bind(thread.updated_at.0.max(thread.created_at.0))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(issue)
    }

    /// Gives release `id` its original creation and publication times.
    pub async fn backdate_release(
        &self,
        id: ReleaseId,
        created_at: Timestamp,
        published_at: Option<Timestamp>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE releases SET created_at = ?1, updated_at = ?1, published_at = CASE WHEN published_at IS NULL THEN NULL ELSE COALESCE(?2, ?1) END \
             WHERE id = ?3",
        )
        .bind(created_at.seconds())
        .bind(published_at.map(Timestamp::seconds))
        .bind(id.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ------------------------------------------------------------------ authors

    /// The active user whose **verified** email address is `email` (case-insensitive).
    pub async fn user_with_verified_email(&self, email: &str) -> Result<Option<AccountId>, StorageError> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT l.account_id FROM user_logins l WHERE l.email = ? AND l.email_verified_at IS NOT NULL AND l.suspended_at IS NULL",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(id.map(AccountId))
    }

    /// The active user (not an organisation) called `name` (case-insensitive).
    pub async fn user_named(&self, name: &str) -> Result<Option<AccountId>, StorageError> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT a.id FROM accounts a JOIN user_logins l ON l.account_id = a.id WHERE a.name = ? AND l.suspended_at IS NULL",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(id.map(AccountId))
    }
}
