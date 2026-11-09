//! Repository wikis in backups (format 6): `wikis/<repo>/<wiki>.git` object stores and
//! their root marker, consistent with the snapshot's catalog.
//!
//! The snapshot lists every wiki and the finalized revisions of its catalog. Wiki
//! objects are immutable and a revision becomes visible only after its objects are in
//! place, so the objects of every listed revision exist when the snapshot is taken;
//! exactly those (commits, trees and page blobs) are copied, each re-hashed on the way.
//! Objects prepared for changes the snapshot does not include are left out. A store
//! that vanished in between (its repository was deleted) is reported missing and its
//! catalog removed from the snapshot ([`rbit_storage::sqlite::backup::prune`]), like the
//! other stored files.
//!
//! A restore copies the stores the restored catalog lists and verifies every object
//! their revisions reach before the data directory is put into service.
use std::path::Path;

use anyhow::bail;
use rbit_git::wikis::{
    WikiObjectError,
    lifecycle::{WikiScope, copy_scope, verify_scope},
};
use rbit_storage::sqlite::backup::wikis::{DIR, Wiki, Wikis};

fn scope(wiki: &Wiki) -> WikiScope<'_> {
    WikiScope { repository: wiki.repository, wiki: wiki.id, revisions: &wiki.revisions }
}

/// What [`copy`] did.
#[derive(Debug, Default)]
pub struct CopiedWikis {
    pub copied: u64,
    pub objects: u64,
    /// Repositories whose wiki store vanished after the snapshot.
    pub missing: Vec<i64>,
}

/// Copies the objects of `wikis` from the data directory `source` into the backup `target`.
pub async fn copy(source: &Path, target: &Path, wikis: Wikis) -> anyhow::Result<CopiedWikis> {
    let Some(identity) = wikis.identity else { return Ok(CopiedWikis::default()) };
    let (from, to) = (source.join(DIR), target.join(DIR));
    tokio::task::spawn_blocking(move || {
        let mut result = CopiedWikis::default();
        for wiki in &wikis.wikis {
            match copy_scope(&from, &to, identity, &scope(wiki)) {
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
            verify_scope(&root, identity, &scope(wiki)).map_err(|error| {
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
