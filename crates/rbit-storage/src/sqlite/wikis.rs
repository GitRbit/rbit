//! Finalized wiki catalog reads. There is deliberately no arbitrary-OID publisher.
mod history;
mod rows;

use super::{
    pool::{Pool, WriteTransaction},
    repositories::readable_by,
};
use crate::StorageError;
use rbit_core::{AccountId, RepoId};
use rbit_wikis::{
    WikiGeneration, WikiPageId, WikiPath, WikiStoreId,
    reading::{WikiCatalog, WikiCatalogPage, WikiHistory, WikiHistoryRequest, WikiPageRequest, WikiRevisionRequest},
};

use rows::{MetaRow, PageRow, RevisionPageRow};

// Same shared repository read predicate as ordinary lists, but administrator and
// suspension flags are resolved from the current SQL snapshot, never a caller bool.
const AUTHORIZED: &str = concat!(
    "SELECT w.id AS wiki_id, w.generation, v.revision, ",
    "(SELECT CASE WHEN length(identity)=16 THEN identity END FROM wiki_store_identity WHERE singleton=1) AS identity ",
    "FROM repositories r JOIN wiki_repositories w ON w.repo_id=r.id ",
    "LEFT JOIN wiki_revisions v ON v.repo_id=w.repo_id AND v.generation=w.generation ",
    "WHERE r.id=?1 AND w.enabled=1 AND (?2 IS NULL OR EXISTS ",
    "(SELECT 1 FROM user_logins active WHERE active.account_id=?2 AND active.suspended_at IS NULL)) AND ",
    readable_by!(
        "?2",
        "EXISTS (SELECT 1 FROM user_logins admin WHERE admin.account_id=?2 AND admin.is_admin=1 AND admin.suspended_at IS NULL)"
    )
);

#[derive(Debug, Clone)]
pub struct WikiCatalogStore {
    pool: Pool,
}

enum Selection<'a> {
    Path(&'a WikiPath),
    Revision(WikiPageId, WikiGeneration),
}

impl WikiCatalogStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Trusted composition binds the dedicated object root to this persistent catalog.
    /// The value is namespace data, not an authentication or publication receipt.
    pub async fn identity(&self) -> Result<WikiStoreId, StorageError> {
        let identity: Vec<u8> = sqlx::query_scalar("SELECT identity FROM wiki_store_identity WHERE singleton=1 AND length(identity)=16")
            .fetch_one(&self.pool)
            .await?;
        rows::identity(identity)
    }

    async fn authorized(tx: &mut WriteTransaction, repository: RepoId, viewer: Option<AccountId>) -> Result<Option<MetaRow>, StorageError> {
        Ok(sqlx::query_as(AUTHORIZED).bind(repository.0).bind(viewer.map(|id| id.0)).fetch_optional(&mut **tx).await?)
    }

    async fn select(
        &self,
        repository: RepoId,
        viewer: Option<AccountId>,
        selection: Selection<'_>,
    ) -> Result<Option<WikiCatalogPage>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let Some(meta) = Self::authorized(&mut tx, repository, viewer).await? else { return Ok(None) };
        let (store, wiki, head) = meta.decode()?;
        let Some(head) = head else {
            return match selection {
                Selection::Path(_) => Ok(Some(WikiCatalogPage { wiki, repository, store, head: None, page: None })),
                Selection::Revision(_, _) => Ok(None),
            };
        };
        let (head, page) = match selection {
            Selection::Path(path) => {
                let page: Option<PageRow> = sqlx::query_as(
                    "SELECT id,path,title,created_at,updated_at FROM wiki_pages WHERE repo_id=?1 AND path=?2 COLLATE BINARY",
                )
                .bind(repository.0)
                .bind(path.as_str())
                .fetch_optional(&mut *tx)
                .await?;
                (head, page.map(|page| page.decode(repository, wiki, head)).transpose()?)
            }
            Selection::Revision(page, generation) => {
                if generation > head.generation {
                    return Ok(None);
                }
                let selected: Option<RevisionPageRow> = sqlx::query_as(
                    "SELECT p.id,h.path,h.title,p.created_at,v.created_at AS updated_at,v.generation,v.revision \
                     FROM wiki_page_revisions h JOIN wiki_pages p ON p.repo_id=h.repo_id AND p.id=h.page_id \
                     JOIN wiki_revisions v ON v.repo_id=h.repo_id AND v.generation=h.generation \
                     WHERE h.repo_id=?1 AND h.page_id=?2 AND h.generation=?3",
                )
                .bind(repository.0)
                .bind(page.get())
                .bind(generation.get())
                .fetch_optional(&mut *tx)
                .await?;
                let Some(selected) = selected else { return Ok(None) };
                let head = rows::head(selected.generation, &selected.revision)?;
                (head, Some(selected.page.decode(repository, wiki, head)?))
            }
        };
        Ok(Some(WikiCatalogPage { wiki, repository, store, head: Some(head), page }))
    }
}

impl WikiCatalog for WikiCatalogStore {
    type Error = StorageError;
    async fn page(&self, request: WikiPageRequest<'_>) -> Result<Option<WikiCatalogPage>, Self::Error> {
        self.select(request.repository, request.viewer, Selection::Path(request.path)).await
    }
    async fn revision(&self, request: WikiRevisionRequest) -> Result<Option<WikiCatalogPage>, Self::Error> {
        self.select(request.repository, request.viewer, Selection::Revision(request.page, request.generation)).await
    }
    async fn history(&self, request: WikiHistoryRequest) -> Result<Option<WikiHistory>, Self::Error> {
        self.read_history(request).await
    }
}
