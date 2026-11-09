//! Bounded history at the current authorized snapshot, without per-row SQL.
use super::{WikiCatalogStore, rows};
use crate::StorageError;
use rbit_core::Timestamp;
use rbit_wikis::{
    change::WikiChange,
    reading::{WikiHistory, WikiHistoryEntry, WikiHistoryRequest},
};

#[derive(sqlx::FromRow)]
struct HistoryRow {
    generation: i64,
    revision: String,
    path: String,
    title: String,
    at: i64,
    author: Option<String>,
    change: Option<String>,
}

impl WikiCatalogStore {
    pub(super) async fn read_history(&self, request: WikiHistoryRequest) -> Result<Option<WikiHistory>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let Some(meta) = Self::authorized(&mut tx, request.repository, request.viewer).await? else { return Ok(None) };
        let (_, _, head) = meta.decode()?;
        let Some(head) = head else { return Ok(None) };
        // Deleted pages keep their history.
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM wiki_pages WHERE repo_id=?1 AND id=?2 \
             UNION ALL SELECT 1 FROM wiki_page_tombstones WHERE repo_id=?1 AND page_id=?2",
        )
        .bind(request.repository.0)
        .bind(request.page.get())
        .fetch_optional(&mut *tx)
        .await?;
        if exists.is_none() {
            return Ok(None);
        }
        // An inclusive upper bound preserves i64::MAX while making deep cursors
        // an indexed range, not a nullable-OR filter that scans newer history first.
        let upper = request.before.map_or(head.generation.get(), |before| before.get() - 1).min(head.generation.get());
        let selected: Vec<HistoryRow> = sqlx::query_as(
            "SELECT h.generation,v.revision,h.path,h.title,v.created_at AS at,v.author,v.change FROM wiki_page_revisions h \
             JOIN wiki_revisions v ON v.repo_id=h.repo_id AND v.generation=h.generation \
             WHERE h.repo_id=?1 AND h.page_id=?2 AND h.generation<=?3 \
             ORDER BY h.generation DESC LIMIT ?4",
        )
        .bind(request.repository.0)
        .bind(request.page.get())
        .bind(upper)
        .bind(i64::from(request.limit.get()) + 1)
        .fetch_all(&mut *tx)
        .await?;
        let limit = usize::from(request.limit.get());
        let has_more = selected.len() > limit;
        let mut entries: Vec<WikiHistoryEntry> = Vec::with_capacity(selected.len().min(limit));
        // The extra row past the limit only tells whether the last entry moved the page.
        for (index, row) in selected.into_iter().enumerate() {
            let (path, title) = rows::names(row.path, row.title)?;
            if let Some(newer) = entries.last_mut().filter(|newer| newer.path != path) {
                newer.renamed_from = Some(path.clone());
            }
            if index == limit {
                break;
            }
            let change = row.change.as_deref().map(|code| WikiChange::from_code(code).ok_or_else(rows::corrupt)).transpose()?;
            entries.push(WikiHistoryEntry {
                head: rows::head(row.generation, &row.revision)?,
                path,
                title,
                at: Timestamp(row.at),
                author: row.author,
                change,
                renamed_from: None,
            });
        }
        let next = if has_more { entries.last().map(|entry| entry.head.generation) } else { None };
        Ok(Some(WikiHistory { entries, next }))
    }
}
