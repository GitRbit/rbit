//! Ref snapshots and new-commit listings against history created with the `git` CLI.

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
async fn snapshots_report_what_changed_and_which_commits_are_new() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    let reader = RepositoryReader::new(root.join(".git"));
    assert!(reader.ref_snapshot().await.unwrap().is_empty(), "a fresh repository has no refs");

    commit(root, "a.txt", "first");
    git(root, &["tag", "-a", "v1", "-m", "release"]);
    let before = reader.ref_snapshot().await.unwrap();
    assert_eq!(before.len(), 2);
    assert!(before.get("refs/heads/main").is_some() && before.get("refs/tags/v1").is_some());

    commit(root, "a.txt", "second");
    commit(root, "a.txt", "third");
    git(root, &["checkout", "-q", "-b", "feature"]);
    commit(root, "b.txt", "feature work");
    git(root, &["tag", "-d", "v1"]);
    let after = reader.ref_snapshot().await.unwrap();

    let updates = before.updates_to(&after);
    let names: Vec<&str> = updates.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, ["refs/heads/feature", "refs/heads/main", "refs/tags/v1"]);
    assert!(updates[0].is_created() && updates[2].is_deleted());

    let main = &updates[1];
    let introduced = reader.new_commits(main.after.clone().unwrap(), vec![main.before.clone().unwrap()], 10).await.unwrap();
    let summaries: Vec<&str> = introduced.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["third", "second"]);

    // A new branch lists only work no ref had before the push… plus main's new commits,
    // because they were not known before either.
    let feature = &updates[0];
    let known: Vec<String> = before.ids().map(str::to_owned).collect();
    let introduced = reader.new_commits(feature.after.clone().unwrap(), known, 10).await.unwrap();
    assert_eq!(introduced.len(), 3);
    assert_eq!(introduced[0].summary, "feature work");

    let limited = reader.new_commits(feature.after.clone().unwrap(), Vec::new(), 2).await.unwrap();
    assert_eq!(limited.len(), 2, "the limit caps the walk");
}
