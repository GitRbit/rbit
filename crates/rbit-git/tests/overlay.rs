//! Comparing across a fork through an object overlay, and `refs/pull/{n}/head`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use rbit_git::{DiffLimits, RepositoryReader, overlay::Overlay, pull_refs};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(dir: &Path, file: &str, contents: &str, message: &str) {
    std::fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// Every object file and pack in a bare repository, to prove nothing was written.
fn object_files(repo: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() { walk(&path, out) } else { out.push(path) }
        }
    }
    let mut files = Vec::new();
    walk(&repo.join("objects"), &mut files);
    files.sort();
    files
}

/// A parent with `main`, a fork (bare clone) with a `feature` branch the parent lacks.
fn network() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q"]);
    commit(&work, "README.md", "# Demo\n", "base");
    let parent = dir.path().join("parent.git");
    git(dir.path(), &["clone", "-q", "--bare", work.to_str().unwrap(), parent.to_str().unwrap()]);
    let fork = dir.path().join("fork.git");
    git(dir.path(), &["clone", "-q", "--bare", parent.to_str().unwrap(), fork.to_str().unwrap()]);
    git(&work, &["checkout", "-q", "-b", "feature"]);
    commit(&work, "feature.txt", "fork work\n", "fork-1");
    git(&work, &["push", "-q", fork.to_str().unwrap(), "feature"]);
    git(&work, &["checkout", "-q", "main"]);
    commit(&work, "main.txt", "parent work\n", "parent-1");
    git(&work, &["push", "-q", parent.to_str().unwrap(), "main"]);
    (dir, parent, fork)
}

#[tokio::test]
async fn an_overlay_compares_a_fork_with_its_parent_without_writing_to_either() {
    let (dir, parent, fork) = network();
    let base = RepositoryReader::new(parent.clone()).commit_id("main".into()).await.unwrap();
    let head = RepositoryReader::new(fork.clone()).commit_id("feature".into()).await.unwrap();
    assert!(RepositoryReader::new(parent.clone()).commit_id(head.clone()).await.is_err(), "the parent lacks the fork's commit");
    let (parent_before, fork_before) = (object_files(&parent), object_files(&fork));

    let tmp = dir.path().join("tmp");
    let overlay = Overlay::create(&tmp, &[&parent, &fork]).await.unwrap();
    let comparison = overlay.reader().compare(base.clone(), head.clone(), DiffLimits::default(), 10).await.unwrap();
    assert_eq!((comparison.ahead, comparison.behind), (1, 1));
    assert_eq!(comparison.commits[0].summary, "fork-1");
    assert_eq!(comparison.files.len(), 1);
    assert_eq!(comparison.files[0].path, "feature.txt");
    let path = overlay.path().to_owned();
    drop(overlay);

    assert!(!path.exists());
    assert_eq!(object_files(&parent), parent_before);
    assert_eq!(object_files(&fork), fork_before);
}

#[tokio::test]
async fn pull_heads_follow_the_branch_and_survive_the_forks_deletion() {
    let (dir, parent, fork) = network();
    let git_binary = Path::new("git");
    assert_eq!(pull_refs::read(git_binary, &parent, 1).await.unwrap(), None);

    let fetched = pull_refs::fetch(git_binary, &parent, &fork, "feature", 1).await.unwrap();
    let head = RepositoryReader::new(fork.clone()).commit_id("feature".into()).await.unwrap();
    assert_eq!(fetched, head);
    assert_eq!(pull_refs::read(git_binary, &parent, 1).await.unwrap().as_deref(), Some(head.as_str()));

    // The fork's branch moves: fetching again follows it (forced, so rewrites work too).
    let work = dir.path().join("work");
    git(&work, &["checkout", "-q", "feature"]);
    git(&work, &["commit", "-q", "--amend", "-m", "fork-1 (amended)"]);
    git(&work, &["push", "-q", "--force", fork.to_str().unwrap(), "feature"]);
    let amended = pull_refs::fetch(git_binary, &parent, &fork, "feature", 1).await.unwrap();
    assert_ne!(amended, head);

    std::fs::remove_dir_all(&fork).unwrap();
    let reader = RepositoryReader::new(parent.clone());
    assert_eq!(reader.commit_id("refs/pull/1/head".into()).await.unwrap(), amended);
    assert_eq!(reader.history(amended.clone(), 0, 1).await.unwrap()[0].summary, "fork-1 (amended)");

    // A same-repository head is set directly.
    let main = reader.commit_id("main".into()).await.unwrap();
    pull_refs::set(git_binary, &parent, 2, &main).await.unwrap();
    assert_eq!(pull_refs::read(git_binary, &parent, 2).await.unwrap(), Some(main));
    assert!(pull_refs::set(git_binary, &parent, 3, "main").await.is_err(), "only full ids");
    assert!(pull_refs::fetch(git_binary, &parent, &parent, "--upload-pack=evil", 4).await.is_err());
    assert!(pull_refs::fetch(git_binary, &parent, &parent, "missing", 4).await.is_err());
}
