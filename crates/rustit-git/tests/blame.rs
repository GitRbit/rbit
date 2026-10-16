//! Blame on a fixture with known authorship.

use std::{path::Path, process::Command, time::Duration};

use rustit_git::{
    GitError, RepositoryReader,
    blame::{Blame, BlameLimits},
};

fn git_as(dir: &Path, author: &str, args: &[&str]) {
    let name = format!("user.name={author}");
    let email = format!("user.email={}@example.com", author.to_lowercase());
    let status = Command::new("git")
        .args(["-c", &name, "-c", &email, "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git is installed");
    assert!(status.success(), "git {args:?} failed");
}

/// Ada writes three lines, Bob rewrites the middle one, then Ada renames the file.
fn fixture() -> (tempfile::TempDir, RepositoryReader) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git_as(root, "Ada", &["init", "-q"]);
    std::fs::write(root.join("poem.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(root.join("blob.bin"), b"\0\x01binary").unwrap();
    git_as(root, "Ada", &["add", "."]);
    git_as(root, "Ada", &["commit", "-q", "-m", "Write the poem"]);
    std::fs::write(root.join("poem.txt"), "one\nTWO\nthree\n").unwrap();
    git_as(root, "Bob", &["commit", "-q", "-am", "Shout the middle"]);
    git_as(root, "Ada", &["mv", "poem.txt", "verse.txt"]);
    git_as(root, "Ada", &["commit", "-q", "-m", "Rename the poem"]);
    let reader = RepositoryReader::new(root.join(".git"));
    (dir, reader)
}

#[tokio::test]
async fn hunks_name_the_commit_that_introduced_each_line() {
    let (_dir, reader) = fixture();
    let Blame::Blamed(blame) = reader.blame("main".into(), "verse.txt".into(), BlameLimits::default()).await.unwrap() else {
        panic!("expected a blame");
    };
    assert_eq!(blame.text, "one\nTWO\nthree\n");
    let summary: Vec<_> = blame.hunks.iter().map(|h| (h.start_line, h.line_count, h.commit.author_name.as_str())).collect();
    assert_eq!(summary, [(1, 1, "Ada"), (2, 1, "Bob"), (3, 1, "Ada")]);
    assert_eq!(blame.hunks[1].commit.summary, "Shout the middle");
    assert_eq!(blame.hunks[0].commit.summary, "Write the poem", "renames are followed");
    assert_eq!(blame.hunks[0].source_path.as_deref(), Some("poem.txt"));
    assert_eq!(blame.commit.summary, "Rename the poem");
}

#[tokio::test]
async fn binary_large_and_missing_files_are_not_blamed() {
    let (_dir, reader) = fixture();
    assert!(matches!(reader.blame("main".into(), "blob.bin".into(), BlameLimits::default()).await.unwrap(), Blame::Binary));
    let tiny = BlameLimits { max_file_bytes: 4, time_budget: Duration::from_secs(5) };
    assert!(matches!(reader.blame("main".into(), "verse.txt".into(), tiny).await.unwrap(), Blame::TooLarge));
    assert!(matches!(reader.blame("main".into(), "nope.txt".into(), BlameLimits::default()).await, Err(GitError::NotFound(_))));
}
