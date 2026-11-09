//! The wiki catalog a backup carries (format 6): every wiki and the finalized revisions
//! whose objects the backup copies, and what an older backup without wiki objects keeps.

use std::path::{Path, PathBuf};

use rbit_core::RepoId;
use rbit_wikis::{WikiId, WikiRevisionId, WikiStoreId};
use sqlx::{Connection, SqliteConnection};

use super::{BackupError, inventory::has_table, read_only, writable};

/// The first backup format that holds wiki objects.
pub const SINCE_FORMAT: u32 = 6;
/// The directory, relative to the data directory and the backup alike.
pub const DIR: &str = "wikis";

/// One wiki and the finalized revisions its catalog records.
#[derive(Debug, Clone)]
pub struct Wiki {
    pub repository: RepoId,
    /// The wiki incarnation (`wiki_repositories.id`).
    pub id: WikiId,
    pub revisions: Vec<WikiRevisionId>,
}

impl Wiki {
    /// `wikis/<repo>/<wiki>.git`.
    pub fn path(&self) -> PathBuf {
        Path::new(DIR).join(self.repository.0.to_string()).join(format!("{}.git", self.id.get()))
    }
}

/// The wikis a snapshot lists, with the catalog identity their stores are bound to.
#[derive(Debug, Default)]
pub struct Wikis {
    pub identity: Option<WikiStoreId>,
    pub wikis: Vec<Wiki>,
}

/// Reads the wikis of the snapshot at `snapshot`. Wikis without a revision have no
/// objects and need nothing besides their catalog row.
pub async fn read(snapshot: &Path) -> Result<Wikis, BackupError> {
    let mut connection = read_only(snapshot).await?;
    let wikis = read_from(&mut connection).await?;
    connection.close().await?;
    Ok(wikis)
}

async fn read_from(connection: &mut SqliteConnection) -> Result<Wikis, BackupError> {
    if !has_table(connection, "wiki_store_identity").await? {
        return Ok(Wikis::default());
    }
    let identity: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT identity FROM wiki_store_identity WHERE singleton=1").fetch_optional(&mut *connection).await?;
    let identity = identity
        .map(|bytes| <[u8; 16]>::try_from(bytes).ok().and_then(|bytes| WikiStoreId::try_from(bytes).ok()).ok_or(BackupError::WikiIdentity))
        .transpose()?;
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT w.repo_id, w.id, v.revision FROM wiki_repositories w JOIN wiki_revisions v ON v.repo_id = w.repo_id \
         ORDER BY w.repo_id, v.generation",
    )
    .fetch_all(&mut *connection)
    .await?;
    let mut wikis: Vec<Wiki> = Vec::new();
    for (repository, wiki, revision) in rows {
        let parsed = revision.parse::<WikiRevisionId>().map_err(|_| BackupError::WikiRevision(revision.clone()))?;
        match wikis.last_mut() {
            Some(last) if last.repository.0 == repository => last.revisions.push(parsed),
            _ => {
                let id = WikiId::new(wiki).ok_or(BackupError::WikiId(wiki))?;
                wikis.push(Wiki { repository: RepoId(repository), id, revisions: vec![parsed] });
            }
        }
    }
    Ok(Wikis { identity, wikis })
}

/// A backup older than format 6 holds no wiki objects: the restored catalog at `database`
/// keeps each wiki (and whether it is on) but loses its pages, so no row refers to a
/// missing object. Returns how many wikis lost their pages.
pub async fn forget_pages(database: &Path) -> Result<u64, BackupError> {
    let mut connection = writable(database).await?;
    let forgotten = forget(&mut connection).await?;
    connection.close().await?;
    Ok(forgotten)
}

async fn forget(connection: &mut SqliteConnection) -> Result<u64, BackupError> {
    let wikis = read_from(connection).await?;
    if wikis.wikis.is_empty() {
        return Ok(0);
    }
    let mut tx = connection.begin().await?;
    for wiki in &wikis.wikis {
        let repository = wiki.repository.0;
        sqlx::query("UPDATE wiki_repositories SET generation = NULL WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wiki_pages WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wiki_revisions WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(u64::try_from(wikis.wikis.len()).unwrap_or(u64::MAX))
}
