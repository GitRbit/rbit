//! Merging pull requests on a scratch repository: each strategy, conflicts, a moving base,
//! clean-up, hooks and line endings.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use rbit_core::{Timestamp, issue::MergeStrategy, pull::CommitIdentity};
use rbit_git::merge::{MergeEnvironment, MergeError, MergeSpec, TrialMerge, merge, trial};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Ada")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(dir: &Path, file: &str, contents: &[u8], message: &str) -> String {
    std::fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

struct Fixture {
    dir: tempfile::TempDir,
    work: PathBuf,
    bare: PathBuf,
    env: MergeEnvironment,
}

impl Fixture {
    /// `main`: base → main-1 (touches `main.txt`); `feature` from base: two commits by Bob
    /// on `feature.txt`. Pushed to a bare repository.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        git(&work, &["init", "-q"]);
        commit(&work, "shared.txt", b"one\ntwo\nthree\n", "base");
        git(&work, &["checkout", "-q", "-b", "feature"]);
        git(&work, &["-c", "user.name=Bob", "-c", "user.email=bob@example.com", "commit", "-q", "--allow-empty", "-m", "noop"]);
        std::fs::write(work.join("feature.txt"), "feature 1\n").unwrap();
        git(&work, &["add", "."]);
        bob_commit(&work, "feature-1");
        std::fs::write(work.join("feature.txt"), "feature 1\nfeature 2\n").unwrap();
        git(&work, &["add", "."]);
        bob_commit(&work, "feature-2");
        git(&work, &["checkout", "-q", "main"]);
        commit(&work, "main.txt", b"main\n", "main-1");
        let bare = dir.path().join("base.git");
        git(dir.path(), &["clone", "-q", "--bare", work.to_str().unwrap(), bare.to_str().unwrap()]);
        let env = MergeEnvironment { git_binary: "git".into(), tmp_dir: dir.path().join("tmp"), timeout: Duration::from_secs(60) };
        Self { dir, work, bare, env }
    }

    fn bare(&self, args: &[&str]) -> String {
        let mut full = vec!["--git-dir", self.bare.to_str().unwrap()];
        full.extend_from_slice(args);
        git(self.dir.path(), &full)
    }

    fn tip(&self, branch: &str) -> String {
        self.bare(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    fn scratch_is_empty(&self) -> bool {
        let merges = self.env.tmp_dir.join("merges");
        !merges.exists() || std::fs::read_dir(merges).unwrap().next().is_none()
    }
}

fn bob_commit(work: &Path, message: &str) {
    let output = Command::new("git")
        .args(["commit", "-q", "-m", message])
        .current_dir(work)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Bob")
        .env("GIT_AUTHOR_EMAIL", "bob@example.com")
        .env("GIT_COMMITTER_NAME", "Bob")
        .env("GIT_COMMITTER_EMAIL", "bob@example.com")
        .output()
        .unwrap();
    assert!(output.status.success());
}

fn identity(name: &str) -> CommitIdentity {
    CommitIdentity::noreply(name, None, "noreply.example")
}

async fn run(fixture: &Fixture, strategy: MergeStrategy, base_tip: &str, head: &str) -> Result<String, MergeError> {
    let (author, merger) = (identity("bob"), identity("carol"));
    let spec = MergeSpec {
        repo: &fixture.bare,
        base_branch: "main",
        base_tip,
        head,
        strategy,
        message: "Merge pull request #1 from bob/feature\n\nAdd the feature\n",
        author: &author,
        merger: &merger,
        at: Timestamp(1_700_000_000),
    };
    merge(&fixture.env, &spec).await.map(|merged| merged.commit)
}

#[tokio::test]
async fn a_merge_commit_has_both_parents_and_the_message() {
    let fixture = Fixture::new();
    let (base, head) = (fixture.tip("main"), fixture.tip("feature"));
    let commit = run(&fixture, MergeStrategy::Merge, &base, &head).await.unwrap();

    assert_eq!(fixture.tip("main"), commit);
    assert_eq!(fixture.bare(&["log", "-1", "--format=%P", &commit]), format!("{base} {head}"));
    assert_eq!(fixture.bare(&["log", "-1", "--format=%B", &commit]), "Merge pull request #1 from bob/feature\n\nAdd the feature");
    assert_eq!(
        fixture.bare(&["log", "-1", "--format=%an <%ae>|%cn <%ce>|%ct", &commit]),
        "carol <carol@noreply.example>|carol <carol@noreply.example>|1700000000"
    );
    assert_eq!(fixture.bare(&["show", &format!("{commit}:feature.txt")]), "feature 1\nfeature 2");
    assert_eq!(fixture.bare(&["show", &format!("{commit}:main.txt")]), "main");
    assert!(fixture.scratch_is_empty(), "the scratch repository is removed after a merge");
}

#[tokio::test]
async fn a_squash_commit_has_one_parent_and_the_heads_tree() {
    let fixture = Fixture::new();
    // Rewind main to the fork point, so the squash's tree is exactly the head's.
    let fork_point = fixture.bare(&["merge-base", "main", "feature"]);
    fixture.bare(&["update-ref", "refs/heads/main", &fork_point]);
    let head = fixture.tip("feature");
    let commit = run(&fixture, MergeStrategy::Squash, &fork_point, &head).await.unwrap();

    assert_eq!(fixture.bare(&["log", "-1", "--format=%P", &commit]), fork_point);
    assert_eq!(fixture.bare(&["rev-parse", &format!("{commit}^{{tree}}")]), fixture.bare(&["rev-parse", &format!("{head}^{{tree}}")]));
    assert_eq!(fixture.bare(&["log", "-1", "--format=%an <%ae>|%cn", &commit]), "bob <bob@noreply.example>|carol");
}

#[tokio::test]
async fn a_rebase_is_linear_keeps_authors_and_sets_the_committer() {
    let fixture = Fixture::new();
    let (base, head) = (fixture.tip("main"), fixture.tip("feature"));
    let tip = run(&fixture, MergeStrategy::Rebase, &base, &head).await.unwrap();

    // The empty "noop" commit is dropped; the two real ones are replayed in order.
    let log = fixture.bare(&["log", "--format=%s|%an <%ae>|%cn <%ce>|%P", &format!("{base}..{tip}")]);
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(lines[0].starts_with("feature-2|Bob <bob@example.com>|carol <carol@noreply.example>|"), "{log}");
    assert!(lines[1].starts_with("feature-1|Bob <bob@example.com>|carol <carol@noreply.example>|"), "{log}");
    assert!(lines[1].ends_with(&base), "the first replayed commit sits on the base tip");
    assert_eq!(fixture.bare(&["rev-list", "--merges", "--count", &tip]), "0");
    assert_eq!(fixture.bare(&["show", &format!("{tip}:main.txt")]), "main");
}

#[tokio::test]
async fn conflicts_list_their_paths_and_leave_the_base_untouched() {
    let fixture = Fixture::new();
    git(&fixture.work, &["checkout", "-q", "-b", "clash", "main~1"]);
    commit(&fixture.work, "shared.txt", b"one\nTWO from clash\nthree\n", "clash");
    commit(&fixture.work, "other.txt", b"clash\n", "other");
    git(&fixture.work, &["checkout", "-q", "main"]);
    commit(&fixture.work, "shared.txt", b"one\nTWO from main\nthree\n", "main edit");
    commit(&fixture.work, "other.txt", b"main\n", "main other");
    git(&fixture.work, &["push", "-q", fixture.bare.to_str().unwrap(), "main", "clash"]);
    let (base, head) = (fixture.tip("main"), fixture.tip("clash"));

    for strategy in [MergeStrategy::Merge, MergeStrategy::Squash, MergeStrategy::Rebase] {
        let error = run(&fixture, strategy, &base, &head).await.unwrap_err();
        let MergeError::Conflict(paths) = error else { panic!("{strategy}: expected a conflict, got {error:?}") };
        if strategy == MergeStrategy::Rebase {
            assert_eq!(paths, ["shared.txt"], "the first replayed commit conflicts");
        } else {
            assert_eq!(paths, ["other.txt", "shared.txt"]);
        }
        assert_eq!(fixture.tip("main"), base);
    }
    let trial_result = trial(&fixture.env, &fixture.bare, &base, &head).await.unwrap();
    assert_eq!(trial_result, TrialMerge::Conflict(vec!["other.txt".to_owned(), "shared.txt".to_owned()]));
    let clean = trial(&fixture.env, &fixture.bare, &base, &fixture.tip("feature")).await.unwrap();
    assert_eq!(clean, TrialMerge::Clean);
    assert!(fixture.scratch_is_empty());
}

#[tokio::test]
async fn a_moved_base_is_refused_and_nothing_changes() {
    let fixture = Fixture::new();
    let stale = fixture.bare(&["rev-parse", "main~1"]);
    let current = fixture.tip("main");
    let head = fixture.tip("feature");
    for strategy in [MergeStrategy::Merge, MergeStrategy::Squash, MergeStrategy::Rebase] {
        let error = run(&fixture, strategy, &stale, &head).await.unwrap_err();
        assert!(matches!(error, MergeError::BaseMoved), "{strategy}: {error:?}");
        assert_eq!(fixture.tip("main"), current);
    }
    assert!(fixture.scratch_is_empty());
}

#[tokio::test]
async fn nothing_to_merge_unrelated_histories_and_timeouts() {
    let mut fixture = Fixture::new();
    let (base, head) = (fixture.tip("main"), fixture.tip("feature"));
    let ancestor = fixture.bare(&["rev-parse", "main~1"]);
    assert!(matches!(run(&fixture, MergeStrategy::Merge, &base, &ancestor).await, Err(MergeError::NothingToMerge)));

    git(&fixture.work, &["checkout", "-q", "--orphan", "orphan"]);
    commit(&fixture.work, "orphan.txt", b"x\n", "orphan");
    git(&fixture.work, &["push", "-q", fixture.bare.to_str().unwrap(), "orphan"]);
    let orphan = fixture.tip("orphan");
    assert!(matches!(run(&fixture, MergeStrategy::Merge, &base, &orphan).await, Err(MergeError::Unrelated)));
    assert!(matches!(trial(&fixture.env, &fixture.bare, &base, &orphan).await, Err(MergeError::Unrelated)));

    assert!(run(&fixture, MergeStrategy::Merge, "not-an-id", &head).await.is_err());

    fixture.env.timeout = Duration::from_nanos(1);
    assert!(matches!(run(&fixture, MergeStrategy::Merge, &base, &head).await, Err(MergeError::TimedOut(_))));
    // The timed-out merge's scratch directory went with its future.
    assert!(fixture.scratch_is_empty());
    assert_eq!(fixture.tip("main"), base);
}

#[tokio::test]
async fn repository_hooks_never_run() {
    let fixture = Fixture::new();
    let marker = fixture.dir.path().join("hook-ran");
    let hook = fixture.bare.join("hooks").join("reference-transaction");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    let marker_path = marker.to_string_lossy().replace('\\', "/");
    std::fs::write(&hook, format!("#!/bin/sh\necho ran >> '{marker_path}'\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // The planted hook works: a plain ref update in the repository runs it.
    fixture.bare(&["update-ref", "refs/heads/probe", &fixture.tip("main")]);
    assert!(marker.exists(), "the fixture's hook is live");
    std::fs::remove_file(&marker).unwrap();

    let (base, head) = (fixture.tip("main"), fixture.tip("feature"));
    run(&fixture, MergeStrategy::Merge, &base, &head).await.unwrap();
    assert!(!marker.exists(), "merging ran a repository hook");
}

#[tokio::test]
async fn line_endings_are_kept_byte_for_byte() {
    let fixture = Fixture::new();
    git(&fixture.work, &["checkout", "-q", "-b", "crlf", "main"]);
    commit(&fixture.work, "windows.txt", b"one\r\ntwo\r\n", "crlf file");
    // Attributes asking for conversion apply to checkouts only; merges never check out.
    // Only the attributes file is staged: `git add .` could re-hash (and normalise) the
    // CRLF file under the new attributes when its index entry looks racily clean.
    std::fs::write(fixture.work.join(".gitattributes"), "*.txt text eol=lf\n").unwrap();
    git(&fixture.work, &["add", ".gitattributes"]);
    git(&fixture.work, &["commit", "-q", "-m", "attributes"]);
    // Forced: under the new attributes the checked-out CRLF file looks modified.
    git(&fixture.work, &["checkout", "-q", "-f", "main"]);
    git(&fixture.work, &["push", "-q", fixture.bare.to_str().unwrap(), "crlf"]);
    // main moves on, so this is a real (non fast-forward) merge.
    commit(&fixture.work, "later.txt", b"later\n", "later");
    git(&fixture.work, &["push", "-q", fixture.bare.to_str().unwrap(), "main"]);

    let (base, head) = (fixture.tip("main"), fixture.tip("crlf"));
    let commit = run(&fixture, MergeStrategy::Squash, &base, &head).await.unwrap();
    let blob = fixture.bare(&["rev-parse", &format!("{commit}:windows.txt")]);
    let output = Command::new("git").args(["--git-dir", fixture.bare.to_str().unwrap(), "cat-file", "blob", &blob]).output().unwrap();
    assert_eq!(output.stdout, b"one\r\ntwo\r\n");
}
