//! Moving a bare repository to another owner's directory when it is transferred
//! (`<root>/<old owner id>/<id>.git` → `<root>/<new owner id>/<id>.git`).
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Both paths live under the same storage root, so the move is a single same-filesystem
//! `rename`: it either happens completely or not at all, and never copies data. The
//! target's parent directory is created when the new owner has no repositories yet.
//!
//! Windows refuses to rename a directory while any file inside it is open. The move
//! therefore takes the repository's lease exclusively (see [`crate::lease`]): it waits for
//! readers and background jobs that have the repository open — after a push, the cache
//! warm-up is usually still reading it — and holds new ones back until it is done. A
//! pinned reader serving a page holds the lease for the whole request.
//!
//! Processes outside rbit's control can still have a file open for a moment: a `git`
//! process serving a clone, or a virus scanner or search indexer inspecting the pack a
//! push just wrote. On Windows a rename refused with a sharing violation or "access
//! denied" is therefore retried a few times with exponential backoff (about 1.5 s in
//! total) before the error is returned. Elsewhere renames never fail for that reason and
//! are not retried.

use std::{path::PathBuf, time::Duration};

use rbit_core::{AccountId, RepoId};

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
    let lease = storage.exclusive(repo).await;
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
        rename_with_retry(&source, &target)?;
        Ok(())
    })
    .await??;
    drop(lease);

    tracing::info!(repo = repo.0, from = from.0, to = to.0, path = %moved.display(), "relocated bare repository");
    Ok(moved)
}

/// Delays before each retry of a rename that another process blocks (Windows only).
const RETRY_DELAYS: [Duration; 6] = [
    Duration::from_millis(25),
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(800),
];

/// `ERROR_ACCESS_DENIED` and `ERROR_SHARING_VIOLATION`: what Windows reports when a file
/// inside the directory is open.
fn is_transient(error: &std::io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32))
}

fn rename_with_retry(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    let mut delays = RETRY_DELAYS.iter();
    loop {
        match std::fs::rename(source, target) {
            Err(error) if is_transient(&error) => {
                let Some(delay) = delays.next() else { return Err(error) };
                tracing::debug!(%error, path = %source.display(), ?delay, "repository directory is in use; retrying the move");
                std::thread::sleep(*delay);
            }
            result => return result,
        }
    }
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
    async fn waits_for_work_in_progress_inside_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(dir.path());
        let original = storage.create(AccountId(1), RepoId(7), "main").await.unwrap();

        // Work in progress: a lease and an open file, which on Windows blocks the rename
        // for as long as it stays open — far longer than any retry.
        let lease = storage.lease(RepoId(7)).await;
        let open = std::fs::File::open(original.join("HEAD")).unwrap();
        let moving = tokio::spawn({
            let storage = storage.clone();
            async move { relocate(&storage, RepoId(7), AccountId(1), AccountId(2)).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!moving.is_finished(), "the move waits for the lease");
        assert!(original.exists());
        drop(open);
        drop(lease);

        let moved = tokio::time::timeout(Duration::from_secs(5), moving).await.unwrap().unwrap().unwrap();
        assert!(moved.join("HEAD").exists() && !original.exists());
        // A reader taken from the storage takes the lease too, and so sees the move whole.
        assert!(storage.reader(AccountId(2), RepoId(7)).is_empty().await.unwrap());
    }

    #[tokio::test]
    async fn a_pinned_reader_keeps_the_repository_in_place_until_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(dir.path());
        storage.create(AccountId(1), RepoId(7), "main").await.unwrap();

        let page = storage.reader(AccountId(1), RepoId(7)).pinned();
        assert!(page.is_empty().await.unwrap(), "the first read opens and pins the repository");
        let moving = tokio::spawn({
            let storage = storage.clone();
            async move { relocate(&storage, RepoId(7), AccountId(1), AccountId(2)).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!moving.is_finished(), "the move waits for the page");
        // Further reads of the page use its lease: the waiting move does not block them.
        let clone = page.clone();
        assert!(tokio::time::timeout(Duration::from_secs(5), clone.refs()).await.unwrap().unwrap().is_empty());
        drop((page, clone));

        let moved = tokio::time::timeout(Duration::from_secs(5), moving).await.unwrap().unwrap().unwrap();
        assert_eq!(moved, storage.path_for(AccountId(2), RepoId(7)));
    }

    #[test]
    fn retries_only_errors_caused_by_open_files_on_windows() {
        let denied = std::io::Error::from_raw_os_error(5);
        let sharing = std::io::Error::from_raw_os_error(32);
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert_eq!(is_transient(&denied), cfg!(windows));
        assert_eq!(is_transient(&sharing), cfg!(windows));
        assert!(!is_transient(&missing));
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
