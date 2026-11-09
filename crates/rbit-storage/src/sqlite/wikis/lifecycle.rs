//! Deleted pages and rename aliases at the current authorized snapshot.
//!
//! Like every other read, each runs the current-viewer authorization first in the same
//! transaction: a viewer who may not read the wiki learns nothing, not even a count.
use super::{WikiCatalogStore, listing::MAX_WIKI_LISTING, rows};
use crate::StorageError;
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_wikis::{
    WikiGeneration, WikiId, WikiPageId, WikiPath,
    listing::{WikiDeletedList, WikiDeletedPage},
};

#[derive(sqlx::FromRow)]
struct DeletedRow {
    page_id: i64,
    path: String,
    title: String,
    deleted_generation: i64,
    at: i64,
    author: Option<String>,
}

impl DeletedRow {
    fn decode(self) -> Result<WikiDeletedPage, StorageError> {
        let (path, title) = rows::names(self.path, self.title)?;
        Ok(WikiDeletedPage {
            id: WikiPageId::new(self.page_id).ok_or_else(rows::corrupt)?,
            path,
            title,
            deleted: WikiGeneration::new(self.deleted_generation).ok_or_else(rows::corrupt)?,
            deleted_at: Timestamp(self.at),
            deleted_by: self.author,
        })
    }
}

/// Tombstones with the time and author of the deleting generation.
macro_rules! deleted {
    ($filter:literal) => {
        concat!(
            "SELECT t.page_id,t.path,t.title,t.deleted_generation,v.created_at AS at,v.author FROM wiki_page_tombstones t ",
            "JOIN wiki_revisions v ON v.repo_id=t.repo_id AND v.generation=t.deleted_generation ",
            $filter
        )
    };
}

impl WikiCatalogStore {
    /// Deleted pages, most recently deleted first, deleted before `before` when given;
    /// at most `limit` (capped at [`MAX_WIKI_LISTING`]). `None` when the viewer may not
    /// read the wiki.
    pub async fn deleted_pages(
        &self,
        repository: RepoId,
        viewer: Option<AccountId>,
        before: Option<WikiGeneration>,
        limit: u8,
    ) -> Result<Option<WikiDeletedList>, StorageError> {
        let limit = limit.clamp(1, MAX_WIKI_LISTING);
        let mut tx = self.pool.begin().await?;
        if Self::authorized(&mut tx, repository, viewer).await?.is_none() {
            return Ok(None);
        }
        let selected: Vec<DeletedRow> =
            sqlx::query_as(deleted!("WHERE t.repo_id=?1 AND t.deleted_generation<?2 ORDER BY t.deleted_generation DESC LIMIT ?3"))
                .bind(repository.0)
                .bind(before.map_or(i64::MAX, WikiGeneration::get))
                .bind(i64::from(limit) + 1)
                .fetch_all(&mut *tx)
                .await?;
        let more = selected.len() > usize::from(limit);
        let entries = selected.into_iter().take(usize::from(limit)).map(DeletedRow::decode).collect::<Result<Vec<_>, _>>()?;
        let next = if more { entries.last().map(|entry| entry.deleted) } else { None };
        Ok(Some(WikiDeletedList { entries, next }))
    }

    /// The deleted page with the stable ID `page`. `None` when the viewer may not read
    /// the wiki or the page is not deleted.
    pub async fn deleted_page(
        &self,
        repository: RepoId,
        viewer: Option<AccountId>,
        page: WikiPageId,
    ) -> Result<Option<WikiDeletedPage>, StorageError> {
        let mut tx = self.pool.begin().await?;
        if Self::authorized(&mut tx, repository, viewer).await?.is_none() {
            return Ok(None);
        }
        let selected: Option<DeletedRow> = sqlx::query_as(deleted!("WHERE t.repo_id=?1 AND t.page_id=?2"))
            .bind(repository.0)
            .bind(page.get())
            .fetch_optional(&mut *tx)
            .await?;
        selected.map(DeletedRow::decode).transpose()
    }

    /// The current path of the page that had `path` before a rename. `None` when the
    /// viewer may not read the wiki, no alias has this path, or its page is deleted.
    pub async fn alias(&self, repository: RepoId, viewer: Option<AccountId>, path: &WikiPath) -> Result<Option<WikiPath>, StorageError> {
        let mut tx = self.pool.begin().await?;
        if Self::authorized(&mut tx, repository, viewer).await?.is_none() {
            return Ok(None);
        }
        // A current page at the path wins; publication removes such aliases anyway.
        let target: Option<String> = sqlx::query_scalar(
            "SELECT p.path FROM wiki_page_aliases a JOIN wiki_pages p ON p.repo_id=a.repo_id AND p.id=a.page_id \
             WHERE a.repo_id=?1 AND a.path=?2 COLLATE BINARY AND p.path<>a.path \
             AND NOT EXISTS(SELECT 1 FROM wiki_pages c WHERE c.repo_id=a.repo_id AND c.path=a.path COLLATE BINARY)",
        )
        .bind(repository.0)
        .bind(path.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        target.map(|target| WikiPath::from_canonical(target).map_err(|_| rows::corrupt())).transpose()
    }

    /// The wiki incarnations of `repository` whose objects must stay: its current wiki
    /// and any wiki with an accepted intent still pending or quarantined (recovery
    /// evidence survives the repository). Housekeeping only: no viewer is involved.
    pub async fn retained_wikis(&self, repository: RepoId) -> Result<Vec<WikiId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM wiki_repositories WHERE repo_id=?1 \
             UNION SELECT wiki_id FROM wiki_mutation_intents WHERE repo_id=?1 AND state IN ('pending','quarantined')",
        )
        .bind(repository.0)
        .fetch_all(&self.pool)
        .await?;
        ids.into_iter().map(|id| WikiId::new(id).ok_or_else(rows::corrupt)).collect()
    }
}
