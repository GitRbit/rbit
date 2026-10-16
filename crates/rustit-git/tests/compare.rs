//! Branch comparison: merge base, ahead/behind counts and the diff since the merge base.

use std::{path::Path, process::Command};

use rustit_git::{ChangeStatus, DiffLimits, RepositoryReader};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(dir: &Path, file: &str, contents: &str, message: &str) {
    std::fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// main: base → main-1 → main-2; feature (from base): feature-1 → feature-2;
/// `orphan` shares no history.
fn fixture() -> (tempfile::TempDir, RepositoryReader, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    commit(root, "shared.txt", "shared\n", "base");
    let base = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
    commit(root, "feature.txt", "one\n", "feature-1");
    commit(root, "feature.txt", "one\ntwo\n", "feature-2");
    git(root, &["checkout", "-q", "main"]);
    commit(root, "main.txt", "main\n", "main-1");
    commit(root, "shared.txt", "changed on main\n", "main-2");
    git(root, &["checkout", "-q", "--orphan", "orphan"]);
    commit(root, "other.txt", "unrelated\n", "orphan");
    git(root, &["checkout", "-q", "main"]);
    let reader = RepositoryReader::new(root.join(".git"));
    (dir, reader, base)
}

#[tokio::test]
async fn compares_from_the_merge_base() {
    let (_dir, reader, base) = fixture();
    let comparison = reader.compare("main".into(), "feature".into(), DiffLimits::default(), 50).await.unwrap();

    assert_eq!(comparison.merge_base.as_deref(), Some(base.as_str()));
    assert_eq!((comparison.ahead, comparison.behind), (2, 2));
    let summaries: Vec<_> = comparison.commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["feature-2", "feature-1"]);
    // Only the feature's change: main's edit of shared.txt is not shown as a reversion.
    assert_eq!(comparison.files.len(), 1, "{:#?}", comparison.files);
    assert_eq!((comparison.files[0].path.as_str(), comparison.files[0].status), ("feature.txt", ChangeStatus::Added));
    assert_eq!((comparison.additions(), comparison.deletions()), (2, 0));
    assert_eq!(comparison.head.summary, "feature-2");
}

#[tokio::test]
async fn identical_and_unrelated_revisions() {
    let (_dir, reader, _) = fixture();
    let same = reader.compare("main".into(), "main".into(), DiffLimits::default(), 50).await.unwrap();
    assert_eq!((same.ahead, same.behind, same.files.len()), (0, 0, 0));

    let unrelated = reader.compare("main".into(), "orphan".into(), DiffLimits::default(), 50).await.unwrap();
    assert_eq!(unrelated.merge_base, None);
    assert_eq!((unrelated.ahead, unrelated.behind), (1, 3));
    assert!(unrelated.files.is_empty());

    let limited = reader.compare("orphan".into(), "main".into(), DiffLimits::default(), 1).await.unwrap();
    assert_eq!((limited.ahead, limited.commits.len()), (3, 1));
    assert!(reader.compare("main".into(), "nope".into(), DiffLimits::default(), 50).await.is_err());
}
