//! Editing files in the browser: who may, read-only repositories, branch protection and the
//! new-branch route, a moving branch, line endings, uploads, the commit identity and the
//! push event, against real stores and a bare repository filled with the `git` CLI.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{Config, Timestamp, User, Visibility, membership::Permission, protection::RuleSettings};
use rbit_services::{
    CreateRepository, DomainEvent, RepoContext, ServiceError, Services,
    edits::{CommitInput, CommitTarget, EditAccess, EditFailure, MIRROR_READ_ONLY},
    protection::ProtectionInput,
};
use rbit_storage::{Database, KvStore, NewUser};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn bare(repo: &RepoContext, args: &[&str]) -> String {
    let mut full = vec!["--git-dir", repo.disk_path.to_str().unwrap()];
    full.extend_from_slice(args);
    git(&repo.disk_path, &full)
}

fn tip(repo: &RepoContext, branch: &str) -> String {
    bare(repo, &["rev-parse", &format!("refs/heads/{branch}")])
}

fn bytes(repo: &RepoContext, revision: &str, file: &str) -> Vec<u8> {
    let output = Command::new("git")
        .args(["--git-dir", repo.disk_path.to_str().unwrap(), "cat-file", "blob", &format!("{revision}:{file}")])
        .output()
        .unwrap();
    assert!(output.status.success());
    output.stdout
}

fn form(repo: &RepoContext, target: CommitTarget) -> CommitInput {
    CommitInput { branch: "main".into(), parent: tip(repo, "main"), target, title: String::new(), description: String::new() }
}

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    /// Owns `alice/demo`.
    alice: User,
    /// A collaborator with write access.
    bob: User,
    /// A collaborator with read access.
    rita: User,
}

impl Fixture {
    async fn new() -> Self {
        Self::with(|_| {}).await
    }

    /// `alice/demo` with `README.md` (LF), `win.txt` (CRLF), `logo.bin` (binary) and
    /// `big.dat` (a Git LFS pointer) on `main`.
    async fn with(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        config.server.public_url = "https://code.example.com".into();
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let user = |name: &'static str| {
            let database = database.clone();
            async move {
                let new = NewUser {
                    name: name.parse().unwrap(),
                    email: format!("{name}@example.com").parse().unwrap(),
                    display_name: None,
                    password_hash: "not-a-real-hash".to_owned(),
                    is_admin: false,
                };
                database.accounts().create_user(new).await.unwrap()
            }
        };
        let (alice, bob, rita) = (user("alice").await, user("bob").await, user("rita").await);
        let input = CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let fixture = Self { _dir: dir, database, services, alice, bob, rita };
        let repo = fixture.repo(&fixture.alice).await;
        fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
        fixture.services.collaborators.add(&fixture.alice, &repo, "rita", Permission::Read).await.unwrap();

        let work = tempfile::tempdir().unwrap();
        git(work.path(), &["init", "-q"]);
        std::fs::write(work.path().join("README.md"), "# Demo\n").unwrap();
        std::fs::write(work.path().join("win.txt"), "one\r\ntwo\r\n").unwrap();
        std::fs::write(work.path().join("logo.bin"), [0_u8, 159, 146, 150, 0, 1]).unwrap();
        let pointer = format!(
            "version https://git-lfs.github.com/spec/v1
oid sha256:{}
size 12
",
            "a".repeat(64)
        );
        std::fs::write(work.path().join("big.dat"), pointer).unwrap();
        git(work.path(), &["add", "."]);
        git(work.path(), &["commit", "-q", "-m", "initial"]);
        git(work.path(), &["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
        fixture
    }

    async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "demo").await.unwrap()
    }
}

#[tokio::test]
async fn a_writer_commits_once_and_the_push_is_announced() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.bob).await;
    let mut events = fixture.services.events.subscribe();
    let parent = tip(&repo, "main");
    let mut input = form(&repo, CommitTarget::Branch);
    input.title = "Explain the demo".into();
    input.description = "Fixes #1".into();

    assert_eq!(fixture.services.edits.access(&fixture.bob, &repo, "main").await.unwrap(), EditAccess::Direct);
    let edited = fixture.services.edits.update_file(&fixture.bob, &repo, &input, "README.md", "", "# Demo\r\n\r\nMore.").await.unwrap();
    assert_eq!((edited.branch.as_str(), edited.new_branch), ("main", false));
    assert_eq!(tip(&repo, "main"), edited.commit);
    assert_eq!(bare(&repo, &["rev-parse", &format!("{}^", edited.commit)]), parent);
    assert_eq!(bytes(&repo, &edited.commit, "README.md"), b"# Demo\n\nMore.", "LF kept, nothing appended");
    let commit = bare(&repo, &["cat-file", "commit", &edited.commit]);
    assert!(commit.contains("author bob <bob@noreply.code.example.com>"), "the noreply address: {commit}");
    assert!(commit.ends_with("Explain the demo\n\nFixes #1"), "{commit}");

    let DomainEvent::RepositoryPushed { repo: pushed, pusher, updates } = events.recv().await.unwrap() else {
        panic!("expected a push event");
    };
    assert_eq!((pushed, pusher), (repo.repo.repo.id, Some(fixture.bob.id())));
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].branch(), Some("main"));
    assert_eq!((updates[0].before.as_deref(), updates[0].after.as_deref()), (Some(parent.as_str()), Some(edited.commit.as_str())));
    let reopened = fixture.repo(&fixture.bob).await;
    assert!(reopened.repo.repo.pushed_at.is_some(), "pushed_at is recorded");

    // A verified address stays private too, as in merges.
    let email = "bob@example.com".parse().unwrap();
    assert!(fixture.database.recovery().mark_verified(fixture.bob.id(), &email, Timestamp(1)).await.unwrap());
    let input = form(&repo, CommitTarget::Branch);
    let created = fixture.services.edits.create_file(&fixture.bob, &repo, &input, "docs/new/guide.md", "Hello\r\n").await.unwrap();
    let commit = bare(&repo, &["cat-file", "commit", &created.commit]);
    assert!(commit.contains("author bob <bob@noreply.code.example.com>"), "{commit}");
    assert!(commit.ends_with("Create docs/new/guide.md"), "the default title: {commit}");
    assert_eq!(bytes(&repo, &created.commit, "docs/new/guide.md"), b"Hello\n");
}

#[tokio::test]
async fn readers_and_mirrors_cannot_edit() {
    let fixture = Fixture::new().await;
    let rita = fixture.repo(&fixture.rita).await;
    let input = form(&rita, CommitTarget::Branch);
    let refused = fixture.services.edits.create_file(&fixture.rita, &rita, &input, "x.txt", "x").await;
    assert!(matches!(refused, Err(EditFailure::Service(ServiceError::Forbidden))), "{refused:?}");
    assert!(!fixture.services.edits.can_edit(&rita).await.unwrap());
    assert!(matches!(fixture.services.edits.start_upload(&rita).await, Err(EditFailure::Service(ServiceError::Forbidden))));

    let repo = fixture.repo(&fixture.alice).await;
    assert!(fixture.services.edits.can_edit(&repo).await.unwrap());
    fixture.database.mirrors().create(repo.repo.repo.id, "https://example.com/upstream.git", 60).await.unwrap();
    assert!(!fixture.services.edits.can_edit(&repo).await.unwrap());
    let refused = fixture.services.edits.delete_file(&fixture.alice, &repo, &input, "README.md").await;
    assert!(matches!(&refused, Err(EditFailure::Invalid(message)) if message == MIRROR_READ_ONLY), "{refused:?}");
    assert!(matches!(fixture.services.edits.access(&fixture.alice, &repo, "main").await, Err(EditFailure::Invalid(_))));
    assert_eq!(tip(&repo, "main"), input.parent, "nothing was committed");
}

#[tokio::test]
async fn protected_branches_take_a_new_branch_instead() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let settings = RuleSettings { require_pull_request: true, ..RuleSettings::default() };
    let rule = ProtectionInput { pattern: "main".into(), settings, ..ProtectionInput::default() };
    fixture.services.protection.create(&fixture.alice, &repo, &rule).await.unwrap();
    let repo = fixture.repo(&fixture.bob).await;
    let parent = tip(&repo, "main");

    let access = fixture.services.edits.access(&fixture.bob, &repo, "main").await.unwrap();
    assert!(matches!(&access, EditAccess::NewBranchOnly(reason) if reason.contains("pull request")), "{access:?}");
    let direct = form(&repo, CommitTarget::Branch);
    let refused = fixture.services.edits.delete_file(&fixture.bob, &repo, &direct, "README.md").await;
    assert!(matches!(&refused, Err(EditFailure::Protected(reason)) if reason.contains("pull request")), "{refused:?}");
    assert_eq!(tip(&repo, "main"), parent);

    let suggested = fixture.services.edits.suggested_branch(&fixture.bob, &repo).await.unwrap();
    assert_eq!(suggested, "bob-patch-1");
    let branch = form(&repo, CommitTarget::NewBranch(suggested.clone()));
    let edited = fixture.services.edits.delete_file(&fixture.bob, &repo, &branch, "README.md").await.unwrap();
    assert_eq!((edited.branch.as_str(), edited.new_branch), ("bob-patch-1", true));
    assert_eq!(tip(&repo, "bob-patch-1"), edited.commit);
    assert_eq!(tip(&repo, "main"), parent, "main does not move");
    assert_eq!(fixture.services.edits.suggested_branch(&fixture.bob, &repo).await.unwrap(), "bob-patch-2");

    let taken = fixture.services.edits.delete_file(&fixture.bob, &repo, &branch, "win.txt").await;
    assert!(matches!(&taken, Err(EditFailure::Invalid(message)) if message == "a branch called bob-patch-1 already exists"), "{taken:?}");
    let invalid = form(&repo, CommitTarget::NewBranch("bad..name".into()));
    let invalid = fixture.services.edits.delete_file(&fixture.bob, &repo, &invalid, "win.txt").await;
    assert!(matches!(invalid, Err(EditFailure::Invalid(_))));
}

#[tokio::test]
async fn a_branch_that_moved_is_reported_not_rebased() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let stale = form(&repo, CommitTarget::Branch);
    let first = fixture.services.edits.create_file(&fixture.alice, &repo, &stale, "a.txt", "a").await.unwrap();
    let refused = fixture.services.edits.create_file(&fixture.alice, &repo, &stale, "b.txt", "b").await;
    assert!(matches!(&refused, Err(EditFailure::BranchMoved { current: Some(current) }) if *current == first.commit), "{refused:?}");
    assert_eq!(tip(&repo, "main"), first.commit);

    // A new branch may still start where the editor was opened.
    let branch = CommitInput { target: CommitTarget::NewBranch("from-old".into()), ..stale };
    let edited = fixture.services.edits.create_file(&fixture.alice, &repo, &branch, "b.txt", "b").await.unwrap();
    assert_eq!(bare(&repo, &["rev-parse", &format!("{}^", edited.commit)]), branch.parent);
}

#[tokio::test]
async fn text_keeps_its_line_endings_and_binaries_are_not_edited() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let input = form(&repo, CommitTarget::Branch);
    let edited = fixture.services.edits.update_file(&fixture.alice, &repo, &input, "win.txt", "", "one\r\ntwo\r\nthree").await.unwrap();
    assert_eq!(bytes(&repo, &edited.commit, "win.txt"), b"one\r\ntwo\r\nthree", "CRLF files stay CRLF");

    // A rename without changes moves the blob; the title says so.
    let input = form(&repo, CommitTarget::Branch);
    let moved = fixture.services.edits.update_file(&fixture.alice, &repo, &input, "win.txt", "text/win.txt", "one\r\ntwo\r\nthree").await;
    let moved = moved.unwrap();
    let message = bare(&repo, &["log", "-1", "--format=%s", &moved.commit]);
    assert_eq!(message, "Rename win.txt to text/win.txt");
    assert_eq!(bytes(&repo, &moved.commit, "text/win.txt"), b"one\r\ntwo\r\nthree");

    let input = form(&repo, CommitTarget::Branch);
    let binary = fixture.services.edits.update_file(&fixture.alice, &repo, &input, "logo.bin", "", "text").await;
    assert!(matches!(&binary, Err(EditFailure::Invalid(message)) if message.contains("binary")), "{binary:?}");
    let pointer = fixture.services.edits.update_file(&fixture.alice, &repo, &input, "big.dat", "", "text").await;
    assert!(matches!(&pointer, Err(EditFailure::Invalid(message)) if message.contains("Git LFS")), "{pointer:?}");
    let unchanged = fixture.services.edits.update_file(&fixture.alice, &repo, &input, "README.md", "", "# Demo\r\n").await;
    assert!(matches!(&unchanged, Err(EditFailure::Invalid(message)) if message == "there are no changes to commit"), "{unchanged:?}");
    for (path, message) in [("../x", "paths cannot contain . or .. parts"), (".git/config", "paths cannot contain a .git part")] {
        let refused = fixture.services.edits.create_file(&fixture.alice, &repo, &input, path, "x").await;
        assert!(matches!(&refused, Err(EditFailure::Invalid(m)) if m == message), "{refused:?}");
    }
    let clobber = fixture.services.edits.create_file(&fixture.alice, &repo, &input, "README.md", "x").await;
    assert!(matches!(&clobber, Err(EditFailure::Invalid(m)) if m == "a file called README.md already exists"), "{clobber:?}");
}

#[tokio::test]
async fn uploads_stream_into_one_commit_within_the_limits() {
    let fixture = Fixture::with(|config| {
        config.editor.max_upload_bytes = 16;
        config.editor.max_upload_files = 2;
    })
    .await;
    let repo = fixture.repo(&fixture.alice).await;
    let input = form(&repo, CommitTarget::Branch);

    let mut upload = fixture.services.edits.start_upload(&repo).await.unwrap();
    upload.add("C:\\Users\\alice\\one.txt", &b"first\r\n"[..]).await.unwrap();
    upload.add("", &b"ignored: no file chosen"[..]).await.unwrap();
    upload.add("README.md", &[0_u8, 1, 2][..]).await.unwrap();
    assert_eq!(upload.names().collect::<Vec<_>>(), ["one.txt", "README.md"]);
    let refused = upload.add("three.txt", &b"x"[..]).await;
    assert!(matches!(&refused, Err(EditFailure::Invalid(m)) if m == "at most 2 files can be uploaded at once"), "{refused:?}");
    let edited = fixture.services.edits.finish_upload(&fixture.alice, &repo, &input, upload, "assets/raw").await.unwrap();
    assert_eq!(bytes(&repo, &edited.commit, "assets/raw/one.txt"), b"first\r\n", "uploads are stored byte for byte");
    assert_eq!(bytes(&repo, &edited.commit, "assets/raw/README.md"), [0, 1, 2]);
    assert_eq!(bare(&repo, &["log", "-1", "--format=%s", &edited.commit]), "Add files via upload");

    // Same names at the root replace the files there.
    let input = form(&repo, CommitTarget::Branch);
    let mut upload = fixture.services.edits.start_upload(&repo).await.unwrap();
    upload.add("README.md", &b"# Replaced\n"[..]).await.unwrap();
    let edited = fixture.services.edits.finish_upload(&fixture.alice, &repo, &input, upload, "").await.unwrap();
    assert_eq!(bytes(&repo, &edited.commit, "README.md"), b"# Replaced\n");

    let mut upload = fixture.services.edits.start_upload(&repo).await.unwrap();
    let too_large = upload.add("big.bin", &[7_u8; 17][..]).await;
    assert!(
        matches!(&too_large, Err(EditFailure::Invalid(m)) if m == "big.bin is too large: files can be at most 16 bytes"),
        "{too_large:?}"
    );
    let input = form(&repo, CommitTarget::Branch);
    let empty = fixture.services.edits.finish_upload(&fixture.alice, &repo, &input, upload, "").await;
    assert!(matches!(&empty, Err(EditFailure::Invalid(m)) if m == "choose at least one file to upload"), "{empty:?}");
    let mut upload = fixture.services.edits.start_upload(&repo).await.unwrap();
    upload.add("x.txt", &b"x"[..]).await.unwrap();
    let into_file = fixture.services.edits.finish_upload(&fixture.alice, &repo, &input, upload, "README.md").await;
    assert!(matches!(&into_file, Err(EditFailure::Invalid(m)) if m.contains("README.md is a file")), "{into_file:?}");
}
