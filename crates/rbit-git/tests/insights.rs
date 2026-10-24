//! Language statistics and last-commit-per-entry against history created with the `git` CLI.

use std::{cell::Cell, path::Path, process::Command, time::Duration};

use rbit_git::{LastCommitLimits, RepositoryReader};

/// Runs `git` with deterministic identity and a commit date that advances one minute per
/// call, so newest-first ordering never depends on two commits sharing a second.
struct Git<'a> {
    dir: &'a Path,
    clock: Cell<i64>,
}

impl<'a> Git<'a> {
    fn new(dir: &'a Path) -> Self {
        Self { dir, clock: Cell::new(1_700_000_000) }
    }

    fn run(&self, args: &[&str]) {
        self.clock.set(self.clock.get() + 60);
        let date = format!("{} +0000", self.clock.get());
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "init.defaultBranch=main",
                "-c",
                "core.autocrlf=false",
            ])
            .args(args)
            .current_dir(self.dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .status()
            .expect("git is installed");
        assert!(status.success(), "git {args:?} failed");
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn commit_all(&self, message: &str) {
        self.run(&["add", "-A"]);
        self.run(&["commit", "-q", "-m", message]);
    }
}

/// main: initial → edit main → (side: side lib) merged with --no-ff → add notes.
fn fixture() -> (tempfile::TempDir, RepositoryReader) {
    let dir = tempfile::tempdir().unwrap();
    let git = Git::new(dir.path());
    git.run(&["init", "-q"]);

    git.write("src/main.rs", &"fn main() {}\n".repeat(50)); // 650 bytes
    git.write("src/lib.rs", &"pub fn f() {}\n".repeat(20)); // 280 bytes
    git.write("web/index.html", &"<p>hi</p>\n".repeat(7)); // 70 bytes
    git.write("README.md", &"# readme\n".repeat(100));
    git.write("Cargo.lock", &"# generated\n".repeat(100));
    git.write("config.yaml", &"key: value\n".repeat(100));
    git.write("vendor/dep/lib.go", &"package dep\n".repeat(100));
    git.write("docs/conf.py", &"x = 1\n".repeat(100));
    git.write("lib/helpers.inc", "<?php echo 1; ?>\n");
    git.commit_all("initial");

    git.write("src/main.rs", &"fn main() { }\n".repeat(50)); // 700 bytes
    git.commit_all("edit main");

    git.run(&["checkout", "-q", "-b", "side", "HEAD~1"]);
    git.write("src/lib.rs", &"pub fn g() {}\n".repeat(20));
    git.commit_all("side lib");
    git.run(&["checkout", "-q", "main"]);
    git.run(&["merge", "-q", "--no-ff", "-m", "merge side", "side"]);

    git.write("notes.txt", "notes\n");
    git.commit_all("add notes");

    git.run(&["checkout", "-q", "-b", "attributes"]);
    git.write(".gitattributes", "vendor/** -linguist-vendored\n*.inc linguist-language=PHP\nweb/** linguist-detectable=false\n");
    git.commit_all("attributes");
    git.run(&["checkout", "-q", "main"]);

    let reader = RepositoryReader::new(dir.path().join(".git"));
    (dir, reader)
}

fn names(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|name| (*name).to_owned()).collect()
}

#[tokio::test]
async fn language_file_lists_match_attributes_and_revision() {
    let (_dir, reader) = fixture();
    assert_eq!(reader.language_files("main".into(), "Rust".into()).await.unwrap(), names(&["src/lib.rs", "src/main.rs"]));
    assert_eq!(reader.language_files("main".into(), "HTML".into()).await.unwrap(), names(&["web/index.html"]));
    assert_eq!(reader.language_files("attributes".into(), "HTML".into()).await.unwrap(), Vec::<String>::new());
    assert_eq!(reader.language_files("attributes".into(), "PHP".into()).await.unwrap(), names(&["lib/helpers.inc"]));
    assert_eq!(reader.language_files("main".into(), "Go".into()).await.unwrap(), Vec::<String>::new());
    assert_eq!(reader.language_files("attributes".into(), "Go".into()).await.unwrap(), names(&["vendor/dep/lib.go"]));
    assert_eq!(reader.language_files("main".into(), "Unknown".into()).await.unwrap(), Vec::<String>::new());
    assert!(reader.language_files("missing".into(), "Rust".into()).await.is_err());
}

#[tokio::test]
async fn branch_history_visits_shared_ancestors_once_and_keeps_merge_parents() {
    let (_dir, reader) = fixture();
    let commits = reader.branch_history(names(&["refs/heads/main", "refs/heads/side", "refs/heads/attributes"]), 200).await.unwrap();
    let unique: std::collections::HashSet<_> = commits.iter().map(|commit| &commit.id).collect();
    assert_eq!(unique.len(), commits.len());
    assert_eq!(commits.len(), 6);
    assert_eq!(commits.iter().find(|commit| commit.summary == "merge side").unwrap().parent_ids.len(), 2);
    assert_eq!(reader.branch_history(names(&["main", "side"]), 2).await.unwrap().len(), 2);
    assert!(reader.branch_history(Vec::new(), 200).await.unwrap().is_empty());
}

#[tokio::test]
async fn language_stats_count_programming_and_markup_only() {
    let (_dir, reader) = fixture();
    let stats = reader.language_stats("main".into()).await.unwrap();

    let summary: Vec<_> = stats.languages.iter().map(|l| (l.name.as_str(), l.bytes)).collect();
    assert_eq!(summary, [("Rust", 980), ("HTML", 70)]);
    assert_eq!(stats.total_bytes, 1050);
    assert!((stats.languages[0].percent - 93.333).abs() < 0.01);
    assert_eq!(stats.languages[0].color, "#dea584");
}

#[tokio::test]
async fn language_stats_honour_gitattributes() {
    let (_dir, reader) = fixture();
    let stats = reader.language_stats("attributes".into()).await.unwrap();

    let names: Vec<_> = stats.languages.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Go", "Rust", "PHP"], "vendor/ re-included, .inc counted as PHP, web/ excluded");
}

#[tokio::test]
async fn last_commits_at_the_root() {
    let (_dir, reader) = fixture();
    let entries = names(&["src", "web", "README.md", "notes.txt", "missing"]);
    let result = reader.last_commits("main".into(), String::new(), entries, LastCommitLimits::default()).await.unwrap();
    assert!(!result.timed_out);

    let summary = |name: &str| result.commits.get(name).map(|c| c.summary.as_str());
    // The merge is not identical to either parent inside src/, so it is what last touched it.
    assert_eq!(summary("src"), Some("merge side"));
    assert_eq!(summary("web"), Some("initial"));
    assert_eq!(summary("README.md"), Some("initial"));
    assert_eq!(summary("notes.txt"), Some("add notes"));
    assert_eq!(summary("missing"), None);
}

#[tokio::test]
async fn last_commits_follow_merged_side_branches() {
    let (_dir, reader) = fixture();
    let entries = names(&["main.rs", "lib.rs"]);
    let result = reader.last_commits("main".into(), "src".into(), entries, LastCommitLimits::default()).await.unwrap();

    assert_eq!(result.commits["main.rs"].summary, "edit main");
    assert_eq!(result.commits["lib.rs"].summary, "side lib", "credited to the commit that changed it, not the merge");
}

#[tokio::test]
async fn last_commits_respect_the_commit_bound() {
    let (_dir, reader) = fixture();
    let limits = LastCommitLimits { max_commits: 1, deadline: Duration::from_secs(60) };
    let result = reader.last_commits("main".into(), String::new(), names(&["notes.txt", "README.md"]), limits).await.unwrap();

    assert!(!result.timed_out);
    assert!(result.commits.contains_key("notes.txt"));
    assert!(!result.commits.contains_key("README.md"), "beyond the bound");
}

#[tokio::test]
async fn last_commits_report_a_missed_deadline() {
    let (_dir, reader) = fixture();
    let limits = LastCommitLimits { max_commits: 5_000, deadline: Duration::ZERO };
    let result = reader.last_commits("main".into(), String::new(), names(&["README.md"]), limits).await.unwrap();

    assert!(result.timed_out);
    assert!(result.commits.is_empty());
}

#[tokio::test]
async fn commit_id_resolves_names() {
    let (_dir, reader) = fixture();
    let id = reader.commit_id("main".into()).await.unwrap();
    assert_eq!(id.len(), 40);
    assert_eq!(reader.commit_id(id[..7].to_owned()).await.unwrap(), id);
}

#[tokio::test]
async fn commit_id_takes_full_ids_as_objects() {
    let (dir, reader) = fixture();
    let id = reader.commit_id("main".into()).await.unwrap();
    assert_eq!(reader.commit_id(id.clone()).await.unwrap(), id);
    assert_eq!(reader.commit_id(id.to_uppercase()).await.unwrap(), id, "hex in either case");
    // A full id names its object even when a branch has that name, as in Git.
    let git = Git::new(dir.path());
    git.run(&["tag", "older", "main~1"]);
    let older = reader.commit_id("older".into()).await.unwrap();
    assert_ne!(older, id);
    git.run(&["branch", &id, "older"]);
    assert_eq!(reader.commit_id(id.clone()).await.unwrap(), id);
    // A name that only looks like an id is a ref; one that is neither is not found.
    let hex_branch = "deadbeef".repeat(5);
    git.run(&["branch", &hex_branch, "older"]);
    assert_eq!(reader.commit_id(hex_branch).await.unwrap(), older);
    assert!(reader.commit_id("0".repeat(40)).await.is_err());
}

#[tokio::test]
async fn ref_pages_preserve_recent_order_and_default_branch_priority() {
    let (dir, reader) = fixture();
    let git = Git::new(dir.path());
    git.run(&["tag", "v-old", "main~2"]);
    git.run(&["tag", "v-new", "main"]);
    git.run(&["tag", "-a", "v-middle", "main~1", "-m", "annotated"]);
    let all = reader.ref_details(rbit_git::RefKind::Tag).await.unwrap();
    let (first, total) = reader.ref_details_page(rbit_git::RefKind::Tag, String::new(), "main".into(), 1, 2).await.unwrap();
    let (last, _) = reader.ref_details_page(rbit_git::RefKind::Tag, String::new(), "main".into(), usize::MAX, 2).await.unwrap();
    assert_eq!(total, 3);
    assert_eq!(first.iter().chain(&last).map(|r| &r.name).collect::<Vec<_>>(), all.iter().map(|r| &r.name).collect::<Vec<_>>());
    let (found, total) = reader.ref_details_page(rbit_git::RefKind::Tag, "V-OLD".into(), "main".into(), 1, 100).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(found[0].name, "v-old");
    let (branches, _) = reader.ref_details_page(rbit_git::RefKind::Branch, String::new(), "main".into(), 1, 1).await.unwrap();
    assert_eq!(branches[0].name, "main", "default precedes newer attributes branch");
}

#[tokio::test]
async fn language_paths_and_multi_branch_history_match_the_selected_snapshot() {
    let (_dir, reader) = fixture();
    assert_eq!(reader.language_files("main".into(), "Rust".into()).await.unwrap(), ["src/lib.rs", "src/main.rs"]);
    assert_eq!(reader.language_files("attributes".into(), "PHP".into()).await.unwrap(), ["lib/helpers.inc"]);
    assert_eq!(reader.language_files("attributes".into(), "HTML".into()).await.unwrap(), Vec::<String>::new());
    let history = reader.branch_history(vec!["main".into(), "side".into()], 3).await.unwrap();
    assert_eq!(history.len(), 3);
    let ids: std::collections::HashSet<_> = history.iter().map(|c| &c.id).collect();
    assert_eq!(ids.len(), history.len());
}
