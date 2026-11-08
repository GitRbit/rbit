//! Repository configuration that tries to run commands: filter drivers, rewritten
//! transports (`url.<base>.insteadOf` with `protocol.*.allow`, `core.sshCommand`),
//! `core.hooksPath` and the `hooks` directory, and `core.fsmonitor`. Each scenario first
//! proves with plain `git` that the configuration does run its command, then runs rbit's
//! real code path and checks that nothing ran.

use async_compression::tokio::bufread::GzipDecoder;
use rbit_core::{Timestamp, file_edit::FilePath, issue::MergeStrategy, pull::CommitIdentity};
use rbit_git::{
    Maintenance, RepositoryReader,
    archive::{ArchiveFormat, Archiver},
    edit::{Change, EditError, EditSpec, EditWorkspace},
    merge::{MergeSpec, merge},
    pull_refs,
};
use tokio::io::{AsyncReadExt, BufReader};

use crate::hostile_support::{Marker, Repo, run_git};

const SECRET: &[u8] = b"exact bytes, never filtered\n";

fn author() -> CommitIdentity {
    CommitIdentity { name: "Ada".into(), email: "ada@example.com".into() }
}

/// `main` with a file every filter attribute applies to, and `feature` one commit ahead.
fn fixture() -> Repo {
    let repo = Repo::new(&[(".gitattributes", b"* filter=evil\n"), ("data.txt", SECRET)]);
    let work = repo.work();
    crate::hostile_support::git(&work, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(work.join("feature.txt"), "feature\n").unwrap();
    crate::hostile_support::git(&work, &["add", "feature.txt"]);
    crate::hostile_support::git(&work, &["commit", "-q", "-m", "feature"]);
    crate::hostile_support::git(&work, &["push", "-q", repo.bare.to_str().unwrap(), "feature"]);
    repo
}

async fn archive(repo: &Repo, format: ArchiveFormat) -> Vec<u8> {
    let commit = repo.tip("main");
    let archiver = Archiver::new("git", repo.dir.path().join("tmp"));
    let mut stream = archiver.start(&repo.bare, &commit, "demo", format).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    if format == ArchiveFormat::TarGz {
        let mut tar = Vec::new();
        GzipDecoder::new(BufReader::new(bytes.as_slice())).read_to_end(&mut tar).await.unwrap();
        return tar;
    }
    bytes
}

#[tokio::test]
async fn archives_never_run_repository_filter_drivers() {
    let repo = fixture();
    let marker = Marker::new(repo.dir.path(), "filter-ran");
    let attributes = repo.dir.path().join("attributes");
    std::fs::write(&attributes, "* filter=evil\n").unwrap();
    repo.config("filter.evil.smudge", &format!("{}; cat", marker.command()));
    repo.config("filter.evil.clean", &format!("{}; cat", marker.command()));
    repo.config("filter.evil.required", "true");
    repo.config("core.attributesFile", &attributes.to_string_lossy().replace('\\', "/"));

    let commit = repo.tip("main");
    let control = run_git(repo.dir.path(), &["--git-dir", repo.bare.to_str().unwrap(), "archive", "--format=tar", &commit], &[]);
    assert!(control.status.success() && marker.ran(), "the fixture's filter runs under plain git archive");
    marker.reset();

    let zip = archive(&repo, ArchiveFormat::Zip).await;
    assert_eq!(zip[..2], *b"PK", "a complete zip archive");
    let tar = archive(&repo, ArchiveFormat::TarGz).await;
    assert!(tar.windows(SECRET.len()).any(|window| window == SECRET), "the tarball holds the committed bytes unfiltered");
    assert!(!marker.ran(), "rbit's archives never run a repository's filter driver");
    let overlays = repo.dir.path().join("tmp/overlays");
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while std::fs::read_dir(&overlays).is_ok_and(|mut entries| entries.next().is_some()) {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("each archive's overlay is removed once git exits");
}

/// Edits, merges and pull request fetches move objects between local repositories. A
/// repository whose configuration rewrites those paths to `ext::` or `ssh://` transports
/// (and allows them) must not get its command run.
#[tokio::test]
async fn local_fetches_never_take_a_transport_the_repository_rewrites() {
    for transport in ["ext", "ssh"] {
        let repo = fixture();
        let marker = Marker::new(repo.dir.path(), "transport-ran");
        let main = repo.tip("main");
        let fork = repo.dir.path().join("fork.git");
        crate::hostile_support::git(repo.dir.path(), &["clone", "-q", "--bare", repo.bare.to_str().unwrap(), fork.to_str().unwrap()]);
        repo.config("protocol.allow", "always");
        repo.config("protocol.ext.allow", "always");
        let tmp = repo.dir.path().join("tmp");
        for prefix in [tmp.to_str().unwrap(), fork.to_str().unwrap()] {
            if transport == "ext" {
                // `ext::` runs its command without a shell: `git init` makes the marker a directory.
                repo.config(&format!("url.ext::git init {}.insteadOf", marker.shell_path().replace(' ', "% ")), prefix);
            } else {
                repo.config("url.ssh://attacker.invalid/x.insteadOf", prefix);
            }
        }
        repo.config("core.sshCommand", &marker.command());
        let control = run_git(repo.dir.path(), &["--git-dir", repo.bare.to_str().unwrap(), "fetch", tmp.join("x").to_str().unwrap()], &[]);
        assert!(!control.status.success() && marker.ran(), "{transport}: the fixture's rewrite runs a command under plain git");
        marker.reset();

        let workspace = EditWorkspace::open(&repo.env(), &repo.bare).await.unwrap();
        let blob = workspace.write_blob(&b"edited\n"[..], 1024).await.unwrap();
        let changes = [Change::Update { path: FilePath::parse("data.txt").unwrap(), blob }];
        let spec = EditSpec {
            branch: "main",
            parent: &main,
            new_branch: None,
            changes: &changes,
            message: "edit\n",
            author: &author(),
            at: Timestamp(1),
        };
        assert!(matches!(workspace.commit(&spec).await, Err(EditError::Failed(_))), "{transport}: the rewritten fetch fails");
        drop(workspace);

        let feature = repo.tip("feature");
        let spec = MergeSpec {
            repo: &repo.bare,
            base_branch: "main",
            base_tip: &main,
            head: &feature,
            strategy: MergeStrategy::Merge,
            message: "merge\n",
            author: &author(),
            merger: &author(),
            at: Timestamp(1),
        };
        assert!(merge(&repo.env(), &spec).await.is_err(), "{transport}: the merge's hand-over fails");
        assert!(pull_refs::fetch("git".as_ref(), &repo.bare, &fork, "feature", 1).await.is_err(), "{transport}");
        assert!(!marker.ran(), "{transport}: no rewritten transport ran a command");
        assert_eq!(repo.tip("main"), main, "{transport}: nothing moved");
    }
}

fn packs(repo: &Repo) -> usize {
    let pack = repo.bare.join("objects").join("pack");
    std::fs::read_dir(pack).unwrap().filter(|entry| entry.as_ref().unwrap().path().extension().is_some_and(|ext| ext == "pack")).count()
}

/// Writes one new object into a pack of its own (no hook runs for `hash-object` or
/// `pack-objects`).
fn add_pack(repo: &Repo, content: &str) {
    let file = repo.dir.path().join(content);
    std::fs::write(&file, content).unwrap();
    let id = repo.bare(&["hash-object", "-w", file.to_str().unwrap()]);
    let base = repo.bare.join("objects").join("pack").join("pack");
    repo.bare_with_input(&["pack-objects", "-q", base.to_str().unwrap()], format!("{id}\n").as_bytes());
}

/// Maintenance, reference updates and reads under a repository that names its own hooks,
/// keeps hooks in its `hooks` directory, asks for an `fsmonitor` and for automatic gc.
#[tokio::test]
async fn maintenance_ref_updates_and_reads_never_run_repository_hooks() {
    let repo = fixture();
    let marker = Marker::new(repo.dir.path(), "hook-ran");
    // Two packs over a limit of one: `gc --auto` decides to collect and calls `pre-auto-gc`.
    repo.bare(&["repack", "-q", "-a", "-d"]);
    add_pack(&repo, "second");
    let hooks = repo.dir.path().join("hooks");
    for hook in ["pre-auto-gc", "reference-transaction", "post-update", "pre-receive", "update", "post-checkout", "post-commit"] {
        marker.hook(&hooks, hook);
        marker.hook(&repo.bare.join("hooks"), hook);
    }
    repo.config("gc.autoPackLimit", "1");
    repo.config("gc.autoDetach", "false");
    repo.config("core.fsmonitor", &marker.command());
    let control = run_git(repo.dir.path(), &["--git-dir", repo.bare.to_str().unwrap(), "gc", "--auto", "--quiet"], &[]);
    assert!(control.status.success() && marker.ran(), "the repository's own pre-auto-gc hook runs under plain git");
    marker.reset();
    add_pack(&repo, "third");
    repo.config("core.hooksPath", &hooks.to_string_lossy().replace('\\', "/"));

    let maintenance = Maintenance::new("git");
    let before = packs(&repo);
    maintenance.gc_auto(&repo.bare).await.unwrap();
    assert!(packs(&repo) < before, "gc ran, so it reached the point where pre-auto-gc would run");
    maintenance.write_commit_graph(&repo.bare).await.unwrap();

    let main = repo.tip("main");
    let workspace = EditWorkspace::open(&repo.env(), &repo.bare).await.unwrap();
    let blob = workspace.write_blob(&b"edited\n"[..], 1024).await.unwrap();
    let changes = [Change::Create { path: FilePath::parse("new.txt").unwrap(), blob }];
    let spec = EditSpec {
        branch: "main",
        parent: &main,
        new_branch: None,
        changes: &changes,
        message: "e\n",
        author: &author(),
        at: Timestamp(1),
    };
    let committed = workspace.commit(&spec).await.unwrap();
    assert_eq!(repo.tip("main"), committed.commit);
    pull_refs::set("git".as_ref(), &repo.bare, 7, &main).await.unwrap();
    assert!(pull_refs::delete_branch("git".as_ref(), &repo.bare, "feature", &repo.tip("feature")).await.unwrap());

    let reader = RepositoryReader::new(repo.bare.clone());
    assert!(!reader.refs().await.unwrap().is_empty());
    reader.blob("main".into(), "data.txt".into(), 1024).await.unwrap();
    reader.history("main".into(), 0, 10).await.unwrap();
    assert!(!marker.ran(), "no repository hook or fsmonitor command ran");
}
