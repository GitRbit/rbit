//! Moving a bare repository to another owner's directory when it is transferred
//! (`<root>/<old owner id>/<id>.git` → `<root>/<new owner id>/<id>.git`).
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Both paths live under the same storage root, so the move is a single same-filesystem
//! `rename`: it either happens completely or not at all, and never copies data. The
//! target's parent directory is created when the new owner has no repositories yet.

use std::path::PathBuf;

use rustit_core::{AccountId, RepoId};

use crate::{GitError, RepositoryStorage};

/// Moves repository `repo` from `from`'s directory to `to`'s and returns the new path.
///
/// [`GitError::NotFound`] if the repository is not at its old path, and
/// [`GitError::AlreadyExists`] if something already occupies the new one (nothing is
/// ever overwritten). Moving a repository back is the same call with the owners swapped.
pub async fn relocate(storage: &RepositoryStorage, repo: RepoId, from: AccountId, to: AccountId) -> Result<PathBuf, GitError> {
    let source = storage.path_for(from, repo);
    let target = storage.path_for(to, repo);
    if source == target {
        return Ok(target);
    }

    let moved = target.clone();
    tokio::task::spawn_blocking(move || -> Result<(), GitError> {
        if !source.is_dir() {
            return Err(GitError::NotFound("repository"));
        }
        if target.exists() {
            return Err(GitError::AlreadyExists);
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&source, &target)?;
        Ok(())
    })
    .await??;

    tracing::info!(repo = repo.0, from = from.0, to = to.0, path = %moved.display(), "relocated bare repository");
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn moves_the_repository_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(dir.path());
        let original = storage.create(AccountId(1), RepoId(7), "main").await.unwrap();
        std::fs::write(original.join("marker"), "kept").unwrap();

        let moved = relocate(&storage, RepoId(7), AccountId(1), AccountId(2)).await.unwrap();
        assert_eq!(moved, storage.path_for(AccountId(2), RepoId(7)));
        assert!(!original.exists());
        assert_eq!(std::fs::read_to_string(moved.join("marker")).unwrap(), "kept");
        assert_eq!(std::fs::read_to_string(moved.join("HEAD")).unwrap(), "ref: refs/heads/main\n");

        relocate(&storage, RepoId(7), AccountId(2), AccountId(1)).await.unwrap();
        assert!(original.join("marker").exists());
    }

    #[tokio::test]
    async fn never_overwrites_and_reports_missing_sources() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(dir.path());
        storage.create(AccountId(1), RepoId(7), "main").await.unwrap();
        storage.create(AccountId(2), RepoId(7), "main").await.unwrap();

        assert!(matches!(relocate(&storage, RepoId(7), AccountId(1), AccountId(2)).await, Err(GitError::AlreadyExists)));
        assert!(storage.path_for(AccountId(1), RepoId(7)).exists(), "the source is untouched");
        assert!(matches!(relocate(&storage, RepoId(8), AccountId(1), AccountId(3)).await, Err(GitError::NotFound(_))));
        assert_eq!(relocate(&storage, RepoId(7), AccountId(1), AccountId(1)).await.unwrap(), storage.path_for(AccountId(1), RepoId(7)));
    }
}
