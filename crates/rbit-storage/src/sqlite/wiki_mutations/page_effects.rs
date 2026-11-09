//! What one publication does to its page inside the publishing transaction: the page's
//! expected state is rechecked, then its catalog row, history, tombstone and aliases
//! change together with the new generation.
//!
//! * A deletion keeps the page's history and leaves a tombstone with its last names;
//!   the page ID is never reused, so restoring brings back the same page and history.
//! * A rename records the old path as an alias of the page. A current page always wins
//!   over an alias, and taking a path (create, rename, restore) removes its alias in the
//!   same transaction, so an alias never points at a path that resolves to itself.
use super::{WikiMutationError, publish::WikiPublishError};
use crate::sqlite::pool::WriteTransaction;
use rbit_core::{RepoId, Timestamp};
use rbit_wikis::{WikiGeneration, WikiPageId, WikiPath, WikiTitle, policy::WikiPolicyError};

/// The page after a publication.
#[derive(Debug, Clone, Copy)]
pub enum WikiPageEffect<'a> {
    /// The page is current with these names (create, edit, rename, restore a revision).
    Write { path: &'a WikiPath, title: &'a WikiTitle },
    /// The current page becomes a tombstone; its history stays.
    Delete,
    /// A deleted page becomes current again under these names.
    Undelete { path: &'a WikiPath, title: &'a WikiTitle },
}

impl<'a> WikiPageEffect<'a> {
    fn names(self) -> Option<(&'a WikiPath, &'a WikiTitle)> {
        match self {
            Self::Write { path, title } | Self::Undelete { path, title } => Some((path, title)),
            Self::Delete => None,
        }
    }
}

fn refused(error: WikiPolicyError) -> WikiPublishError {
    WikiPublishError::Policy(error)
}

/// Rechecks the page against the form's `base` generation and the requested names.
/// `base` is `None` only for a page that does not exist yet.
pub(super) async fn check(
    tx: &mut WriteTransaction,
    repository: RepoId,
    page: WikiPageId,
    base: Option<WikiGeneration>,
    effect: WikiPageEffect<'_>,
) -> Result<(), WikiPublishError> {
    if let Some(base) = base {
        let (current, deleted, changed): (bool, bool, Option<i64>) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM wiki_pages WHERE repo_id=?1 AND id=?2), \
             EXISTS(SELECT 1 FROM wiki_page_tombstones WHERE repo_id=?1 AND page_id=?2), \
             (SELECT MAX(generation) FROM wiki_page_revisions WHERE repo_id=?1 AND page_id=?2)",
        )
        .bind(repository.0)
        .bind(page.get())
        .fetch_one(&mut **tx)
        .await?;
        match (effect, current, deleted) {
            (WikiPageEffect::Write { .. } | WikiPageEffect::Delete, true, _) | (WikiPageEffect::Undelete { .. }, false, true) => {}
            // Deleted since the form was read, or restored by someone else meanwhile.
            (WikiPageEffect::Write { .. } | WikiPageEffect::Delete, false, true) | (WikiPageEffect::Undelete { .. }, true, _) => {
                return Err(refused(WikiPolicyError::Conflict));
            }
            _ => return Err(refused(WikiPolicyError::NotFound)),
        }
        if changed.is_none_or(|changed| changed > base.get()) {
            return Err(refused(WikiPolicyError::Conflict));
        }
    }
    if let Some((path, _)) = effect.names() {
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_pages WHERE repo_id=?1 AND path=?2 COLLATE BINARY AND id<>?3)")
                .bind(repository.0)
                .bind(path.as_str())
                .bind(page.get())
                .fetch_one(&mut **tx)
                .await?;
        if taken {
            return Err(WikiPublishError::PathTaken);
        }
    }
    Ok(())
}

/// Applies `effect` at `generation`, whose `wiki_revisions` row is already inserted.
pub(super) async fn apply(
    tx: &mut WriteTransaction,
    repository: RepoId,
    page: WikiPageId,
    generation: WikiGeneration,
    effect: WikiPageEffect<'_>,
    at: Timestamp,
) -> Result<(), WikiMutationError> {
    let previous: Option<(String, String, i64)> = sqlx::query_as("SELECT path,title,created_at FROM wiki_pages WHERE repo_id=?1 AND id=?2")
        .bind(repository.0)
        .bind(page.get())
        .fetch_optional(&mut **tx)
        .await?;
    let (path, title) = match effect {
        WikiPageEffect::Write { path, title } => {
            sqlx::query(
                "INSERT INTO wiki_pages(id,repo_id,path,title,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) \
                 ON CONFLICT(id) DO UPDATE SET path=excluded.path,title=excluded.title,updated_at=excluded.updated_at",
            )
            .bind(page.get())
            .bind(repository.0)
            .bind(path.as_str())
            .bind(title.as_str())
            .bind(at.seconds())
            .execute(&mut **tx)
            .await?;
            if let Some((old, _, _)) = previous.as_ref().filter(|(old, _, _)| old != path.as_str()) {
                sqlx::query(
                    "INSERT INTO wiki_page_aliases(repo_id,path,page_id,generation) VALUES(?1,?2,?3,?4) \
                     ON CONFLICT(repo_id,path) DO UPDATE SET page_id=excluded.page_id,generation=excluded.generation",
                )
                .bind(repository.0)
                .bind(old)
                .bind(page.get())
                .bind(generation.get())
                .execute(&mut **tx)
                .await?;
            }
            (path.as_str().to_owned(), title.as_str().to_owned())
        }
        WikiPageEffect::Undelete { path, title } => {
            let created: i64 = sqlx::query_scalar("DELETE FROM wiki_page_tombstones WHERE repo_id=?1 AND page_id=?2 RETURNING created_at")
                .bind(repository.0)
                .bind(page.get())
                .fetch_one(&mut **tx)
                .await?;
            sqlx::query("INSERT INTO wiki_pages(id,repo_id,path,title,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)")
                .bind(page.get())
                .bind(repository.0)
                .bind(path.as_str())
                .bind(title.as_str())
                .bind(created)
                .bind(at.seconds())
                .execute(&mut **tx)
                .await?;
            (path.as_str().to_owned(), title.as_str().to_owned())
        }
        WikiPageEffect::Delete => {
            let (path, title, created) = previous.ok_or(WikiMutationError::Policy(WikiPolicyError::NotFound))?;
            sqlx::query(
                "INSERT INTO wiki_page_tombstones(repo_id,page_id,path,title,created_at,deleted_generation) VALUES(?1,?2,?3,?4,?5,?6)",
            )
            .bind(repository.0)
            .bind(page.get())
            .bind(&path)
            .bind(&title)
            .bind(created)
            .bind(generation.get())
            .execute(&mut **tx)
            .await?;
            sqlx::query("DELETE FROM wiki_pages WHERE repo_id=?1 AND id=?2").bind(repository.0).bind(page.get()).execute(&mut **tx).await?;
            (path, title)
        }
    };
    if effect.names().is_some() {
        // The page now holds this path: an alias of it would only shadow nothing.
        sqlx::query("DELETE FROM wiki_page_aliases WHERE repo_id=?1 AND path=?2 COLLATE BINARY")
            .bind(repository.0)
            .bind(&path)
            .execute(&mut **tx)
            .await?;
    }
    // Every change, a deletion included, is one entry of the page's history.
    sqlx::query("INSERT INTO wiki_page_revisions(repo_id,page_id,generation,path,title) VALUES(?,?,?,?,?)")
        .bind(repository.0)
        .bind(page.get())
        .bind(generation.get())
        .bind(&path)
        .bind(&title)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
