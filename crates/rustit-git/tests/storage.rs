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
async fn tags_can_be_created_from_a_branch_once() {
    let root = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(root.path());
    let bare = storage.create(AccountId(1), RepoId(1), "main").await.unwrap();
    let work = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(work.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(work.path().join("a.txt"), "a\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "first"]);
    git(&["push", "-q", bare.to_str().unwrap(), "HEAD:refs/heads/main"]);

    let reader = storage.reader(AccountId(1), RepoId(1));
    let main = reader.commit_id("main".into()).await.unwrap();
    assert_eq!(reader.create_tag("v1.0".into(), "main".into()).await.unwrap(), main);
    assert_eq!(reader.commit_id("v1.0".into()).await.unwrap(), main);
    assert!(matches!(reader.create_tag("v1.0".into(), "main".into()).await, Err(GitError::AlreadyExists)));
    for bad in ["", "a..b", "with space", "-flag", "x.lock"] {
        assert!(matches!(reader.create_tag(bad.into(), "main".into()).await, Err(GitError::InvalidBranchName(_))), "{bad:?}");
    }
    assert!(matches!(reader.create_tag("v2".into(), "nope".into()).await, Err(GitError::NotFound(_))));
}

#[tokio::test]
async fn refuses_to_overwrite_and_rejects_bad_branch_names() {
    let root = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(root.path());

    storage.create(AccountId(1), RepoId(1), "main").await.unwrap();
    assert!(matches!(storage.create(AccountId(1), RepoId(1), "main").await, Err(GitError::AlreadyExists)));
    assert!(matches!(storage.create(AccountId(1), RepoId(2), "bad..name").await, Err(GitError::InvalidBranchName(_))));
}
