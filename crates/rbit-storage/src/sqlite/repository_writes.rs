//! Fixed repository write statements shared by the existing stores and future typed commands.
//!
//! Single-query helpers preserve their caller's executor: pooled today, outer transaction
//! when explicitly supplied by a later reviewed caller. They do not begin, commit, enroll
//! authority, perform filesystem IO or certify settlement. Transfer is the one multiquery
//! helper and borrows the connection of its caller's existing transaction.

use rbit_core::{AccountId, RepoId, RepoName, Timestamp};
use sqlx::SqliteExecutor;

use super::{pool::Conn, repo_settings::RepoGeneralSettings, repositories::NewRepository, rows::RepositoryRow};
use crate::StorageError;

/// Executes only the fixed INSERT; the caller keeps timestamp capture and row conversion.
pub(super) async fn create<'e>(executor: impl SqliteExecutor<'e>, new: &'e NewRepository, now: i64) -> Result<RepositoryRow, StorageError> {
    sqlx::query_as(
        "INSERT INTO repositories (owner_id, name, description, visibility, default_branch, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, owner_id, name, description, visibility, default_branch, created_at, updated_at, pushed_at, \
             NULL AS archived_at",
    )
    .bind(new.owner_id.0)
    .bind(new.name.as_str())
    .bind(new.description.as_deref())
    .bind(new.visibility.as_str())
    .bind(&new.default_branch)
    .bind(now)
    .bind(now)
    .fetch_one(executor)
    .await
    .map_err(|e| StorageError::from_insert(e, "repository name"))
}

/// Shared by both public metadata-delete aliases; retains missing-row unit success.
pub(super) async fn delete<'e>(executor: impl SqliteExecutor<'e>, id: RepoId) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM repositories WHERE id = ?").bind(id.0).execute(executor).await?;
    Ok(())
}

pub(super) async fn update_general<'e>(
    executor: impl SqliteExecutor<'e>,
    id: RepoId,
    settings: &'e RepoGeneralSettings,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE repositories SET description = ?, visibility = ?, default_branch = ?, updated_at = ? WHERE id = ?")
        .bind(settings.description.as_deref())
        .bind(settings.visibility.as_str())
        .bind(&settings.default_branch)
        .bind(Timestamp::now().seconds())
        .bind(id.0)
        .execute(executor)
        .await?;
    Ok(())
}

pub(super) async fn rename<'e>(executor: impl SqliteExecutor<'e>, id: RepoId, name: &'e RepoName) -> Result<(), StorageError> {
    sqlx::query("UPDATE repositories SET name = ?, updated_at = ? WHERE id = ?")
        .bind(name.as_str())
        .bind(Timestamp::now().seconds())
        .bind(id.0)
        .execute(executor)
        .await
        .map_err(|e| StorageError::from_insert(e, "repository name"))?;
    Ok(())
}

pub(super) async fn archive<'e>(executor: impl SqliteExecutor<'e>, id: RepoId, by: AccountId, at: Timestamp) -> Result<bool, StorageError> {
    let result = sqlx::query(
        "INSERT INTO repo_archives (repo_id, archived_by, archived_at) \
             SELECT id, ?2, ?3 FROM repositories WHERE id = ?1 \
             ON CONFLICT (repo_id) DO NOTHING",
    )
    .bind(id.0)
    .bind(by.0)
    .bind(at.seconds())
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(super) async fn unarchive<'e>(executor: impl SqliteExecutor<'e>, id: RepoId) -> Result<bool, StorageError> {
    let result = sqlx::query("DELETE FROM repo_archives WHERE repo_id = ?").bind(id.0).execute(executor).await?;
    Ok(result.rows_affected() == 1)
}

/// Runs all three fixed statements on the SAME caller-owned transaction connection.
/// The caller retains outer BEGIN/COMMIT and all error/rollback ownership.
pub(super) async fn transfer(conn: &mut Conn, repo: RepoId, new_owner: AccountId) -> Result<(), StorageError> {
    sqlx::query("UPDATE repositories SET owner_id = ?, updated_at = ? WHERE id = ?")
        .bind(new_owner.0)
        .bind(Timestamp::now().seconds())
        .bind(repo.0)
        .execute(&mut *conn)
        .await
        .map_err(|e| StorageError::from_insert(e, "repository name"))?;
    sqlx::query("DELETE FROM team_repositories WHERE repo_id = ?").bind(repo.0).execute(&mut *conn).await?;
    sqlx::query("DELETE FROM collaborators WHERE repo_id = ? AND user_id = ?").bind(repo.0).bind(new_owner.0).execute(&mut *conn).await?;
    Ok(())
}
