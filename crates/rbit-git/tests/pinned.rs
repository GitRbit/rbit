//! A pinned reader shares one opened repository between calls, yet still sees objects,
//! packs and references written after it was opened (as during a push or a `git gc`).

use std::{
    path::Path,
    process::{Command, Stdio},
};

use rbit_git::RepositoryReader;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(Stdio::null())
        .status()
        .expect("git is installed");
    assert!(status.success(), "git {args:?} failed");
}

fn commit(dir: &Path, file: &str, message: &str) {
    std::fs::write(dir.join(file), message).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
}

#[tokio::test]
async fn pinned_reader_sees_history_written_after_it_was_opened() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    commit(root, "a.txt", "first");

    let reader = RepositoryReader::new(root.join(".git")).pinned();
    let clone = reader.clone();
    assert_eq!(reader.history("main".into(), 0, 10).await.unwrap().len(), 1);

    // New loose objects, then everything repacked into a new pack.
    commit(root, "a.txt", "second");
    git(root, &["checkout", "-q", "-b", "feature"]);
    commit(root, "b.txt", "feature work");
    git(root, &["gc", "-q"]);

    let history = clone.history("feature".into(), 0, 10).await.unwrap();
    let summaries: Vec<&str> = history.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["feature work", "second", "first"]);

    let branches: Vec<String> = reader.refs().await.unwrap().into_iter().map(|r| r.name).collect();
    assert_eq!(branches, ["feature", "main"]);
    let tree = reader.tree("feature".into(), String::new()).await.unwrap();
    assert_eq!(tree.entries.len(), 2);
}

#[tokio::test]
async fn pinned_and_plain_readers_agree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    commit(root, "README.md", "# hello");

    let plain = RepositoryReader::new(root.join(".git"));
    let pinned = plain.pinned();
    let (a, b) = tokio::join!(plain.split_revision_path("main/README.md".into()), pinned.split_revision_path("main/README.md".into()));
    assert_eq!(a.unwrap(), b.unwrap());
    let (a, b) = tokio::join!(plain.commit_id("main".into()), pinned.commit_id("main".into()));
    assert_eq!(a.unwrap(), b.unwrap());
}
