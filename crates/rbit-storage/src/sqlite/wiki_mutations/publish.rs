//! Visible wiki publication: one writer transaction makes prepared immutable objects
//! the new whole-wiki head.
//!
//! The dedicated object store has no refs: a generation becomes visible only when this
//! transaction commits its `wiki_revisions` row and moves `wiki_repositories.generation`.
//! Objects prepared for a refused or crashed publication stay unreferenced. Inside the
//! transaction the actor's CURRENT repository access, archive state, the absence of an
//! accepted intent, the exact parent head and the page's own last change are rechecked,
//! so a stale form or a revoked grant can never publish. It runs on the general pool and
//! therefore only while the writer protocol is in its legacy mode; managed modes refuse
//! these writes in SQLite itself.
use super::{WikiMutationError, WikiMutationStore, authorization};
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_wikis::{
    WikiGeneration, WikiPageId, WikiPath, WikiRevisionId, WikiTitle,
    change::WikiChange,
    policy::{WikiExpected, WikiHead, WikiMutationState, WikiPolicyError, check_write},
};

/// One finalized page change. The caller prepared `target` on top of `parent`.
#[derive(Debug)]
pub struct WikiPublication<'a> {
    pub repository: RepoId,
    pub actor: AccountId,
    /// Display name recorded in history; never an authorization fact.
    pub author: &'a str,
    /// The visible whole-wiki head the objects were prepared on (`None`: first page).
    pub parent: Option<WikiHead>,
    pub target: WikiRevisionId,
    pub page: WikiPageId,
    /// For an existing page, the whole-wiki generation its form was read at. A change
    /// of this page after that generation is a conflict; other pages' changes are not.
    pub base: Option<WikiGeneration>,
    /// The page's names after the change; `None` deletes the page.
    pub names: Option<(&'a WikiPath, &'a WikiTitle)>,
    pub change: WikiChange,
    pub at: Timestamp,
}

/// Why a publication was refused. None of these made anything visible.
#[derive(Debug)]
pub enum WikiPublishError {
    /// Permission, archive, pending intent, page conflict or exhausted generations.
    Policy(WikiPolicyError),
    /// Another publication moved the head after preparation; prepare again.
    Moved,
    /// Another current page already uses the requested path.
    PathTaken,
    Mutation(WikiMutationError),
}

impl From<sqlx::Error> for WikiPublishError {
    fn from(error: sqlx::Error) -> Self {
        Self::Mutation(error.into())
    }
}

impl From<WikiMutationError> for WikiPublishError {
    fn from(error: WikiMutationError) -> Self {
        match error {
            WikiMutationError::Policy(policy) => Self::Policy(policy),
            other => Self::Mutation(other),
        }
    }
}

impl WikiMutationStore {
    /// Reserves a never-reused page ID before objects are prepared, using the same
    /// allocation floor as accepted intents. An unused reservation is simply skipped.
    pub async fn allocate_page(&self) -> Result<WikiPageId, WikiMutationError> {
        let mut tx = self.pool.begin_immediate().await?;
        let floor: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0) FROM sqlite_sequence WHERE name IN ('wiki_pages','wiki_page_allocations')")
                .fetch_one(&mut *tx)
                .await?;
        let id = floor.checked_add(1).and_then(WikiPageId::new).ok_or(WikiMutationError::Exhausted)?;
        sqlx::query("INSERT INTO wiki_page_allocations(id) VALUES(?)").bind(id.get()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wiki_page_allocations WHERE id=?").bind(id.get()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Turns the repository's wiki on or off, creating its catalog incarnation once.
    /// The caller must have checked current administrator access and archive state.
    pub async fn set_enabled(&self, repository: RepoId, enabled: bool) -> Result<(), WikiMutationError> {
        sqlx::query(
            "INSERT INTO wiki_repositories(repo_id,enabled) VALUES(?1,?2) ON CONFLICT(repo_id) DO UPDATE SET enabled=excluded.enabled",
        )
        .bind(repository.0)
        .bind(enabled)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Publishes `publication` as the next generation, or refuses with nothing visible.
    pub async fn publish(&self, publication: WikiPublication<'_>) -> Result<WikiGeneration, WikiPublishError> {
        let WikiPublication { repository, actor, author, parent, target, page, base, names, change, at } = publication;
        let mut tx = self.pool.begin_immediate().await?;
        let authorized = authorization::load(&mut tx, repository, actor).await?.decode(repository, actor)?;
        // Permission and archive first: the probe matches itself, so only access decides.
        let expected = match base {
            None => WikiExpected::Create { repository, wiki: authorized.wiki, head: parent },
            Some(_) => WikiExpected::Change {
                repository,
                wiki: authorized.wiki,
                page,
                head: parent.ok_or(WikiPublishError::Policy(WikiPolicyError::Conflict))?,
            },
        };
        let probe = WikiMutationState { repository, wiki: authorized.wiki, head: parent, page: base.map(|_| page), pending: false };
        let generation = check_write(&authorized.access, expected, &probe).map_err(WikiPublishError::Policy)?;
        let pending: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_mutation_intents WHERE wiki_id=? AND state IN ('pending','quarantined'))")
                .bind(authorized.wiki.get())
                .fetch_one(&mut *tx)
                .await?;
        if pending {
            return Err(WikiPublishError::Policy(WikiPolicyError::Pending));
        }
        if authorized.head != parent {
            return Err(WikiPublishError::Moved);
        }
        if let Some(base) = base {
            let changed: Option<Option<i64>> = sqlx::query_scalar(
                "SELECT (SELECT MAX(generation) FROM wiki_page_revisions h WHERE h.repo_id=p.repo_id AND h.page_id=p.id) \
                 FROM wiki_pages p WHERE p.repo_id=?1 AND p.id=?2",
            )
            .bind(repository.0)
            .bind(page.get())
            .fetch_optional(&mut *tx)
            .await?;
            match changed {
                Some(Some(changed)) if changed <= base.get() => {}
                Some(_) => return Err(WikiPublishError::Policy(WikiPolicyError::Conflict)),
                None => return Err(WikiPublishError::Policy(WikiPolicyError::NotFound)),
            }
        }
        if let Some((path, _)) = names {
            let taken: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_pages WHERE repo_id=?1 AND path=?2 COLLATE BINARY AND id<>?3)")
                    .bind(repository.0)
                    .bind(path.as_str())
                    .bind(page.get())
                    .fetch_one(&mut *tx)
                    .await?;
            if taken {
                return Err(WikiPublishError::PathTaken);
            }
        }
        sqlx::query("INSERT INTO wiki_revisions(repo_id,generation,revision,created_at,author,change) VALUES(?,?,?,?,?,?)")
            .bind(repository.0)
            .bind(generation.get())
            .bind(target.to_string())
            .bind(at.seconds())
            .bind(author)
            .bind(change.code())
            .execute(&mut *tx)
            .await?;
        match names {
            Some((path, title)) => {
                sqlx::query(
                    "INSERT INTO wiki_pages(id,repo_id,path,title,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) \
                     ON CONFLICT(id) DO UPDATE SET path=excluded.path,title=excluded.title,updated_at=excluded.updated_at",
                )
                .bind(page.get())
                .bind(repository.0)
                .bind(path.as_str())
                .bind(title.as_str())
                .bind(at.seconds())
                .execute(&mut *tx)
                .await?;
                sqlx::query("INSERT INTO wiki_page_revisions(repo_id,page_id,generation,path,title) VALUES(?,?,?,?,?)")
                    .bind(repository.0)
                    .bind(page.get())
                    .bind(generation.get())
                    .bind(path.as_str())
                    .bind(title.as_str())
                    .execute(&mut *tx)
                    .await?;
            }
            // Deletion removes the catalog page and its per-page history rows; the
            // whole-wiki revisions and their Git objects remain.
            None => {
                sqlx::query("DELETE FROM wiki_pages WHERE repo_id=? AND id=?")
                    .bind(repository.0)
                    .bind(page.get())
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("UPDATE wiki_repositories SET generation=? WHERE repo_id=?")
            .bind(generation.get())
            .bind(repository.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(generation)
    }
}
