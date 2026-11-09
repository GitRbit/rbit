//! Creating, editing, renaming, restoring and deleting wiki pages.
//!
//! A change prepares an unpublished commit on the current head, then publishes it in
//! one SQLite transaction that rechecks the actor's current write access, the archive
//! state, the head and the page's own last change (see
//! `rbit_storage::sqlite::wiki_mutations::publish`). Edit forms carry the whole-wiki
//! generation they were read at: a change of the same page since then is a conflict,
//! while changes of other pages are not (the change is prepared on the newer head).
use rbit_core::Timestamp;
use rbit_git::wikis::changes::{PrepareWikiChange, WikiTreeChange};
use rbit_identity_models::account::User;
use rbit_storage::sqlite::wiki_mutations::{
    page_effects::WikiPageEffect,
    publish::{WikiPublication, WikiPublishError},
};
use rbit_wikis::{WikiContent, WikiGeneration, WikiPageId, WikiPath, WikiTitle, change::WikiChange};

use super::{errors::WikiError, pages::WikiService};
use crate::RepoContext;

/// How often a change is prepared again when another publication moved the head.
const PUBLISH_ATTEMPTS: usize = 3;

/// Raw form fields of a page. An empty path is derived from the title.
#[derive(Debug, Clone, Default)]
pub struct WikiPageInput {
    pub title: String,
    pub path: String,
    pub content: String,
}

/// Validated page fields.
struct Fields {
    title: WikiTitle,
    path: WikiPath,
    content: WikiContent,
}

/// `Getting started` → `Getting-started`: whitespace becomes `-`, characters a path
/// cannot hold are dropped. The domain path rules still decide validity.
pub fn path_from_title(title: &str) -> String {
    let mut path = String::with_capacity(title.len());
    for character in title.trim().chars() {
        if character.is_whitespace() || character == '/' {
            if !path.is_empty() && !path.ends_with('-') {
                path.push('-');
            }
        } else if character.is_alphanumeric() || matches!(character, '-' | '_') {
            path.push(character);
        }
    }
    path.trim_matches('-').to_owned()
}

impl WikiPageInput {
    fn validate(&self) -> Result<Fields, WikiError> {
        let title = WikiTitle::try_from(self.title.clone()).map_err(WikiError::InvalidTitle)?;
        let path = if self.path.trim().is_empty() { path_from_title(title.as_str()) } else { self.path.trim().to_owned() };
        let path = WikiPath::try_from(path).map_err(WikiError::InvalidPath)?;
        // Browsers submit CRLF line breaks; pages keep the LF form Git tools expect.
        let content = WikiContent::try_from(self.content.replace("\r\n", "\n")).map_err(WikiError::InvalidContent)?;
        Ok(Fields { title, path, content })
    }
}

/// The tree effect of one change, rebuilt for every preparation attempt.
#[derive(Clone, Copy)]
enum Tree<'a> {
    Upsert(&'a str),
    Remove,
    Keep,
}

struct Change<'a> {
    page: WikiPageId,
    base: Option<WikiGeneration>,
    effect: WikiPageEffect<'a>,
    tree: Tree<'a>,
    kind: WikiChange,
}

impl WikiService {
    fn ensure_writer(repo: &RepoContext) -> Result<(), WikiError> {
        if !repo.access.can_write() {
            return Err(WikiError::Forbidden);
        }
        if repo.repo.repo.is_archived() {
            return Err(WikiError::Archived);
        }
        Ok(())
    }

    /// Prepares and publishes one change; storage rechecks everything at commit.
    async fn apply(&self, repo: &RepoContext, user: &User, change: Change<'_>) -> Result<(), WikiError> {
        Self::ensure_writer(repo)?;
        let _one_change = self.writes.lock().await;
        let objects = self.objects().await?;
        for _ in 0..PUBLISH_ATTEMPTS {
            let selection = self.catalog.page_by_id(repo.repo.repo.id, Some(user.id()), change.page).await?.ok_or(WikiError::NotFound)?;
            let at = Timestamp::now();
            let tree = match change.tree {
                Tree::Upsert(text) => WikiTreeChange::Upsert(WikiContent::try_from(text.to_owned()).map_err(WikiError::InvalidContent)?),
                Tree::Remove => WikiTreeChange::Remove,
                Tree::Keep => WikiTreeChange::Keep,
            };
            let target = objects
                .prepare_change(PrepareWikiChange {
                    repository: repo.repo.repo.id,
                    wiki: selection.wiki,
                    page: change.page,
                    parent: selection.head.map(|head| head.revision),
                    change: tree,
                    actor: user.id(),
                    at,
                })
                .await?;
            let published = self
                .mutations
                .publish(WikiPublication {
                    repository: repo.repo.repo.id,
                    actor: user.id(),
                    author: user.name().as_str(),
                    parent: selection.head,
                    target,
                    page: change.page,
                    base: change.base,
                    effect: change.effect,
                    change: change.kind,
                    at,
                })
                .await;
            match published {
                Ok(_) => return Ok(()),
                Err(WikiPublishError::Moved) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(WikiError::Unavailable)
    }

    /// Creates a page; returns its path.
    pub async fn create(&self, repo: &RepoContext, user: &User, input: &WikiPageInput) -> Result<WikiPath, WikiError> {
        Self::ensure_writer(repo)?;
        let fields = input.validate()?;
        let page = self.mutations.allocate_page().await?;
        let change = Change {
            page,
            base: None,
            effect: WikiPageEffect::Write { path: &fields.path, title: &fields.title },
            tree: Tree::Upsert(fields.content.as_str()),
            kind: WikiChange::Create,
        };
        self.apply(repo, user, change).await?;
        Ok(fields.path)
    }

    /// Edits (and possibly renames) the page as read at the whole-wiki generation `base`.
    pub async fn edit(
        &self,
        repo: &RepoContext,
        user: &User,
        page: WikiPageId,
        base: WikiGeneration,
        input: &WikiPageInput,
    ) -> Result<WikiPath, WikiError> {
        Self::ensure_writer(repo)?;
        let fields = input.validate()?;
        let current = self.page_by_id(repo, Some(user), page).await?;
        let same_content = current.content.as_str() == fields.content.as_str();
        if same_content && current.page.title == fields.title && current.page.path == fields.path {
            return Err(WikiError::Unchanged);
        }
        let (tree, kind) =
            if same_content { (Tree::Keep, WikiChange::Rename) } else { (Tree::Upsert(fields.content.as_str()), WikiChange::Edit) };
        let effect = WikiPageEffect::Write { path: &fields.path, title: &fields.title };
        let change = Change { page, base: Some(base), effect, tree, kind };
        self.apply(repo, user, change).await?;
        Ok(fields.path)
    }

    /// Makes the content of revision `generation` the page's new current content,
    /// keeping its current title and path. History is never rewritten.
    pub async fn restore(
        &self,
        repo: &RepoContext,
        user: &User,
        page: WikiPageId,
        generation: WikiGeneration,
        base: WikiGeneration,
    ) -> Result<WikiPath, WikiError> {
        Self::ensure_writer(repo)?;
        let old = self.revision(repo, Some(user), page, generation).await?;
        let current = self.page_by_id(repo, Some(user), page).await?;
        if old.content.as_str() == current.content.as_str() {
            return Err(WikiError::Unchanged);
        }
        let (path, title) = (current.page.path, current.page.title);
        let change = Change {
            page,
            base: Some(base),
            effect: WikiPageEffect::Write { path: &path, title: &title },
            tree: Tree::Upsert(old.content.as_str()),
            kind: WikiChange::Restore,
        };
        self.apply(repo, user, change).await?;
        Ok(path)
    }

    /// Deletes the page as read at `base`. Its history stays readable and the page can
    /// be restored with [`Self::undelete`].
    pub async fn delete(&self, repo: &RepoContext, user: &User, page: WikiPageId, base: WikiGeneration) -> Result<(), WikiError> {
        let change = Change { page, base: Some(base), effect: WikiPageEffect::Delete, tree: Tree::Remove, kind: WikiChange::Delete };
        self.apply(repo, user, change).await
    }

    /// Restores a deleted page, as listed at `base`, with the content and names it had
    /// when it was deleted. Refused with `PathTaken` while another page uses its path.
    pub async fn undelete(&self, repo: &RepoContext, user: &User, page: WikiPageId, base: WikiGeneration) -> Result<WikiPath, WikiError> {
        Self::ensure_writer(repo)?;
        let deleted = self.deleted_page(repo, Some(user), page).await?;
        let last = self.previous(repo, Some(user), page, deleted.deleted).await?.ok_or(WikiError::NotFound)?;
        let content = self.revision(repo, Some(user), page, last).await?.content;
        let (path, title) = (deleted.path, deleted.title);
        let change = Change {
            page,
            base: Some(base),
            effect: WikiPageEffect::Undelete { path: &path, title: &title },
            tree: Tree::Upsert(content.as_str()),
            kind: WikiChange::Restore,
        };
        self.apply(repo, user, change).await?;
        Ok(path)
    }

    /// Switches the wiki on or off. Repository administrators only; never while archived.
    pub async fn set_enabled(&self, repo: &RepoContext, enabled: bool) -> Result<(), WikiError> {
        if !repo.access.can_admin() {
            return Err(WikiError::Forbidden);
        }
        if repo.repo.repo.is_archived() {
            return Err(WikiError::Archived);
        }
        Ok(self.mutations.set_enabled(repo.repo.repo.id, enabled).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::path_from_title;

    #[test]
    fn paths_derive_from_titles_without_inventing_separators() {
        assert_eq!(path_from_title("Getting started"), "Getting-started");
        assert_eq!(path_from_title("  API / Tokens: v2  "), "API-Tokens-v2");
        assert_eq!(path_from_title("Café  au lait"), "Café-au-lait");
        assert_eq!(path_from_title("<script>"), "script");
        assert_eq!(path_from_title("!!!"), "");
    }
}
