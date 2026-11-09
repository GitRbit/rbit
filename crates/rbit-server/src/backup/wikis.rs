//! Repository wikis in backups (format 6): `wikis/<repo>/<wiki>.git` object stores and
//! their root marker, consistent with the snapshot's catalog.
//!
//! The snapshot lists every wiki and the finalized revisions of its catalog. Wiki
//! objects are immutable and a revision becomes visible only after its objects are in
//! place, so the objects of every listed revision exist when the snapshot is taken;
//! exactly those (commits, trees and page blobs) are copied, each re-hashed on the way.
//! Objects prepared for changes the snapshot does not include are left out. A store
//! that vanished in between (its repository was deleted) is reported missing and its
//! catalog removed from the snapshot ([`super::prune`]), like the other stored files.
//!
//! A restore copies the stores the restored catalog lists and verifies every object
//! their revisions reach before the data directory is put into service.
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use rbit_core::RepoId;
use rbit_git::wikis::{
    WikiObjectError,
    lifecycle::{WikiScope, copy_scope, verify_scope},
};
use rbit_wikis::{WikiId, WikiRevisionId, WikiStoreId};
use sqlx::SqliteConnection;

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
    fn scope(&self) -> WikiScope<'_> {
        WikiScope { repository: self.repository, wiki: self.id, revisions: &self.revisions }
    }

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

/// What [`copy`] did.
#[derive(Debug, Default)]
pub struct CopiedWikis {
    pub copied: u64,
    pub objects: u64,
    /// Repositories whose wiki store vanished after the snapshot.
    pub missing: Vec<i64>,
}

/// Reads the wikis of the snapshot behind `connection`. Wikis without a revision have
/// no objects and need nothing besides their catalog row.
pub async fn read(connection: &mut SqliteConnection) -> anyhow::Result<Wikis> {
    let present: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='wiki_store_identity')")
        .fetch_one(&mut *connection)
        .await?;
    if !present {
        return Ok(Wikis::default());
    }
    let identity: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT identity FROM wiki_store_identity WHERE singleton=1").fetch_optional(&mut *connection).await?;
    let identity = identity
        .map(|bytes| {
            <[u8; 16]>::try_from(bytes).ok().and_then(|bytes| WikiStoreId::try_from(bytes).ok()).context("invalid wiki store identity")
        })
        .transpose()?;
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT w.repo_id, w.id, v.revision FROM wiki_repositories w JOIN wiki_revisions v ON v.repo_id = w.repo_id \
         ORDER BY w.repo_id, v.generation",
    )
    .fetch_all(&mut *connection)
    .await?;
    let mut wikis: Vec<Wiki> = Vec::new();
    for (repository, wiki, revision) in rows {
        let revision = revision.parse::<WikiRevisionId>().ok().with_context(|| format!("invalid wiki revision {revision:?}"))?;
        match wikis.last_mut() {
            Some(last) if last.repository.0 == repository => last.revisions.push(revision),
            _ => {
                let id = WikiId::new(wiki).with_context(|| format!("invalid wiki id {wiki}"))?;
                wikis.push(Wiki { repository: RepoId(repository), id, revisions: vec![revision] });
            }
        }
    }
    Ok(Wikis { identity, wikis })
}

/// Copies the objects of `wikis` from the data directory `source` into the backup `target`.
pub async fn copy(source: &Path, target: &Path, wikis: Wikis) -> anyhow::Result<CopiedWikis> {
    let Some(identity) = wikis.identity else { return Ok(CopiedWikis::default()) };
    let (from, to) = (source.join(DIR), target.join(DIR));
    tokio::task::spawn_blocking(move || {
        let mut result = CopiedWikis::default();
        for wiki in &wikis.wikis {
            match copy_scope(&from, &to, identity, &wiki.scope()) {
                Ok(count) => {
                    result.copied += 1;
                    result.objects += count.objects;
                }
                Err(WikiObjectError::Missing) => {
                    tracing::warn!(repo = wiki.repository.0, "wiki is in the database but its objects are not on disk; skipped");
                    result.missing.push(wiki.repository.0);
                }
                Err(error) => {
                    return Err(anyhow::Error::new(error)
                        .context(format!("backup incomplete: the wiki of repository {} could not be copied", wiki.repository.0)));
                }
            }
        }
        Ok(result)
    })
    .await?
}

/// Restores and verifies the wiki stores the restored catalog lists; returns how many.
pub async fn restore(from: &Path, target: &Path, wikis: Wikis) -> anyhow::Result<u64> {
    let Some(identity) = wikis.identity.filter(|_| !wikis.wikis.is_empty()) else { return Ok(0) };
    let marker_dir = from.join(DIR);
    if !marker_dir.is_dir() {
        bail!("the backup is damaged: {} is missing", Path::new(DIR).display());
    }
    // The root marker binds the stores to the catalog; verification checks it below.
    tokio::fs::create_dir_all(target.join(DIR)).await?;
    let mut listing = tokio::fs::read_dir(&marker_dir).await?;
    while let Some(entry) = listing.next_entry().await? {
        if entry.file_type().await?.is_file() {
            tokio::fs::copy(entry.path(), target.join(DIR).join(entry.file_name())).await?;
        }
    }
    for wiki in &wikis.wikis {
        let relative = wiki.path();
        if !from.join(&relative).is_dir() {
            bail!("the backup is damaged: the wiki of repository {} ({}) is missing", wiki.repository.0, relative.display());
        }
        super::restore::copy_tree(&from.join(&relative), &target.join(&relative)).await?;
    }
    let root = target.join(DIR);
    let count = u64::try_from(wikis.wikis.len()).unwrap_or(u64::MAX);
    tokio::task::spawn_blocking(move || {
        for wiki in &wikis.wikis {
            verify_scope(&root, identity, &wiki.scope()).map_err(|error| {
                anyhow::Error::new(error).context(format!(
                    "the backup is damaged: the wiki of repository {} ({}) fails verification",
                    wiki.repository.0,
                    wiki.path().display()
                ))
            })?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(count)
}

/// A backup older than format 6 holds no wiki objects: the restored catalog keeps each
/// wiki (and whether it is on) but loses its pages, so no row refers to a missing object.
/// Returns how many wikis lost their pages.
pub async fn forget_pages(connection: &mut SqliteConnection) -> anyhow::Result<u64> {
    let wikis = read(connection).await?;
    if wikis.wikis.is_empty() {
        return Ok(0);
    }
    let mut tx = sqlx::Connection::begin(&mut *connection).await?;
    for wiki in &wikis.wikis {
        let repository = wiki.repository.0;
        sqlx::query("UPDATE wiki_repositories SET generation = NULL WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wiki_pages WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wiki_revisions WHERE repo_id = ?").bind(repository).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(u64::try_from(wikis.wikis.len()).unwrap_or(u64::MAX))
}
