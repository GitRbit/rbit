//! Bounded page listings and title search at the current authorized snapshot.
//!
//! Every listing runs the same current-viewer authorization as page reads first, in
//! the same transaction, so a viewer who may not read the repository or whose wiki
//! is disabled sees nothing at all (not even a count).
use super::{WikiCatalogStore, rows};
use crate::StorageError;
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_wikis::{
    WikiPageId, WikiPath,
    listing::{WikiPageEntry, WikiPageList},
};

/// Upper bound of one listing or search page.
pub const MAX_WIKI_LISTING: u8 = 100;

#[derive(sqlx::FromRow)]
struct EntryRow {
    id: i64,
    path: String,
    title: String,
    updated_at: i64,
}

impl EntryRow {
    fn decode(self) -> Result<WikiPageEntry, StorageError> {
        let (path, title) = rows::names(self.path, self.title)?;
        Ok(WikiPageEntry { id: WikiPageId::new(self.id).ok_or_else(rows::corrupt)?, path, title, updated_at: Timestamp(self.updated_at) })
    }
}

/// `LIKE` pattern matching `query` literally anywhere (ASCII case-insensitive).
fn contains(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for character in query.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

impl WikiCatalogStore {
    /// Whether the repository has a wiki and whether it is enabled. The caller must
    /// already have authorized repository read access; this reveals only that bit.
    pub async fn enabled(&self, repository: RepoId) -> Result<Option<bool>, StorageError> {
        Ok(sqlx::query_scalar("SELECT enabled FROM wiki_repositories WHERE repo_id=?")
            .bind(repository.0)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// Current pages after `after` (exclusive), at most `limit` (capped at
    /// [`MAX_WIKI_LISTING`]). `None` when the viewer may not read the wiki.
    pub async fn pages(
        &self,
        repository: RepoId,
        viewer: Option<AccountId>,
        after: Option<&WikiPath>,
        limit: u8,
    ) -> Result<Option<WikiPageList>, StorageError> {
        let limit = limit.clamp(1, MAX_WIKI_LISTING);
        let mut tx = self.pool.begin().await?;
        if Self::authorized(&mut tx, repository, viewer).await?.is_none() {
            return Ok(None);
        }
        let selected: Vec<EntryRow> = sqlx::query_as(
            "SELECT id,path,title,updated_at FROM wiki_pages WHERE repo_id=?1 AND path > ?2 COLLATE BINARY \
             ORDER BY path COLLATE BINARY LIMIT ?3",
        )
        .bind(repository.0)
        .bind(after.map_or("", WikiPath::as_str))
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *tx)
        .await?;
        let more = selected.len() > usize::from(limit);
        let entries = selected.into_iter().take(usize::from(limit)).map(EntryRow::decode).collect::<Result<Vec<_>, _>>()?;
        let next = if more { entries.last().map(|entry| entry.path.clone()) } else { None };
        Ok(Some(WikiPageList { entries, next }))
    }

    /// Pages whose title or path contains `query`, by path, at most `limit`. Content
    /// is not searched. `None` when the viewer may not read the wiki.
    pub async fn search(
        &self,
        repository: RepoId,
        viewer: Option<AccountId>,
        query: &str,
        limit: u8,
    ) -> Result<Option<Vec<WikiPageEntry>>, StorageError> {
        let limit = limit.clamp(1, MAX_WIKI_LISTING);
        let mut tx = self.pool.begin().await?;
        if Self::authorized(&mut tx, repository, viewer).await?.is_none() {
            return Ok(None);
        }
        let pattern = contains(query);
        let selected: Vec<EntryRow> = sqlx::query_as(
            "SELECT id,path,title,updated_at FROM wiki_pages WHERE repo_id=?1 \
             AND (title LIKE ?2 ESCAPE '\\' OR path LIKE ?2 ESCAPE '\\') ORDER BY path COLLATE BINARY LIMIT ?3",
        )
        .bind(repository.0)
        .bind(pattern)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        Ok(Some(selected.into_iter().map(EntryRow::decode).collect::<Result<Vec<_>, _>>()?))
    }
}

#[cfg(test)]
mod tests {
    use super::contains;

    #[test]
    fn search_patterns_match_wildcards_literally() {
        assert_eq!(contains("Setup"), "%Setup%");
        assert_eq!(contains("100%_done\\"), "%100\\%\\_done\\\\%");
        assert_eq!(contains(""), "%%");
    }
}
