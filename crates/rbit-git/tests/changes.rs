//! Commit details and ref listings against history created with the `git` CLI.

use std::{path::Path, process::Command};

use rbit_git::{ChangeStatus, DiffLimits, FileDiffBody, RefKind, RepositoryReader};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git is installed");
    assert!(status.success(), "git {args:?} failed");
}

/// main: add a.txt + b.txt → edit a.txt, rename b.txt → c.txt; tag v1 on the first commit;
/// branch `feature` with one extra commit.
fn fixture() -> (tempfile::TempDir, RepositoryReader) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(root.join("b.txt"), "unchanged content that is long enough to be detected as a rename\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "first"]);
    git(root, &["tag", "-a", "v1", "-m", "release"]);

    std::fs::write(root.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
    git(root, &["mv", "b.txt", "c.txt"]);
    git(root, &["commit", "-q", "-am", "second"]);

    git(root, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(root.join("d.txt"), "new\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "feature work"]);
    git(root, &["checkout", "-q", "main"]);

    let reader = RepositoryReader::new(root.join(".git"));
    (dir, reader)
}

#[tokio::test]
async fn commit_detail_reports_edits_and_renames() {
    let (_dir, reader) = fixture();
    let detail = reader.commit_detail("main".into(), DiffLimits::default()).await.unwrap();

    assert_eq!(detail.commit.summary, "second");
    assert_eq!(detail.files.len(), 2, "{:#?}", detail.files);

    let edited = detail.files.iter().find(|f| f.path == "a.txt").unwrap();
    assert_eq!(edited.status, ChangeStatus::Modified);
    assert_eq!((edited.additions, edited.deletions), (2, 1));
    assert!(matches!(edited.body, FileDiffBody::Text(ref hunks) if hunks.len() == 1));

    let renamed = detail.files.iter().find(|f| f.path == "c.txt").unwrap();
    assert_eq!(renamed.status, ChangeStatus::Renamed);
    assert_eq!(renamed.old_path.as_deref(), Some("b.txt"));
    assert!(matches!(renamed.body, FileDiffBody::ModeOnly));
}

#[tokio::test]
async fn root_commit_is_all_additions() {
    let (_dir, reader) = fixture();
    let detail = reader.commit_detail("v1".into(), DiffLimits::default()).await.unwrap();
    assert!(detail.files.iter().all(|f| f.status == ChangeStatus::Added));
    assert_eq!(detail.additions(), 4);
}

#[tokio::test]
async fn ref_details_list_branches_and_peel_annotated_tags() {
    let (_dir, reader) = fixture();

    let branches = reader.ref_details(RefKind::Branch).await.unwrap();
    let names: Vec<_> = branches.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"main") && names.contains(&"feature"), "{names:?}");

    let tags = reader.ref_details(RefKind::Tag).await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].commit.summary, "first");
}

#[tokio::test]
async fn line_budget_caps_rendered_lines_and_flags_truncation() {
    let (_dir, reader) = fixture();
    let limits = DiffLimits { max_total_lines: 2, ..DiffLimits::default() };
    let detail = reader.commit_detail("v1".into(), limits).await.unwrap();

    assert!(detail.truncated);
    let rendered: usize = detail
        .files
        .iter()
        .map(|f| match &f.body {
            FileDiffBody::Text(hunks) => hunks.iter().map(|h| h.lines.len()).sum(),
            _ => 0,
        })
        .sum();
    assert!(rendered <= 2, "rendered {rendered} lines");

    // The over-budget file keeps honest counts; files after it are marked unknown.
    let first = &detail.files[0];
    assert!(first.stats_known && first.additions > 0);
    assert!(detail.files[1..].iter().all(|f| !f.stats_known));
}

#[tokio::test]
async fn zero_and_exact_budgets_preserve_known_statistics() {
    let (_dir, reader) = fixture();
    // The root commit adds a.txt (three lines) and b.txt (one line), in that order.
    for (budget, known, truncated) in
        [(0, [false, false], true), (1, [true, false], true), (3, [true, false], true), (4, [true, true], false)]
    {
        let limits = DiffLimits { max_total_lines: budget, ..DiffLimits::default() };
        let detail = reader.commit_detail("v1".into(), limits).await.unwrap();
        assert_eq!(detail.truncated, truncated, "line budget {budget}");
        assert_eq!(detail.files.iter().map(|file| file.stats_known).collect::<Vec<_>>(), known);
        for (file, count) in detail.files.iter().zip([3, 1]) {
            assert_eq!(file.additions, if file.stats_known { count } else { 0 });
            assert_eq!(file.deletions, 0);
        }
        if budget == 1 {
            assert!(matches!(detail.files[0].body, FileDiffBody::TooLarge));
        } else if budget >= 3 {
            assert!(matches!(detail.files[0].body, FileDiffBody::Text(ref hunks) if hunks[0].lines.len() == 3));
        }
    }
    for (max_files, expected, truncated) in [(0, 0, true), (1, 1, true), (2, 2, false), (3, 2, false)] {
        let limits = DiffLimits { max_files, ..DiffLimits::default() };
        let detail = reader.commit_detail("v1".into(), limits).await.unwrap();
        assert_eq!(detail.files.len(), expected);
        assert_eq!(detail.truncated, truncated, "file budget {max_files}");
        assert!(detail.files.iter().all(|file| file.stats_known && file.status == ChangeStatus::Added));
    }
}
