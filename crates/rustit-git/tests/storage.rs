//! Repository creation and reading against a real on-disk repository.

use rustit_core::{AccountId, RepoId};
use rustit_git::{GitError, RepositoryStorage};

#[tokio::test]
async fn creates_empty_bare_repository_with_configured_head() {
    let root = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(root.path());

    let path = storage.create(AccountId(7), RepoId(42), "trunk").await.unwrap();
    assert!(path.ends_with("7/42.git") || path.ends_with("7\\42.git"));
    assert_eq!(std::fs::read_to_string(path.join("HEAD")).unwrap().trim(), "ref: refs/heads/trunk");

    let reader = storage.reader(AccountId(7), RepoId(42));
    assert!(reader.is_empty().await.unwrap());
    assert!(reader.refs().await.unwrap().is_empty());
}

#[tokio::test]
async fn refuses_to_overwrite_and_rejects_bad_branch_names() {
    let root = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(root.path());

    storage.create(AccountId(1), RepoId(1), "main").await.unwrap();
    assert!(matches!(storage.create(AccountId(1), RepoId(1), "main").await, Err(GitError::AlreadyExists)));
    assert!(matches!(storage.create(AccountId(1), RepoId(2), "bad..name").await, Err(GitError::InvalidBranchName(_))));
}
