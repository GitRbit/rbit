//! Import records and imported history against a real, migrated SQLite database.

use rbit_core::{
    AccountId, IssueKind, IssueNumber, IssueState, IssueText, IssueTitle, RepoId, StateReason, TimelineEvent, Timestamp, Visibility,
    import::{ImportCounts, ImportOptions, ImportPhase, ImportStatus, SourceKind},
};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::{
        imports::{ImportedPull, NewImport},
        issues::{ImportedClosing, ImportedComment, ImportedEvent, ImportedThread, NewIssue},
    },
};

async fn setup() -> (tempfile::TempDir, Database, AccountId, RepoId) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap();
    let repo = database
        .repositories()
        .create(NewRepository {
            owner_id: user.id(),
            name: "imported".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap();
    (dir, database, user.id(), repo.id)
}

fn thread(repo: RepoId, author: AccountId, number: u32, kind: IssueKind) -> ImportedThread {
    ImportedThread {
        repo,
        number: IssueNumber::new(number).unwrap(),
        kind,
        title: IssueTitle::parse(&format!("Thread {number}")).unwrap(),
        body: IssueText::body("Body").unwrap(),
        author,
        created_at: Timestamp(1_000),
        updated_at: Timestamp(5_000),
        closed: None,
        locked: false,
        comments: Vec::new(),
        events: Vec::new(),
    }
}

#[tokio::test]
async fn an_import_record_moves_from_queued_to_finished() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.imports();
    let options = ImportOptions { lfs: false, ..ImportOptions::default() };
    let new = NewImport { repo, importer: alice, kind: SourceKind::Gitea, source_url: "https://codeberg.org/a/b", options };
    store.create(&new).await.unwrap();
    let queued = store.find(repo).await.unwrap().unwrap();
    assert_eq!(
        (queued.status, queued.phase, queued.options, queued.importer),
        (ImportStatus::Queued, ImportPhase::Repository, options, Some(alice))
    );

    store.start(repo, Timestamp(10)).await.unwrap();
    let counts = ImportCounts { issues: 3, comments: 7, ..ImportCounts::default() };
    store.save_progress(repo, ImportPhase::Issues, &counts, &["release v1: too large".to_owned()]).await.unwrap();
    let running = store.find(repo).await.unwrap().unwrap();
    assert_eq!(
        (running.status, running.phase, running.counts, running.started_at),
        (ImportStatus::Running, ImportPhase::Issues, counts, Some(Timestamp(10)))
    );
    assert_eq!(running.warnings, ["release v1: too large"]);

    store.finish(repo, Timestamp(20), None).await.unwrap();
    let done = store.find(repo).await.unwrap().unwrap();
    assert_eq!(
        (done.status, done.phase, done.error, done.finished_at),
        (ImportStatus::Succeeded, ImportPhase::Finished, None, Some(Timestamp(20)))
    );

    // A second import of the same repository replaces the record; a restart fails it.
    store.create(&new).await.unwrap();
    assert_eq!(store.fail_unfinished(Timestamp(30), "interrupted").await.unwrap(), 1);
    let failed = store.find(repo).await.unwrap().unwrap();
    assert_eq!(
        (failed.status, failed.error.as_deref(), failed.phase),
        (ImportStatus::Failed, Some("interrupted"), ImportPhase::Repository)
    );
    assert_eq!(store.fail_unfinished(Timestamp(40), "again").await.unwrap(), 0, "finished imports stay as they are");
}

#[tokio::test]
async fn imported_threads_keep_their_numbers_states_and_times() {
    let (_dir, database, alice, repo) = setup().await;
    let issues = database.issues();
    issues.reserve_numbers(repo, IssueNumber::new(7).unwrap()).await.unwrap();

    let mut closed = thread(repo, alice, 3, IssueKind::Issue);
    closed.closed = Some(ImportedClosing { at: Timestamp(4_000), reason: StateReason::NotPlanned, by: None });
    closed.comments = vec![
        ImportedComment {
            author: alice,
            body: IssueText::comment("First").unwrap(),
            created_at: Timestamp(2_000),
            updated_at: Timestamp(2_000),
        },
        ImportedComment {
            author: alice,
            body: IssueText::comment("Second").unwrap(),
            created_at: Timestamp(3_000),
            updated_at: Timestamp(3_500),
        },
    ];
    closed.events = vec![ImportedEvent {
        actor: None,
        event: TimelineEvent::Closed { reason: StateReason::NotPlanned, commit: None, pull: None },
        at: Timestamp(4_000),
    }];
    let issue = issues.insert_imported(&closed).await.unwrap();
    assert_eq!(issue.number.get(), 3);
    assert_eq!(
        (issue.state, issue.state_reason, issue.closed_at),
        (IssueState::Closed, Some(StateReason::NotPlanned), Some(Timestamp(4_000)))
    );
    assert_eq!((issue.created_at, issue.updated_at, issue.comment_count), (Timestamp(1_000), Timestamp(5_000), 2));
    let timeline = issues.timeline(issue.id, None, 10).await.unwrap();
    assert_eq!(timeline.items.len(), 3, "two comments and the closing event, in their original order");

    let taken = issues.insert_imported(&thread(repo, alice, 3, IssueKind::Issue)).await;
    assert!(matches!(taken, Err(StorageError::Conflict("issue number"))));

    // New threads are numbered after everything reserved or imported.
    let new = NewIssue {
        repo,
        kind: IssueKind::Issue,
        title: IssueTitle::parse("Fresh").unwrap(),
        body: IssueText::default(),
        author: alice,
        at: Timestamp::now(),
    };
    assert_eq!(issues.create(&new).await.unwrap().number.get(), 8);
    issues.insert_imported(&thread(repo, alice, 20, IssueKind::Issue)).await.unwrap();
    assert_eq!(issues.create(&new).await.unwrap().number.get(), 21);
}

#[tokio::test]
async fn imported_pull_requests_are_merged_history_or_open() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.imports();
    let sha = "a".repeat(40);

    let mut merged = thread(repo, alice, 1, IssueKind::Pull);
    merged.closed = Some(ImportedClosing { at: Timestamp(4_000), reason: StateReason::Merged, by: None });
    let pull = ImportedPull {
        head_repo: None,
        head_branch: "feature".to_owned(),
        base_branch: "main".to_owned(),
        head_sha: sha.clone(),
        is_draft: false,
        merged: Some((Timestamp(4_000), Some("b".repeat(40)))),
        merged_by: None,
    };
    let issue = store.insert_pull(&merged, &pull).await.unwrap();
    let row = database.pulls().find(issue.id).await.unwrap().unwrap();
    assert!(row.is_merged());
    assert_eq!(
        (row.merge_strategy, row.merge_commit_sha.as_deref(), row.merged_at),
        (None, Some("b".repeat(40).as_str()), Some(Timestamp(4_000)))
    );
    assert_eq!(row.head_repo_id, None);

    let open = ImportedPull { head_repo: Some(repo), merged: None, ..pull };
    let issue = store.insert_pull(&thread(repo, alice, 2, IssueKind::Pull), &open).await.unwrap();
    let row = database.pulls().find(issue.id).await.unwrap().unwrap();
    assert!(!row.is_merged() && row.head_repo_id == Some(repo));
    assert_eq!(database.issues().open_counts(repo).await.unwrap(), (0, 1));
}

#[tokio::test]
async fn authors_are_found_by_verified_email_or_name() {
    let (_dir, database, alice, _repo) = setup().await;
    let store = database.imports();
    assert_eq!(store.user_with_verified_email("alice@example.com").await.unwrap(), None, "unverified addresses never match");
    database.recovery().mark_verified(alice, &"alice@example.com".parse().unwrap(), Timestamp(1)).await.unwrap();
    assert_eq!(store.user_with_verified_email("ALICE@example.com").await.unwrap(), Some(alice));
    assert_eq!(store.user_named("Alice").await.unwrap(), Some(alice));
    assert_eq!(store.user_named("nobody").await.unwrap(), None);
}
