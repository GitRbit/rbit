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

use anyhow::Context;
use sqlx::{ConnectOptions, Connection, sqlite::SqliteConnectOptions};

use super::copy::{Copied, CopiedBlobs};

/// Deletes the rows of everything `repositories` and `blobs` report missing from the
/// snapshot at `snapshot`, in one transaction.
pub async fn prune(snapshot: &Path, repositories: &Copied, blobs: &CopiedBlobs, wikis: &[i64]) -> anyhow::Result<()> {
    let nothing = repositories.missing.is_empty()
        && wikis.is_empty()
        && blobs.attachments.missing.is_empty()
        && blobs.lfs.missing.is_empty()
        && blobs.artifacts.missing.is_empty()
        && blobs.caches.missing.is_empty()
        && blobs.logs.missing.is_empty();
    if nothing {
        return Ok(());
    }
    let options = rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new()).filename(snapshot).foreign_keys(true);
    let mut connection = options.connect().await.with_context(|| format!("cannot open {}", snapshot.display()))?;
    let mut tx = connection.begin().await?;
    for id in &repositories.missing {
        // Cascades to everything else of the repository, its stored files' rows included.
        sqlx::query("DELETE FROM repositories WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("cannot leave repository {id}, deleted during the backup, out of it; take the backup again"))?;
    }
    for repository in wikis {
        sqlx::query("DELETE FROM wiki_repositories WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
    }
    for id in &blobs.attachments.missing {
        sqlx::query("DELETE FROM release_assets WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    for oid in &blobs.lfs.missing {
        sqlx::query("DELETE FROM lfs_objects WHERE oid = ?").bind(oid).execute(&mut *tx).await?;
    }
    for id in &blobs.artifacts.missing {
        sqlx::query("DELETE FROM actions_artifacts WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    for id in &blobs.caches.missing {
        sqlx::query("DELETE FROM actions_caches WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    for task in &blobs.logs.missing {
        // As the sweeper records a log removed by retention.
        sqlx::query("UPDATE actions_tasks SET log_state = 'expired' WHERE id = ?").bind(task).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    connection.close().await?;
    Ok(())
}
