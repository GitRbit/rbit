//! The pull request service against real repositories: creation across a fork,
//! `refs/pull/{n}/head`, pushes to head and base, mergeability, reviews, every merge
//! strategy, reopening rules and webhook payloads.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{
    Config, IssueNumber, IssueState, Permission, StateReason, TimelineEvent, User, Visibility,
    issue::{IssueAction, MergeStrategy, ReviewState},
    pull::{HeadSpec, Mergeability, ReviewSide},
};
use rbit_git::DiffLimits;
use rbit_services::{
    CreateRepository, DomainEvent, RepoContext, ServiceError, Services,
    forks::ForkInput,
    pulls::{LineComment, MergeFailure, MergeInput, NewPullInput, PullContext, PullCounts, ReviewVerdict},
    timeline::{StateChange, TimelineEntry},
};
use rbit_storage::{Database, KvStore, NewUser, kv::CachedComparisonCounts};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(dir: &Path, file: &str, contents: &str, message: &str) -> String {
    std::fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    kv: KvStore,
    alice: User,
    bob: User,
    carol: User,
    work: std::path::PathBuf,
}

async fn user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

impl Fixture {
    /// alice/demo with `main` (README) and `feature` (one commit ahead); bob forked it;
    /// carol is a writer of alice/demo.
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let carol = user(&database, "carol").await;
        let input = CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let repo = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
        services.collaborators.add(&alice, &repo, "carol", Permission::Write).await.unwrap();

        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        git(&work, &["init", "-q"]);
        commit(&work, "README.md", "# Demo\n\nline two\nline three\n", "Initial commit");
        git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "main"]);
        git(&work, &["checkout", "-q", "-b", "feature"]);
        commit(&work, "feature.txt", "one\n", "Add the feature");
        git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "feature"]);
        git(&work, &["checkout", "-q", "main"]);

        let fixture = Self { _dir: dir, database, services, kv, alice, bob, carol, work };
        let parent = fixture.repo(&fixture.bob, "alice").await;
        fixture.services.forks.fork(&fixture.bob, &parent, ForkInput::default()).await.unwrap();
        fixture
    }

    async fn repo(&self, viewer: &User, owner: &str) -> RepoContext {
        self.services.repositories.open(Some(viewer), owner, "demo").await.unwrap()
    }

    /// Pushes `refspec` from the work tree to `owner/demo` and reports it like a transport.
    async fn push(&self, pusher: &User, owner: &str, args: &[&str]) {
        let repo = self.repo(pusher, owner).await;
        let pending = self.services.pushes.begin(&repo, Some(pusher.id())).await;
        let mut full = vec!["push", "-q"];
        full.extend_from_slice(args);
        let path = repo.disk_path.to_str().unwrap().to_owned();
        full.insert(2, &path);
        git(&self.work, &full);
        let mut events = self.services.events.subscribe();
        self.services.pushes.complete(pending).await;
        let Ok(DomainEvent::RepositoryPushed { repo, pusher, updates }) = events.recv().await else { panic!("a push event") };
        self.services.pulls.handle_push(repo, pusher, &updates).await.unwrap();
    }

    async fn open_pull(&self, viewer: &User, number: u32) -> PullContext {
        let repo = self.repo(viewer, "alice").await;
        self.services.pulls.open(Some(viewer), repo, IssueNumber::new(number).unwrap()).await.unwrap()
    }

    async fn create(&self, actor: &User, head_owner: &str, head: &str) -> Result<u32, ServiceError> {
        let base = self.repo(actor, "alice").await;
        let spec = HeadSpec::parse(&format!("{head_owner}:{head}")).unwrap();
        let head_repo = self.services.pulls.resolve_head(Some(actor), &base, &spec).await?;
        let input = NewPullInput { base_branch: "main".into(), head_branch: head.into(), ..NewPullInput::default() };
        Ok(self.services.pulls.create(actor, &base, &head_repo, input).await?.0.number.get())
    }

    async fn events(&self, viewer: &User, number: u32) -> Vec<TimelineEvent> {
        let ctx = self.open_pull(viewer, number).await;
        let timeline = self.services.timeline.timeline(Some(viewer), &ctx.thread, None, 200).await.unwrap();
        timeline
            .entries
            .into_iter()
            .filter_map(|entry| if let TimelineEntry::Event { event, .. } = entry { Some(event) } else { None })
            .collect()
    }

    fn merge_input(strategy: MergeStrategy) -> MergeInput {
        MergeInput { strategy, title: String::new(), message: String::new(), expected_head: None, delete_head_branch: false, bypass: false }
    }
}

#[tokio::test]
async fn review_threads_require_submitted_roots_and_writer_or_root_author() {
    let f = Fixture::new().await;
    let number = f.create(&f.alice, "alice", "feature").await.unwrap();
    let bob_ctx = f.open_pull(&f.bob, number).await;
    let alice_ctx = f.open_pull(&f.alice, number).await;
    let carol_ctx = f.open_pull(&f.carol, number).await;
    let pulls = &f.services.pulls;
    let input = LineComment {
        path: "feature.txt".into(),
        side: ReviewSide::Right,
        line: 1,
        body: "Please explain this".into(),
        in_reply_to: None,
        start_review: false,
    };
    let root = pulls.comment_on_line(&f.bob, &bob_ctx, input.clone()).await.unwrap();
    assert!(root.resolved_at.is_none());
    let reply = pulls.comment_on_line(&f.alice, &alice_ctx, LineComment { in_reply_to: Some(root.id), ..input.clone() }).await.unwrap();
    // A public reader who authored only a reply has no resolution permission.
    let dave = user(&f.database, "dave").await;
    let dave_ctx = f.open_pull(&dave, number).await;
    let reader_reply = pulls.comment_on_line(&dave, &dave_ctx, LineComment { in_reply_to: Some(reply.id), ..input.clone() }).await.unwrap();
    assert!(matches!(pulls.set_thread_resolved(&dave, &dave_ctx, reader_reply.id, true).await, Err(ServiceError::Forbidden)));
    let resolved = pulls.set_thread_resolved(&f.bob, &bob_ctx, reply.id, true).await.unwrap();
    assert_eq!(resolved.id, root.id);
    assert_eq!(resolved.resolved_by, Some(f.bob.id()));
    assert!(resolved.resolved_at.is_some());
    // Repeated posts preserve who resolved the thread and produce no extra event.
    assert_eq!(pulls.set_thread_resolved(&f.carol, &carol_ctx, root.id, true).await.unwrap(), resolved);
    let reopened = pulls.set_thread_resolved(&f.carol, &carol_ctx, root.id, false).await.unwrap();
    assert_eq!((reopened.resolved_at, reopened.resolved_by), (None, None));
    let events = f.events(&f.alice, number).await;
    assert_eq!(events.iter().filter(|e| matches!(e, TimelineEvent::ReviewThreadResolved { comment } if *comment == root.id)).count(), 1);
    assert_eq!(events.iter().filter(|e| matches!(e, TimelineEvent::ReviewThreadReopened { comment } if *comment == root.id)).count(), 1);
    // Even the pending review's author and repository writers cannot resolve it.
    let pending = pulls.comment_on_line(&f.bob, &bob_ctx, LineComment { start_review: true, ..input }).await.unwrap();
    for (actor, ctx) in [(&f.bob, &bob_ctx), (&f.alice, &alice_ctx)] {
        assert!(matches!(pulls.set_thread_resolved(actor, ctx, pending.id, true).await, Err(ServiceError::NotFound)));
    }
    // Resolution remains independent of whether the anchor is outdated.
    f.database.reviews().mark_outdated(&[root.id]).await.unwrap();
    assert!(pulls.set_thread_resolved(&f.bob, &bob_ctx, root.id, true).await.unwrap().outdated);
}

#[tokio::test]
async fn cached_counts_are_only_those_of_the_current_file_limit() {
    let f = Fixture::new().await;
    let number = f.create(&f.alice, "alice", "feature").await.unwrap();
    let ctx = f.open_pull(&f.alice, number).await;
    let pulls = &f.services.pulls;
    let (base, head) = pulls.compared_ids(&ctx).await.unwrap();
    assert_eq!(pulls.cached_counts(&base, &head).await, None);
    pulls.changes(&ctx, DiffLimits::default()).await.unwrap();
    let counted = PullCounts { ahead: 1, files: 1 };
    assert_eq!(pulls.cached_counts(&base, &head).await, Some(counted), "the Files changed tab counts for the others");

    // Counts made under another file limit (another server version) are not this answer.
    let other = CachedComparisonCounts { ahead: 1, files: 0, max_files: 0, additions: 0, deletions: 0 };
    f.kv.insights().put_comparison_counts(&base, &head, other).await.unwrap();
    assert_eq!(pulls.cached_counts(&base, &head).await, None);
    assert_eq!(pulls.counts_between(&ctx, &base, &head).await.unwrap(), counted, "recounted");
    assert_eq!(pulls.cached_counts(&base, &head).await, Some(counted), "and remembered again");
    // Only for the pull request's own pair.
    assert!(matches!(pulls.counts_between(&ctx, &head, &base).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn pull_json_statistics_come_from_the_comparison_cache() {
    let f = Fixture::new().await;
    let number = f.create(&f.alice, "alice", "feature").await.unwrap();
    let ctx = f.open_pull(&f.alice, number).await;
    let pulls = &f.services.pulls;
    let (base, head) = pulls.compared_ids(&ctx).await.unwrap();
    let figures = |json: &serde_json::Value| (json["commits"].clone(), json["additions"].clone(), json["changed_files"].clone());

    // A miss diffs once and remembers every figure, line counts included.
    assert_eq!(f.kv.insights().comparison_counts(&base, &head).await.unwrap(), None);
    let json = pulls.json(&ctx, "https://rbit.test").await.unwrap();
    assert_eq!(figures(&json), (1.into(), 1.into(), 1.into()));
    let cached = f.kv.insights().comparison_counts(&base, &head).await.unwrap().unwrap();
    assert_eq!((cached.ahead, cached.files, cached.additions, cached.deletions), (1, 1, 1, 0));

    // A hit does not diff at all: the cached figures are what the JSON shows.
    let marked = CachedComparisonCounts { ahead: 7, files: 9, additions: 42, deletions: 5, ..cached };
    f.kv.insights().put_comparison_counts(&base, &head, marked).await.unwrap();
    let json = pulls.json(&ctx, "https://rbit.test").await.unwrap();
    assert_eq!(figures(&json), (7.into(), 42.into(), 9.into()));
    assert_eq!(json["deletions"], 5);

    // Counts written under another file limit are recomputed.
    f.kv.insights().put_comparison_counts(&base, &head, CachedComparisonCounts { max_files: 0, ..marked }).await.unwrap();
    assert_eq!(figures(&pulls.json(&ctx, "https://rbit.test").await.unwrap()), (1.into(), 1.into(), 1.into()));
}

#[tokio::test]
async fn pull_request_events_without_subscribers_build_no_payload() {
    let f = Fixture::new().await;
    let mut events = f.services.events.subscribe();
    let number = f.create(&f.alice, "alice", "feature").await.unwrap();
    let opened = events.recv().await.unwrap();
    assert!(matches!(opened, DomainEvent::Issue { action: IssueAction::Opened, .. }));
    let ctx = f.open_pull(&f.alice, number).await;
    let (base, head) = f.services.pulls.compared_ids(&ctx).await.unwrap();

    assert_eq!(f.services.webhooks.handle_event(&opened).await.unwrap(), 0);
    assert_eq!(f.kv.insights().comparison_counts(&base, &head).await.unwrap(), None, "nothing was diffed for nobody");
    // Tools still get the payload on request.
    assert!(f.services.pulls.payload(&opened, "https://rbit.test").await.unwrap().is_some());
}

#[tokio::test]
async fn creating_validates_numbers_and_stores_the_head() {
    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    let mut events = fixture.services.events.subscribe();
    let number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    assert_eq!(number, 1);
    let ctx = fixture.open_pull(&fixture.bob, 1).await;
    assert_eq!(
        (ctx.issue().title.as_str(), ctx.pull.is_cross_repository(), ctx.head_label()),
        ("Add the feature", false, "feature".to_owned())
    );
    let head = ctx.repo().reader.commit_id("refs/heads/feature".into()).await.unwrap();
    assert_eq!(ctx.repo().reader.commit_id("refs/pull/1/head".into()).await.unwrap(), head);
    assert!(matches!(events.recv().await.unwrap(), DomainEvent::Issue { action: IssueAction::Opened, .. }));

    for (head, message) in [("feature", "already proposes"), ("main", "must differ"), ("missing", "does not exist")] {
        let error = fixture.create(&fixture.alice, "alice", head).await.unwrap_err();
        assert!(error.to_string().contains(message), "{head}: {error}");
    }
    git(&fixture.work, &["branch", "behind", "main"]);
    fixture.push(&fixture.alice, "alice", &["behind"]).await;
    assert!(fixture.create(&fixture.alice, "alice", "behind").await.unwrap_err().to_string().contains("already contains"));

    let commits = pulls.commits(&ctx).await.unwrap();
    assert_eq!(commits.len(), 1);
    let files = pulls.changes(&ctx, DiffLimits::default()).await.unwrap().files;
    assert_eq!(files[0].path, "feature.txt");

    // Issue numbers are not pull requests.
    let repo = fixture.repo(&fixture.alice, "alice").await;
    assert!(matches!(pulls.open(Some(&fixture.alice), repo, IssueNumber::new(9).unwrap()).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn a_fork_pull_request_follows_pushes_and_outlives_the_fork() {
    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    git(&fixture.work, &["checkout", "-q", "-b", "fork-work", "main"]);
    commit(&fixture.work, "fork.txt", "from the fork\n", "Fork work");
    fixture.push(&fixture.bob, "bob", &["fork-work"]).await;

    // Compare across the network before anything exists in the parent.
    let base = fixture.repo(&fixture.bob, "alice").await;
    let fork = pulls.resolve_head(Some(&fixture.bob), &base, &HeadSpec::parse("bob:fork-work").unwrap()).await.unwrap();
    assert_eq!(fork.repo.full_name(), "bob/demo");
    let comparison = pulls.compare(&base, "main", &fork, "fork-work", DiffLimits::default(), 10).await.unwrap();
    assert_eq!((comparison.ahead, comparison.files.len()), (1, 1));
    assert!(base.reader.commit_id(comparison.head.id.clone()).await.is_err(), "nothing was written to the parent");
    assert!(pulls.resolve_head(Some(&fixture.bob), &base, &HeadSpec::parse("nobody:x").unwrap()).await.is_err());

    let number = fixture.create(&fixture.bob, "bob", "fork-work").await.unwrap();
    let ctx = fixture.open_pull(&fixture.alice, number).await;
    assert!(ctx.pull.is_cross_repository());
    assert_eq!(ctx.head_label(), "bob:fork-work");
    assert_eq!(base.reader.commit_id(format!("refs/pull/{number}/head")).await.unwrap(), ctx.pull.head_sha);

    // A line comment, then a push that changes the file: the comment becomes outdated.
    let comment = LineComment {
        path: "fork.txt".into(),
        side: ReviewSide::Right,
        line: 1,
        body: "Typo?".into(),
        in_reply_to: None,
        start_review: false,
    };
    let stored = pulls.comment_on_line(&fixture.alice, &ctx, comment.clone()).await.unwrap();
    assert!(stored.diff_hunk.ends_with("+from the fork"), "{}", stored.diff_hunk);
    let bad = LineComment { line: 99, ..comment.clone() };
    assert!(matches!(pulls.comment_on_line(&fixture.alice, &ctx, bad).await, Err(ServiceError::Invalid(_))));

    let before = ctx.pull.head_sha.clone();
    commit(&fixture.work, "fork.txt", "from the fork, fixed\n", "Fix the typo");
    fixture.push(&fixture.bob, "bob", &["fork-work"]).await;
    let moved = fixture.open_pull(&fixture.alice, number).await;
    assert_ne!(moved.pull.head_sha, before);
    assert_eq!(base.reader.commit_id(format!("refs/pull/{number}/head")).await.unwrap(), moved.pull.head_sha);
    let reviews = pulls.reviews(Some(&fixture.alice), &moved).await.unwrap();
    assert!(reviews.visible_comments().all(|(comment, _)| comment.outdated));
    assert!(
        fixture
            .events(&fixture.alice, number)
            .await
            .iter()
            .any(|event| matches!(event, TimelineEvent::HeadPushed { count: 1, forced: false, .. }))
    );

    // A force push is recorded as such.
    git(&fixture.work, &["commit", "-q", "--amend", "-m", "Fix the typo (amended)"]);
    fixture.push(&fixture.bob, "bob", &["--force", "fork-work"]).await;
    assert!(
        fixture.events(&fixture.alice, number).await.iter().any(|event| matches!(event, TimelineEvent::HeadPushed { forced: true, .. }))
    );

    // Deleting the fork keeps the pull request and its commits.
    let fork = fixture.repo(&fixture.bob, "bob").await;
    fixture.services.repo_settings.delete(&fixture.bob, &fork, "bob/demo").await.unwrap();
    let orphaned = fixture.open_pull(&fixture.alice, number).await;
    assert!(orphaned.head_repo.is_none());
    assert_eq!(pulls.commits(&orphaned).await.unwrap().len(), 2);
    assert!(!pulls.head_branch_exists(&orphaned).await);
    let merge_box = pulls.merge_box(Some(&fixture.alice), &orphaned).await.unwrap();
    assert!(!merge_box.evaluation.can_merge(), "a missing head branch blocks the merge");
}

#[tokio::test]
async fn review_tally_rechecks_grants_for_repeated_former_writers() {
    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    let number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    let ctx = fixture.open_pull(&fixture.alice, number).await;
    for verdict in [ReviewVerdict::Comment, ReviewVerdict::RequestChanges, ReviewVerdict::Approve] {
        pulls.submit_review(&fixture.carol, &ctx, verdict, "Review").await.unwrap();
    }
    let repo = fixture.repo(&fixture.alice, "alice").await;
    assert_eq!(pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap().tally.approvals, [fixture.carol.id()]);
    fixture.services.collaborators.change(&fixture.alice, &repo, "carol", Permission::Read).await.unwrap();
    assert_eq!(pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap().tally.approvals.len(), 0);
    fixture.services.collaborators.change(&fixture.alice, &repo, "carol", Permission::Write).await.unwrap();
    assert_eq!(pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap().tally.approvals, [fixture.carol.id()]);
    fixture.services.collaborators.remove(&fixture.alice, &repo, "carol").await.unwrap();
    assert_eq!(pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap().tally.approvals.len(), 0);
}

#[tokio::test]
async fn a_merge_box_from_loaded_reviews_counts_the_same_votes() {
    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    let number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    let ctx = fixture.open_pull(&fixture.alice, number).await;
    pulls.submit_review(&fixture.carol, &ctx, ReviewVerdict::RequestChanges, "Not yet").await.unwrap();
    pulls.submit_review(&fixture.carol, &ctx, ReviewVerdict::Approve, "Now").await.unwrap();
    for viewer in [Some(&fixture.alice), Some(&fixture.carol), None] {
        // What the conversation page loaded for this viewer (no pending reviews: never counted).
        let reviews = pulls.reviews(viewer, &ctx).await.unwrap();
        let read = pulls.merge_box(viewer, &ctx).await.unwrap();
        let passed = pulls.merge_box_from(viewer, &ctx, &reviews).await.unwrap();
        assert_eq!(passed.tally.approvals, [fixture.carol.id()]);
        assert_eq!((passed.tally.approvals, passed.tally.changes_requested), (read.tally.approvals, read.tally.changes_requested));
        assert_eq!(passed.evaluation.can_merge(), read.evaluation.can_merge());
        assert_eq!((passed.can_merge, passed.can_delete_head), (read.can_merge, read.can_delete_head));
        assert_eq!(passed.default_squash_message, read.default_squash_message);
    }
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario from mergeability to the last merge")]
async fn mergeability_reviews_and_every_merge_strategy() {
    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    // Three pull requests from alice's branches, main moved on meanwhile.
    for branch in ["squash-me", "rebase-me", "conflict"] {
        git(&fixture.work, &["checkout", "-q", "-b", branch, "main"]);
        commit(&fixture.work, &format!("{branch}.txt"), "work\n", &format!("Work on {branch}"));
        if branch == "conflict" {
            commit(&fixture.work, "README.md", "# Demo\n\nchanged on the branch\nline three\n", "Edit the README");
        }
        fixture.push(&fixture.alice, "alice", &[branch]).await;
    }
    git(&fixture.work, &["checkout", "-q", "main"]);
    commit(&fixture.work, "README.md", "# Demo\n\nchanged on main\nline three\n", "Edit the README on main");
    fixture.push(&fixture.alice, "alice", &["main"]).await;

    let merge_number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    let squash = fixture.create(&fixture.alice, "alice", "squash-me").await.unwrap();
    let rebase = fixture.create(&fixture.alice, "alice", "rebase-me").await.unwrap();
    let conflict = fixture.create(&fixture.alice, "alice", "conflict").await.unwrap();
    assert_eq!(pulls.check_unknown().await.unwrap(), 4);
    let conflicted = fixture.open_pull(&fixture.alice, conflict).await;
    assert_eq!((conflicted.pull.mergeable, conflicted.pull.conflict_paths.clone()), (Mergeability::Conflict, vec!["README.md".to_owned()]));
    let refused = pulls.merge(&fixture.alice, &conflicted, Fixture::merge_input(MergeStrategy::Merge)).await.unwrap_err();
    assert!(matches!(refused, MergeFailure::Blocked(_)), "{refused}");

    // Reviews: the author cannot approve; bob (a reader) can, but only carol (a writer) counts.
    let ctx = fixture.open_pull(&fixture.alice, merge_number).await;
    assert_eq!(ctx.pull.mergeable, Mergeability::Clean);
    assert!(pulls.submit_review(&fixture.alice, &ctx, ReviewVerdict::Approve, "").await.is_err());
    pulls.submit_review(&fixture.bob, &fixture.open_pull(&fixture.bob, merge_number).await, ReviewVerdict::Approve, "").await.unwrap();
    let changes = pulls.submit_review(&fixture.carol, &ctx, ReviewVerdict::RequestChanges, "Needs work").await.unwrap();
    let merge_box = pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap();
    assert_eq!((merge_box.tally.approvals.len(), merge_box.tally.changes_requested.len()), (0, 1));
    let as_bob = fixture.open_pull(&fixture.bob, merge_number).await;
    assert!(pulls.dismiss_review(&fixture.bob, &as_bob, changes.id, "Not yours").await.is_err(), "readers cannot dismiss");
    pulls.dismiss_review(&fixture.alice, &ctx, changes.id, "Addressed").await.unwrap();
    let pending = LineComment {
        path: "feature.txt".into(),
        side: ReviewSide::Right,
        line: 1,
        body: "Nice".into(),
        in_reply_to: None,
        start_review: true,
    };
    pulls.comment_on_line(&fixture.carol, &ctx, pending).await.unwrap();
    assert!(pulls.reviews(Some(&fixture.alice), &ctx).await.unwrap().pending.is_none(), "pending reviews are private");
    assert!(pulls.reviews(Some(&fixture.carol), &ctx).await.unwrap().pending.is_some());
    pulls.submit_review(&fixture.carol, &ctx, ReviewVerdict::Approve, "").await.unwrap();
    let merge_box = pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap();
    assert_eq!(merge_box.tally.approvals, [fixture.carol.id()]);
    assert!(merge_box.evaluation.can_merge());
    assert!(fixture.events(&fixture.alice, merge_number).await.iter().any(|event| matches!(event, TimelineEvent::ReviewDismissed { .. })));

    // Readers cannot merge; a stale expected head is refused.
    assert!(matches!(
        pulls.merge(&fixture.bob, &fixture.open_pull(&fixture.bob, merge_number).await, Fixture::merge_input(MergeStrategy::Merge)).await,
        Err(MergeFailure::Service(ServiceError::Forbidden))
    ));
    let stale = MergeInput { expected_head: Some("0".repeat(40)), ..Fixture::merge_input(MergeStrategy::Merge) };
    assert!(matches!(pulls.merge(&fixture.alice, &ctx, stale).await, Err(MergeFailure::HeadChanged)));

    let mut events = fixture.services.events.subscribe();
    let input = MergeInput { delete_head_branch: true, ..Fixture::merge_input(MergeStrategy::Merge) };
    let merge_commit = pulls.merge(&fixture.alice, &ctx, input).await.unwrap();
    let repo = fixture.repo(&fixture.alice, "alice").await;
    assert_eq!(repo.reader.commit_id("refs/heads/main".into()).await.unwrap(), merge_commit);
    let detail = repo.reader.commit_detail(merge_commit.clone(), DiffLimits::default()).await.unwrap();
    assert_eq!(detail.commit.parent_ids.len(), 2);
    assert_eq!(detail.commit.summary, "Merge pull request #1 from alice/feature");
    assert_eq!(detail.commit.author_email, "alice@noreply.localhost", "never a real address");
    assert!(repo.reader.commit_id("refs/heads/feature".into()).await.is_err(), "the head branch was deleted");
    let merged = fixture.open_pull(&fixture.alice, merge_number).await;
    assert!(merged.pull.is_merged());
    assert_eq!((merged.issue().state, merged.issue().state_reason), (IssueState::Closed, Some(StateReason::Merged)));
    assert!(
        fixture
            .events(&fixture.alice, merge_number)
            .await
            .iter()
            .any(|event| matches!(event, TimelineEvent::Merged { strategy: Some(MergeStrategy::Merge), .. }))
    );
    let published: Vec<DomainEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(
        published.iter().any(
            |event| matches!(event, DomainEvent::RepositoryPushed { updates, .. } if updates.iter().any(|u| u.branch() == Some("main")))
        )
    );
    assert!(
        published
            .iter()
            .any(|event| matches!(event, DomainEvent::Issue { action: IssueAction::Closed { reason: StateReason::Merged }, .. }))
    );

    // Webhook payload of the merge.
    let closed = published.iter().find(|event| matches!(event, DomainEvent::Issue { action: IssueAction::Closed { .. }, .. })).unwrap();
    let payload = fixture.services.pulls.payload(closed, "https://git.example").await.unwrap().unwrap();
    assert_eq!(payload.body["action"], "closed");
    assert_eq!(payload.body["pull_request"]["merged"], true);
    assert_eq!(payload.body["pull_request"]["head"]["ref"], "feature");
    assert!(!payload.body.to_string().contains("@example.com"), "no email address in payloads");

    // Squash and rebase; main has moved under them, so mergeability is recomputed on demand.
    let squash_ctx = fixture.open_pull(&fixture.alice, squash).await;
    let squashed = pulls.merge(&fixture.alice, &squash_ctx, Fixture::merge_input(MergeStrategy::Squash)).await.unwrap();
    let detail = repo.reader.commit_detail(squashed.clone(), DiffLimits::default()).await.unwrap();
    assert_eq!((detail.commit.parent_ids.len(), detail.commit.summary.as_str()), (1, "Work on squash-me (#2)"));
    let rebase_ctx = fixture.open_pull(&fixture.alice, rebase).await;
    let rebased = pulls.merge(&fixture.alice, &rebase_ctx, Fixture::merge_input(MergeStrategy::Rebase)).await.unwrap();
    let detail = repo.reader.commit_detail(rebased.clone(), DiffLimits::default()).await.unwrap();
    assert_eq!(
        (detail.commit.summary.as_str(), detail.commit.parent_ids.as_slice()),
        ("Work on rebase-me", std::slice::from_ref(&squashed))
    );
    assert_eq!(detail.commit.author_email, "ada@example.com", "rebased commits keep their author");

    // Reopening a merged pull request is refused.
    let merged_ctx = fixture.open_pull(&fixture.alice, squash).await;
    let error = fixture.services.timeline.set_state(&fixture.alice, &merged_ctx.thread, StateChange::Reopen).await.unwrap_err();
    assert!(error.to_string().contains("merged"), "{error}");
}

#[tokio::test]
async fn merged_outside_rbit_and_reopening_rules() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    git(&fixture.work, &["checkout", "-q", "-b", "other", "main"]);
    commit(&fixture.work, "other.txt", "x\n", "Other work");
    fixture.push(&fixture.alice, "alice", &["other"]).await;
    let other = fixture.create(&fixture.alice, "alice", "other").await.unwrap();

    // Pushing the head into main by hand marks the pull request merged.
    git(&fixture.work, &["checkout", "-q", "main"]);
    git(&fixture.work, &["merge", "-q", "--ff-only", "feature"]);
    fixture.push(&fixture.carol, "alice", &["main"]).await;
    let merged = fixture.open_pull(&fixture.alice, number).await;
    assert!(merged.pull.is_merged());
    assert_eq!((merged.pull.merge_strategy, merged.pull.merged_by), (None, Some(fixture.carol.id())));
    assert_eq!(merged.issue().state_reason, Some(StateReason::Merged));
    assert!(fixture.events(&fixture.alice, number).await.iter().any(|event| matches!(event, TimelineEvent::Merged { strategy: None, .. })));

    // Closing, then deleting the head branch: reopening is refused until it is pushed again.
    let ctx = fixture.open_pull(&fixture.alice, other).await;
    fixture.services.timeline.set_state(&fixture.alice, &ctx.thread, StateChange::Close(StateReason::Completed)).await.unwrap();
    fixture.push(&fixture.alice, "alice", &["--delete", "other"]).await;
    let closed = fixture.open_pull(&fixture.alice, other).await;
    let error = fixture.services.timeline.set_state(&fixture.alice, &closed.thread, StateChange::Reopen).await.unwrap_err();
    assert!(error.to_string().contains("no longer exists"), "{error}");
    fixture.push(&fixture.alice, "alice", &["other"]).await;
    // A newer open pull request for the same branches blocks reopening the old one.
    let newer = fixture.create(&fixture.alice, "alice", "other").await.unwrap();
    let error = fixture.services.timeline.set_state(&fixture.alice, &closed.thread, StateChange::Reopen).await.unwrap_err();
    assert!(error.to_string().contains(&format!("#{newer}")), "{error}");
}

#[tokio::test]
async fn drafts_requests_and_hidden_repositories() {
    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    let number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    let ctx = fixture.open_pull(&fixture.alice, number).await;
    pulls.set_draft(&fixture.alice, &ctx, true).await.unwrap();
    let draft = fixture.open_pull(&fixture.alice, number).await;
    assert!(draft.pull.is_draft);
    assert!(!pulls.merge_box(Some(&fixture.alice), &draft).await.unwrap().evaluation.can_merge());
    assert!(
        pulls.set_draft(&fixture.bob, &fixture.open_pull(&fixture.bob, number).await, false).await.is_err(),
        "readers cannot mark it ready"
    );
    pulls.set_draft(&fixture.alice, &draft, false).await.unwrap();

    assert!(pulls.request_reviewers(&fixture.alice, &ctx, &["alice".into()]).await.is_err(), "not the author");
    assert!(pulls.request_reviewers(&fixture.alice, &ctx, &["nobody".into()]).await.is_err());
    assert!(
        pulls.request_reviewers(&fixture.bob, &fixture.open_pull(&fixture.bob, number).await, &["carol".into()]).await.is_err(),
        "readers cannot request"
    );
    pulls.request_reviewers(&fixture.alice, &ctx, &["carol".into(), "bob".into()]).await.unwrap();
    let data = pulls.reviews(Some(&fixture.alice), &ctx).await.unwrap();
    assert_eq!(data.requested.len(), 2);
    pulls.submit_review(&fixture.carol, &ctx, ReviewVerdict::Comment, "Looks fine").await.unwrap();
    pulls.remove_reviewers(&fixture.alice, &ctx, &["bob".into()]).await.unwrap();
    assert!(pulls.reviews(Some(&fixture.alice), &ctx).await.unwrap().requested.is_empty(), "submitting cleared carol's request");
    let latest = pulls.reviews(Some(&fixture.alice), &ctx).await.unwrap().latest;
    assert_eq!(
        latest.iter().map(|(account, state)| (account.name.to_string(), *state)).collect::<Vec<_>>(),
        [("carol".to_owned(), ReviewState::Commented)]
    );
    let events = fixture.events(&fixture.alice, number).await;
    assert!(events.contains(&TimelineEvent::ConvertedToDraft) && events.contains(&TimelineEvent::ReadyForReview));
    assert!(events.iter().any(|event| matches!(event, TimelineEvent::ReviewRequested { .. })));
    assert!(events.iter().any(|event| matches!(event, TimelineEvent::ReviewRequestRemoved { .. })));

    // A private repository's pull requests do not exist for outsiders.
    let secret = CreateRepository { name: "secret".into(), description: String::new(), visibility: Visibility::Private };
    fixture.services.repositories.create(&fixture.alice, secret).await.unwrap();
    assert!(matches!(fixture.services.repositories.open(Some(&fixture.bob), "alice", "secret").await, Err(ServiceError::NotFound)));
    let list =
        pulls.list(&fixture.repo(&fixture.alice, "alice").await, &rbit_storage::sqlite::pulls::PullFilter::default(), 1).await.unwrap();
    assert_eq!((list.total, list.open, list.closed, list.items.len()), (1, 1, 0, 1));
}

#[tokio::test]
async fn branch_protection_gates_merges_and_head_branch_deletion() {
    use rbit_core::protection::RuleSettings;
    use rbit_services::protection::ProtectionInput;

    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    let protection = &fixture.services.protection;
    let repo = fixture.repo(&fixture.alice, "alice").await;
    let rule = |pattern: &str, settings: RuleSettings| ProtectionInput {
        pattern: pattern.into(),
        settings,
        users: Vec::new(),
        teams: Vec::new(),
        contexts: Vec::new(),
    };
    let main = RuleSettings { require_pull_request: true, required_approvals: 1, ..RuleSettings::default() };
    protection.create(&fixture.alice, &repo, &rule("main", main)).await.unwrap();
    protection
        .create(&fixture.alice, &repo, &rule("feature", RuleSettings { deny_force_push: false, ..RuleSettings::default() }))
        .await
        .unwrap();

    let number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    pulls.check_unknown().await.unwrap();
    let ctx = fixture.open_pull(&fixture.alice, number).await;
    let merge_box = pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap();
    assert!(merge_box.rules.is_protected() && merge_box.evaluation.is_blocked() && !merge_box.evaluation.can_merge());
    assert!(!merge_box.can_delete_head, "the head branch is protected against deletion");
    let refused = pulls.merge(&fixture.alice, &ctx, Fixture::merge_input(MergeStrategy::Squash)).await.unwrap_err();
    assert!(matches!(&refused, MergeFailure::Blocked(message) if message.contains("approval")), "{refused}");
    let bypass = MergeInput { bypass: true, ..Fixture::merge_input(MergeStrategy::Squash) };
    assert!(
        matches!(pulls.merge(&fixture.alice, &ctx, bypass).await, Err(MergeFailure::Blocked(_))),
        "no bypass without allow_admin_bypass"
    );

    // A reader's approval does not count; a writer's does.
    pulls.submit_review(&fixture.bob, &fixture.open_pull(&fixture.bob, number).await, ReviewVerdict::Approve, "").await.unwrap();
    assert!(!pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap().evaluation.can_merge());
    pulls.submit_review(&fixture.carol, &fixture.open_pull(&fixture.carol, number).await, ReviewVerdict::Approve, "").await.unwrap();
    assert!(pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap().evaluation.can_merge());
    let input = MergeInput { delete_head_branch: true, ..Fixture::merge_input(MergeStrategy::Squash) };
    pulls.merge(&fixture.alice, &ctx, input).await.unwrap();
    assert!(repo.reader.commit_id("refs/heads/feature".into()).await.is_ok(), "the protected head branch is kept");
    let merged = fixture.open_pull(&fixture.alice, number).await;
    assert!(merged.pull.is_merged());
    assert!(pulls.merged_into_default_branch(merged.issue().id).await.unwrap(), "main is the default branch");
    assert!(matches!(pulls.delete_head(&fixture.alice, &merged).await, Err(ServiceError::Invalid(_))));
}

// Milestone 3: required status checks (S).
#[tokio::test]
async fn required_status_checks_gate_merges_and_statuses_wake_the_worker() {
    use rbit_core::{
        protection::RuleSettings,
        pull::{ChecksStatus, Requirement},
        status::StatusState,
    };
    use rbit_services::{protection::ProtectionInput, statuses::NewStatus};

    let fixture = Fixture::new().await;
    let pulls = &fixture.services.pulls;
    let statuses = &fixture.services.statuses;
    let repo = fixture.repo(&fixture.alice, "alice").await;
    let checked = RuleSettings { require_status_checks: true, ..RuleSettings::default() };
    let rule = ProtectionInput { pattern: "main".into(), settings: checked, contexts: vec!["ci".into()], ..ProtectionInput::default() };
    fixture.services.protection.create(&fixture.alice, &repo, &rule).await.unwrap();

    // A pull request from bob's fork: its head is in alice/demo as refs/pull/1/head.
    git(&fixture.work, &["checkout", "-q", "-b", "topic"]);
    commit(&fixture.work, "topic.txt", "topic\n", "Add the topic");
    fixture.push(&fixture.bob, "bob", &["topic"]).await;
    let number = fixture.create(&fixture.bob, "bob", "topic").await.unwrap();
    pulls.check_unknown().await.unwrap();
    let ctx = fixture.open_pull(&fixture.alice, number).await;
    let head = ctx.pull.head_sha.clone();
    let post = |state: &str, context: &str| NewStatus { state: state.into(), context: Some(context.into()), ..NewStatus::default() };

    let merge_box = pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap();
    assert!(merge_box.evaluation.is_blocked() && merge_box.statuses.is_empty());
    let line = |merge_box: &rbit_services::pulls::MergeBox| {
        let check = merge_box.evaluation.checks.iter().find(|c| matches!(c.requirement, Requirement::RequiredChecks(_))).unwrap();
        check.requirement.describe(check.met)
    };
    assert_eq!(line(&merge_box), "Waiting for 1 of 1 required checks");
    let refused = pulls.merge(&fixture.alice, &ctx, Fixture::merge_input(MergeStrategy::Merge)).await.unwrap_err();
    assert!(matches!(&refused, MergeFailure::Blocked(message) if message.contains("required checks")), "{refused}");

    // Statuses on the fork's head are posted to the base repository.
    statuses.create(&fixture.alice, &repo, &head, &post("failure", "ci")).await.unwrap();
    statuses.create(&fixture.alice, &repo, &head, &post("pending", "lint")).await.unwrap();
    let merge_box = pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap();
    assert_eq!(line(&merge_box), "1 of 1 required checks failed");
    let contexts: Vec<_> = merge_box.statuses.iter().map(|s| (s.context.as_str(), s.state)).collect();
    assert_eq!(contexts, [("ci", StatusState::Failure), ("lint", StatusState::Pending)]);
    assert!(merge_box.evaluation.checks.iter().any(|c| c.requirement == Requirement::Checks(ChecksStatus::Failing)));

    // A status for the head re-evaluates the pull request: its mergeability is current,
    // so nothing is queued; once it is stale, it is.
    assert_eq!(pulls.handle_status(repo.repo.repo.id, &head).await.unwrap(), 0);
    assert_eq!(pulls.handle_status(repo.repo.repo.id, &"0".repeat(40)).await.unwrap(), 0, "no pull request has that head");
    git(&fixture.work, &["checkout", "-q", "main"]);
    commit(&fixture.work, "base.txt", "base\n", "Move the base");
    // The base moved outside the push path: the stored mergeability is stale now.
    git(&fixture.work, &["push", "-q", repo.disk_path.to_str().unwrap(), "main"]);
    assert_eq!(pulls.handle_status(repo.repo.repo.id, &head).await.unwrap(), 1);
    pulls.check_unknown().await.unwrap();

    // Success on the required context (the newest report wins) unblocks the merge; the
    // pending, unrequired one does not block.
    statuses.create(&fixture.alice, &repo, &head, &post("success", "ci")).await.unwrap();
    let ctx = fixture.open_pull(&fixture.alice, number).await;
    let merge_box = pulls.merge_box(Some(&fixture.alice), &ctx).await.unwrap();
    assert_eq!(line(&merge_box), "1 required check passed");
    assert!(merge_box.evaluation.can_merge(), "{:?}", merge_box.evaluation);
    pulls.merge(&fixture.alice, &ctx, Fixture::merge_input(MergeStrategy::Merge)).await.unwrap();
    assert!(fixture.open_pull(&fixture.alice, number).await.pull.is_merged());
}
