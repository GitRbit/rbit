//! Removes from the snapshot every row whose repository or file the backup could not
//! copy, so the restored database never refers to something the backup lacks.
//!
//! A file goes missing between the snapshot and its copy when it is deleted in between:
//! its release, artifact or repository was deleted, a cache entry was evicted, a log
//! expired, an LFS object was pruned. The live database has dropped the row by then too,
//! or is about to; a release file or artifact also has its row inserted just before its
//! file is renamed into place. Either way the backup now records the state as if that
//! deletion, or the unfinished upload, came before the snapshot. A wiki whose object store
//! vanished loses its whole catalog (the cascade removes its revisions and pages), as
//! when its repository's deletion comes first.

use std::path::Path;

use sqlx::Connection;

use super::{BackupError, writable};

/// Everything the backup could not copy, by row id (an LFS object by oid, a wiki and a
/// log by repository and task).
#[derive(Debug, Default, Clone, Copy)]
pub struct Missing<'a> {
    pub repositories: &'a [i64],
    pub wikis: &'a [i64],
    pub attachments: &'a [i64],
    pub lfs_objects: &'a [String],
    pub artifacts: &'a [i64],
    pub caches: &'a [i64],
    pub logs: &'a [i64],
}

impl Missing<'_> {
    /// Whether the backup copied everything the snapshot lists.
    pub fn is_empty(&self) -> bool {
        self.repositories.is_empty()
            && self.wikis.is_empty()
            && self.attachments.is_empty()
            && self.lfs_objects.is_empty()
            && self.artifacts.is_empty()
            && self.caches.is_empty()
            && self.logs.is_empty()
    }
}

/// Deletes the rows of everything `missing` lists from the snapshot at `snapshot`, in one
/// transaction. Nothing is opened when nothing is missing.
pub async fn prune(snapshot: &Path, missing: &Missing<'_>) -> Result<(), BackupError> {
    if missing.is_empty() {
        return Ok(());
    }
    let mut connection = writable(snapshot).await?;
    let mut tx = connection.begin().await?;
    for &id in missing.repositories {
        // Cascades to everything else of the repository, its stored files' rows included.
        sqlx::query("DELETE FROM repositories WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| BackupError::PruneRepository(id, error))?;
    }
    for repository in missing.wikis {
        sqlx::query("DELETE FROM wiki_repositories WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
    }
    for id in missing.attachments {
        sqlx::query("DELETE FROM release_assets WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    for oid in missing.lfs_objects {
        sqlx::query("DELETE FROM lfs_objects WHERE oid = ?").bind(oid).execute(&mut *tx).await?;
    }
    for id in missing.artifacts {
        sqlx::query("DELETE FROM actions_artifacts WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    for id in missing.caches {
        sqlx::query("DELETE FROM actions_caches WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    for task in missing.logs {
        // As the sweeper records a log removed by retention.
        sqlx::query("UPDATE actions_tasks SET log_state = 'expired' WHERE id = ?").bind(task).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    connection.close().await?;
    Ok(())
}
