//! Hostile reference names against every place rbit writes a reference (branch creation
//! by edits, tags, `HEAD`), case aliases on case-insensitive filesystems, and concurrent
//! writers losing the compare-and-swap.

use std::sync::Arc;

use rbit_core::{AccountId, RepoId, Timestamp, file_edit::FilePath, pull::CommitIdentity};
use rbit_git::{
    GitError, RepositoryReader, RepositoryStorage,
    edit::{Change, EditError, EditSpec, EditWorkspace, is_valid_branch_name},
};
use tokio::sync::Barrier;

use crate::hostile_support::Repo;
#[cfg(any(windows, target_os = "macos"))]
use crate::hostile_support::git;

#[tokio::test]
async fn pushed_replacement_refs_cannot_rewrite_the_objects_of_an_unchanged_branch() {
    use rbit_core::{
        RefUpdate, RepoAccess,
        protection::{ProtectionRule, Pusher, decide_push, reasons},
    };

    let repo = Repo::new(&[("README.md", b"approved\n")]);
    let main = repo.tip("main");
    let original = repo.bare(&["rev-parse", "main:README.md"]);
    let replacement = repo.bare_with_input(&["hash-object", "-w", "--stdin"], b"unreviewed\n");
    let name = format!("refs/replace/{original}");

    // A control proves why this ref is dangerous: even an exact blob id is
    // transparently redirected by gitoxide, while main has never moved.
    repo.bare(&["update-ref", &name, &replacement]);
    repo.config("core.useReplaceRefs", "true");
    let native = gix::open_opts(&repo.bare, gix::open::Options::isolated()).unwrap();
    assert_eq!(native.find_blob(gix::ObjectId::from_hex(original.as_bytes()).unwrap()).unwrap().data, b"unreviewed\n");
    drop(native);
    assert_eq!(repo.tip("main"), main);
    let tree = repo.bare(&["rev-parse", "main^{tree}"]);
    let empty_tree = repo.bare_with_input(&["hash-object", "-t", "tree", "-w", "--stdin"], b"");
    repo.bare(&["update-ref", &format!("refs/replace/{tree}"), &empty_tree]);

    // The shared HTTP/SSH gate must refuse this before receive-pack writes it,
    // including when there are no branch rules and the pusher is an administrator.
    let rules: &[ProtectionRule] = &[];
    let actor = Pusher::User { id: AccountId(1), access: RepoAccess::Admin, teams: vec![] };
    let updates = [RefUpdate { name: name.clone(), before: None, after: Some(replacement.clone()) }];
    let decision = decide_push(rules, &actor, &updates);
    assert_eq!(decision.verdict.refusals[0].reason, reasons::REPLACE_REFS);
    assert_eq!(decision.bypasses, Vec::new());
    // Existing replacement refs are ignored by both reader paths, including a
    // reopened pinned reader, and cannot be enabled through repository config.
    let reader = RepositoryReader::new(repo.bare.clone());
    for reader in [reader.clone(), reader.pinned()] {
        for _ in 0..2 {
            assert_eq!(reader.blob(main.clone(), "README.md".into(), 1024).await.unwrap().text(), Some("approved\n"));
            reader.close();
        }
    }
    // Browser edit validation must read the real parent too, consistently with
    // its Git subprocesses. The replacement tree has no README to update.
    let edited = edit(&repo, "main", &main, Some("reviewed-edit"), "reviewed edit\n").await.unwrap();
    assert_eq!(reader.blob(edited, "README.md".into(), 1024).await.unwrap().text(), Some("reviewed edit\n"));
    let cleanup = [RefUpdate { name, before: Some(replacement), after: None }];
    assert!(!decide_push(rules, &actor, &cleanup).verdict.is_refused(), "writers can clean up existing replacement refs");
    assert_eq!(repo.tip("main"), main);
}

/// Not reference names at all: traversal, absolute, empty parts, NUL and control
/// characters, NTFS alternate data streams, 8.3 short names, Git's own syntax, options.
const INVALID: &[&str] = &[
    "",
    "..",
    "a/../b",
    "../../config",
    "/abs",
    "a//b",
    "a/",
    ".hidden",
    "a\0b",
    "a\nb",
    "a\tb",
    "a\x7fb",
    "a b",
    "a\\b",
    "a:b",
    "main:stream",
    "main::$DATA",
    "MAIN~1",
    "PROGRA~1",
    "@{-1}",
    "x@{0}",
    "a^b",
    "a?b",
    "a*b",
    "a[b",
    "x.lock",
    "x.",
    "-delete",
    "--output=x",
];

/// Valid reference names that Windows reserves as device names.
const DEVICES: &[&str] = &["CON", "nul", "aux.txt", "COM1", "lpt1", "dir/PRN"];

fn author() -> CommitIdentity {
    CommitIdentity { name: "Ada".into(), email: "ada@example.com".into() }
}

/// Commits a change of `README.md` from `parent`, on `branch` or a `new_branch`.
async fn edit(repo: &Repo, branch: &str, parent: &str, new_branch: Option<&str>, content: &str) -> Result<String, EditError> {
    let workspace = EditWorkspace::open(&repo.env(), &repo.bare).await.unwrap();
    let blob = workspace.write_blob(content.as_bytes(), 1024).await.unwrap();
    let changes = [Change::Update { path: FilePath::parse("README.md").unwrap(), blob }];
    let spec = EditSpec { branch, parent, new_branch, changes: &changes, message: "edit\n", author: &author(), at: Timestamp(1) };
    workspace.commit(&spec).await.map(|committed| committed.commit)
}

/// Every branch and tag with its target.
async fn listing(reader: &RepositoryReader) -> Vec<(String, String)> {
    reader.refs().await.unwrap().into_iter().map(|r| (r.name, r.target)).collect()
}

fn entries(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<_> =
        std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[tokio::test]
async fn hostile_names_never_become_references() {
    let repo = Repo::new(&[("README.md", b"# Demo\n")]);
    let main = repo.tip("main");
    let reader = RepositoryReader::new(repo.bare.clone());
    let refs_before = listing(&reader).await;
    let around = entries(repo.dir.path());
    let storage = RepositoryStorage::new(repo.dir.path().join("storage"));
    storage.create(AccountId(1), RepoId(1), "main").await.unwrap();

    for name in INVALID {
        assert!(!is_valid_branch_name(name), "{name:?}");
        assert!(matches!(reader.create_tag((*name).to_owned(), main.clone()).await, Err(GitError::InvalidBranchName(_))), "{name:?}");
        assert!(edit(&repo, "main", &main, Some(name), "x\n").await.is_err(), "{name:?}");
        assert!(edit(&repo, name, &main, None, "x\n").await.is_err(), "{name:?}");
        if !name.starts_with('-') {
            let set = storage.set_head(AccountId(1), RepoId(1), name).await;
            assert!(matches!(set, Err(GitError::InvalidBranchName(_))), "{name:?}");
            assert!(matches!(storage.create(AccountId(1), RepoId(2), name).await, Err(GitError::InvalidBranchName(_))), "{name:?}");
        }
    }
    assert_eq!(listing(&reader).await, refs_before, "no reference was written");
    assert_eq!(repo.tip("main"), main);
    let head = std::fs::read_to_string(storage.path_for(AccountId(1), RepoId(1)).join("HEAD")).unwrap();
    assert_eq!(head, "ref: refs/heads/main\n");
    assert!(!storage.path_for(AccountId(1), RepoId(2)).exists());
    let mut after = entries(repo.dir.path());
    after.retain(|name| !matches!(name.as_str(), "storage" | "tmp"));
    assert_eq!(after, around, "nothing appeared next to the repository");

    // A device name either fails or becomes a real, readable reference: never a write
    // that reports success and is lost to a device.
    for name in DEVICES {
        let tagged = reader.create_tag((*name).to_owned(), main.clone()).await;
        let branch = edit(&repo, "main", &main, Some(name), &format!("{name}\n")).await;
        let refs = listing(&reader).await;
        if tagged.is_ok() {
            assert!(refs.contains(&((*name).to_owned(), main.clone())), "tag {name:?} is listed: {refs:?}");
            assert_eq!(reader.commit_id(format!("refs/tags/{name}")).await.unwrap(), main);
        }
        if let Ok(commit) = &branch {
            assert_eq!(&repo.tip(name), commit, "branch {name:?} reads back");
        }
        if cfg!(windows) {
            assert!(tagged.is_err() && branch.is_err(), "Windows cannot store {name:?} as a loose reference");
        }
    }
    assert_eq!(repo.tip("main"), main);
}

/// `main` and the tag `v1`, both packed: on a case-insensitive filesystem a loose
/// `MAIN` or `V1` would be what `main` and `v1` resolve to.
fn packed() -> (Repo, String) {
    let repo = Repo::new(&[("README.md", b"# Demo\n")]);
    repo.bare(&["tag", "v1", "main"]);
    repo.bare(&["pack-refs", "--all"]);
    let main = repo.tip("main");
    (repo, main)
}

#[cfg(any(windows, target_os = "macos"))]
#[tokio::test]
async fn case_aliases_cannot_replace_a_packed_reference() {
    // Plain Git shows the hazard: creating `MAIN` "from nothing" moves `main`.
    let (control, main) = packed();
    let other =
        git(control.dir.path(), &["--git-dir", control.bare.to_str().unwrap(), "commit-tree", "-m", "x", &format!("{main}^{{tree}}")]);
    control.bare(&["update-ref", "refs/heads/MAIN", &other, &"0".repeat(main.len())]);
    assert_eq!(control.tip("main"), other, "the fixture reproduces the alias");

    let (repo, main) = packed();
    let reader = RepositoryReader::new(repo.bare.clone());
    let created = edit(&repo, "main", &main, Some("MAIN"), "alias\n").await;
    assert!(matches!(&created, Err(EditError::BranchExists(name)) if name == "MAIN"), "{created:?}");
    assert!(matches!(edit(&repo, "MAIN", &main, None, "alias\n").await, Err(EditError::BranchMoved)));
    assert!(matches!(reader.create_tag("V1".into(), main.clone()).await, Err(GitError::AlreadyExists)));
    assert_eq!(repo.tip("main"), main, "main never moved");
    assert_eq!(reader.commit_id("refs/tags/v1".into()).await.unwrap(), main);
    let names: Vec<_> = listing(&reader).await.into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["main", "v1"]);
    // Distinct names still work.
    edit(&repo, "main", &main, Some("main-2"), "fine\n").await.unwrap();
}

#[cfg(not(any(windows, target_os = "macos")))]
#[tokio::test]
async fn case_distinct_references_are_separate_where_filesystems_keep_case() {
    let (repo, main) = packed();
    let reader = RepositoryReader::new(repo.bare.clone());
    let other = edit(&repo, "main", &main, Some("MAIN"), "distinct\n").await.unwrap();
    reader.create_tag("V1".into(), other.clone()).await.unwrap();
    assert_eq!((repo.tip("main"), repo.tip("MAIN")), (main.clone(), other.clone()));
    assert_eq!(reader.commit_id("refs/tags/v1".into()).await.unwrap(), main);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writers_lose_the_compare_and_swap() {
    const WRITERS: usize = 6;
    let repo = Arc::new(Repo::new(&[("README.md", b"# Demo\n")]));
    let main = repo.tip("main");
    for new_branch in [None, Some("ada-patch-1")] {
        let barrier = Arc::new(Barrier::new(WRITERS));
        let tasks: Vec<_> = (0..WRITERS)
            .map(|writer| {
                let (repo, barrier, main) = (Arc::clone(&repo), Arc::clone(&barrier), main.clone());
                tokio::spawn(async move {
                    let workspace = EditWorkspace::open(&repo.env(), &repo.bare).await.unwrap();
                    let blob = workspace.write_blob(format!("writer {writer}\n").as_bytes(), 1024).await.unwrap();
                    let changes = [Change::Update { path: FilePath::parse("README.md").unwrap(), blob }];
                    let spec = EditSpec {
                        branch: "main",
                        parent: &main,
                        new_branch,
                        changes: &changes,
                        message: "race\n",
                        author: &author(),
                        at: Timestamp(1),
                    };
                    barrier.wait().await;
                    workspace.commit(&spec).await.map(|committed| committed.commit)
                })
            })
            .collect();
        let mut won = Vec::new();
        for task in tasks {
            match task.await.unwrap() {
                Ok(commit) => won.push(commit),
                Err(EditError::BranchMoved) if new_branch.is_none() => {}
                Err(EditError::BranchExists(_)) if new_branch.is_some() => {}
                Err(other) => panic!("{new_branch:?}: unexpected {other:?}"),
            }
        }
        assert_eq!(won.len(), 1, "{new_branch:?}: exactly one writer wins: {won:?}");
        assert_eq!(repo.tip(new_branch.unwrap_or("main")), won[0], "{new_branch:?}: the winner's commit is the tip");
    }

    // Tags of different commits: one creation wins and the tag never moves afterwards.
    let reader = RepositoryReader::new(repo.bare.clone());
    let targets: Vec<String> =
        (0..WRITERS).map(|writer| repo.bare(&["commit-tree", "-m", &format!("target {writer}"), &format!("{main}^{{tree}}")])).collect();
    let barrier = Arc::new(Barrier::new(WRITERS));
    let tags: Vec<_> = targets
        .iter()
        .map(|target| {
            let (reader, barrier, target) = (reader.clone(), Arc::clone(&barrier), target.clone());
            tokio::spawn(async move {
                barrier.wait().await;
                reader.create_tag("release".into(), target).await
            })
        })
        .collect();
    let mut created = Vec::new();
    for task in tags {
        if let Ok(commit) = task.await.unwrap() {
            created.push(commit);
        }
    }
    assert_eq!(created.len(), 1, "exactly one tag creation wins: {created:?}");
    assert_eq!(reader.commit_id("refs/tags/release".into()).await.unwrap(), created[0]);
}
