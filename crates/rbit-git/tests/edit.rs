//! Committing web edits on a scratch repository: nested paths, renames, deletions that
//! empty a directory, preserved modes and bytes, a moving branch, new branches, limits and
//! clean-up.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use rbit_core::{Timestamp, file_edit::FilePath, pull::CommitIdentity};
use rbit_git::{
    edit::{BlobId, Change, EditError, EditSpec, EditWorkspace},
    merge::MergeEnvironment,
};

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

fn path(path: &str) -> FilePath {
    FilePath::parse(path).unwrap()
}

struct Fixture {
    dir: tempfile::TempDir,
    bare: PathBuf,
    env: MergeEnvironment,
    author: CommitIdentity,
}

impl Fixture {
    /// `main` with `README.md`, an executable `bin/run.sh`, `docs/only.md` (alone in its
    /// directory), a CRLF `win.txt` and a `.gitattributes` that would convert line endings
    /// if anything applied it.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join("bin")).unwrap();
        std::fs::create_dir_all(work.join("docs")).unwrap();
        git(&work, &["init", "-q"]);
        std::fs::write(work.join("README.md"), "# Demo\n").unwrap();
        std::fs::write(work.join("bin/run.sh"), "#!/bin/sh\necho hi\n").unwrap();
        std::fs::write(work.join("docs/only.md"), "alone\n").unwrap();
        std::fs::write(work.join("win.txt"), "one\r\ntwo\r\n").unwrap();
        std::fs::write(work.join(".gitattributes"), "* text=auto eol=lf\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["update-index", "--chmod=+x", "bin/run.sh"]);
        git(&work, &["commit", "-q", "-m", "initial"]);
        let bare = dir.path().join("repo.git");
        git(dir.path(), &["clone", "-q", "--bare", work.to_str().unwrap(), bare.to_str().unwrap()]);
        let env = MergeEnvironment { git_binary: "git".into(), tmp_dir: dir.path().join("tmp"), timeout: Duration::from_secs(60) };
        Self { dir, bare, env, author: CommitIdentity { name: "Ada Lovelace".into(), email: "ada@example.com".into() } }
    }

    fn bare(&self, args: &[&str]) -> String {
        let mut full = vec!["--git-dir", self.bare.to_str().unwrap()];
        full.extend_from_slice(args);
        git(self.dir.path(), &full)
    }

    fn tip(&self, branch: &str) -> String {
        self.bare(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    /// `git ls-tree -r` of `revision`: `mode path` per file.
    fn files(&self, revision: &str) -> Vec<String> {
        let listing = self.bare(&["ls-tree", "-r", "--format=%(objectmode) %(path)", revision]);
        listing.lines().map(str::to_owned).collect()
    }

    fn bytes(&self, revision: &str, file: &str) -> Vec<u8> {
        let output = Command::new("git")
            .args(["--git-dir", self.bare.to_str().unwrap(), "cat-file", "blob", &format!("{revision}:{file}")])
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    }

    fn scratch_is_empty(&self) -> bool {
        let merges = self.env.tmp_dir.join("merges");
        !merges.exists() || std::fs::read_dir(merges).unwrap().next().is_none()
    }

    async fn workspace(&self) -> EditWorkspace {
        EditWorkspace::open(&self.env, &self.bare).await.unwrap()
    }

    /// Commits `changes` (made by `build` in a fresh workspace) on `main` from `parent`.
    async fn commit_with(
        &self,
        parent: &str,
        new_branch: Option<&str>,
        build: impl AsyncFnOnce(&EditWorkspace) -> Vec<Change>,
    ) -> Result<String, EditError> {
        let workspace = self.workspace().await;
        let changes = build(&workspace).await;
        let spec = EditSpec {
            branch: "main",
            parent,
            new_branch,
            changes: &changes,
            message: "Edit in the browser\n\nFixes #1\n",
            author: &self.author,
            at: Timestamp(1_700_000_000),
        };
        let committed = workspace.commit(&spec).await?;
        assert_eq!(committed.branch, new_branch.unwrap_or("main"));
        Ok(committed.commit)
    }
}

async fn blob(workspace: &EditWorkspace, content: &[u8]) -> BlobId {
    workspace.write_blob(content, 1024).await.unwrap()
}

#[tokio::test]
async fn new_files_in_new_directories_land_as_one_commit() {
    let fixture = Fixture::new();
    let parent = fixture.tip("main");
    let commit = fixture
        .commit_with(&parent, None, async |workspace| {
            vec![
                Change::Create { path: path("src/deep/nested/lib.rs"), blob: blob(workspace, b"pub fn f() {}").await },
                Change::Create { path: path("notes.txt"), blob: blob(workspace, b"line\r\nline").await },
            ]
        })
        .await
        .unwrap();

    assert_eq!(fixture.tip("main"), commit);
    assert_eq!(fixture.bare(&["rev-parse", &format!("{commit}^")]), parent, "one commit on top of the parent");
    assert_eq!(fixture.bare(&["rev-list", "--count", &format!("{parent}..{commit}")]), "1");
    assert_eq!(fixture.bytes(&commit, "src/deep/nested/lib.rs"), b"pub fn f() {}", "no newline is added");
    assert_eq!(fixture.bytes(&commit, "notes.txt"), b"line\r\nline", "bytes are stored exactly, attributes notwithstanding");
    let header = fixture.bare(&["cat-file", "commit", &commit]);
    assert!(header.contains("author Ada Lovelace <ada@example.com> 1700000000 +0000"), "{header}");
    assert!(header.contains("committer Ada Lovelace <ada@example.com> 1700000000 +0000"), "{header}");
    assert!(header.ends_with("Edit in the browser\n\nFixes #1"), "{header}");
    assert!(fixture.scratch_is_empty(), "the workspace is removed with the workspace");
}

#[tokio::test]
async fn edits_keep_the_mode_and_renames_move_content() {
    let fixture = Fixture::new();
    let parent = fixture.tip("main");
    let commit = fixture
        .commit_with(&parent, None, async |workspace| {
            vec![
                Change::Update { path: path("bin/run.sh"), blob: blob(workspace, b"#!/bin/sh\necho bye\n").await },
                Change::Move { from: path("README.md"), to: path("docs/README.md"), blob: None },
                Change::Move { from: path("win.txt"), to: path("windows/win.txt"), blob: Some(blob(workspace, b"three\r\n").await) },
            ]
        })
        .await
        .unwrap();
    let files = fixture.files(&commit);
    assert!(files.contains(&"100755 bin/run.sh".to_owned()), "{files:?}");
    assert!(files.contains(&"100644 docs/README.md".to_owned()), "{files:?}");
    assert!(!files.iter().any(|file| file.ends_with(" README.md")), "the source of a move is gone: {files:?}");
    assert_eq!(fixture.bytes(&commit, "bin/run.sh"), b"#!/bin/sh\necho bye\n");
    assert_eq!(fixture.bytes(&commit, "docs/README.md"), b"# Demo\n", "a rename without new content keeps the old");
    assert_eq!(fixture.bytes(&commit, "windows/win.txt"), b"three\r\n");
    let renames = fixture.bare(&["diff", "--name-status", "-M", &parent, &commit]);
    assert!(renames.contains("R100\tREADME.md\tdocs/README.md"), "{renames}");
}

#[tokio::test]
async fn deleting_the_last_file_of_a_directory_removes_the_directory() {
    let fixture = Fixture::new();
    let parent = fixture.tip("main");
    let commit = fixture.commit_with(&parent, None, async |_| vec![Change::Delete { path: path("docs/only.md") }]).await.unwrap();
    let files = fixture.files(&commit);
    assert!(!files.iter().any(|file| file.contains("docs")), "{files:?}");
    assert_eq!(fixture.bare(&["ls-tree", "--name-only", &commit, "--", "docs"]), "", "no empty directory is left");
    assert_eq!(files.len(), 4);
}

#[tokio::test]
async fn a_moved_branch_is_refused_and_left_alone() {
    let fixture = Fixture::new();
    let parent = fixture.tip("main");
    // Someone else commits first.
    let other = fixture.commit_with(&parent, None, async |workspace| {
        vec![Change::Update { path: path("README.md"), blob: blob(workspace, b"# Theirs\n").await }]
    });
    let other = other.await.unwrap();

    let stale = fixture.commit_with(&parent, None, async |workspace| {
        vec![Change::Update { path: path("README.md"), blob: blob(workspace, b"# Mine\n").await }]
    });
    assert!(matches!(stale.await, Err(EditError::BranchMoved)));
    assert_eq!(fixture.tip("main"), other, "the branch keeps the other commit");
    assert!(fixture.scratch_is_empty());
}

#[tokio::test]
async fn new_branches_start_at_the_parent_and_are_never_overwritten() {
    let fixture = Fixture::new();
    let parent = fixture.tip("main");
    let change = async |workspace: &EditWorkspace| vec![Change::Create { path: path("patch.txt"), blob: blob(workspace, b"x\n").await }];
    let commit = fixture.commit_with(&parent, Some("ada-patch-1"), change).await.unwrap();
    assert_eq!(fixture.tip("ada-patch-1"), commit);
    assert_eq!(fixture.tip("main"), parent, "the original branch does not move");

    let again = fixture.commit_with(&parent, Some("ada-patch-1"), change).await;
    assert!(matches!(&again, Err(EditError::BranchExists(name)) if name == "ada-patch-1"), "{again:?}");
    assert_eq!(fixture.tip("ada-patch-1"), commit);
    let invalid = fixture.commit_with(&parent, Some("bad..name"), change).await;
    assert!(matches!(invalid, Err(EditError::Git(_))), "{invalid:?}");
}

#[tokio::test]
async fn refusals_and_limits() {
    let fixture = Fixture::new();
    let parent = fixture.tip("main");
    let same = fixture.commit_with(&parent, None, async |workspace| {
        vec![Change::Update { path: path("README.md"), blob: blob(workspace, b"# Demo\n").await }]
    });
    assert!(matches!(same.await, Err(EditError::NothingChanged)));
    let clobber = fixture
        .commit_with(&parent, None, async |workspace| vec![Change::Create { path: path("README.md"), blob: blob(workspace, b"x").await }]);
    assert!(matches!(clobber.await, Err(EditError::Rejected(message)) if message == "a file called README.md already exists"));
    let through = fixture
        .commit_with(&parent, None, async |workspace| vec![Change::Create { path: path("win.txt/x"), blob: blob(workspace, b"x").await }]);
    assert!(matches!(through.await, Err(EditError::Rejected(_))));
    assert_eq!(fixture.tip("main"), parent);

    let workspace = fixture.workspace().await;
    let too_large = workspace.write_blob(&[b'x'; 2048][..], 2047).await;
    assert!(matches!(too_large, Err(EditError::TooLarge(2047))));
    assert!(workspace.write_blob(&[b'x'; 2048][..], 2048).await.is_ok(), "the limit itself is allowed");
    let unknown = EditSpec {
        branch: "main",
        parent: &"0".repeat(40),
        new_branch: None,
        changes: &[Change::Delete { path: path("README.md") }],
        message: "x\n",
        author: &fixture.author,
        at: Timestamp(1),
    };
    assert!(workspace.commit(&unknown).await.is_err(), "an unknown parent fails");
    drop(workspace);
    assert!(fixture.scratch_is_empty());
}
