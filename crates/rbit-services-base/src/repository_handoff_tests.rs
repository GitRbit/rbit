//! Concrete Storage/Git handoffs only; actual Service/HTTP callers remain unadopted.
use rbit_core::{AccountId, Visibility};
use rbit_git::{RepositoryStorage, owned_edit::OwnedGitDrainError};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::repository_commands::{
        AdmittedRepositoryCommand, RepositoryCommand, RepositoryCommandDrainError, RepositoryFlowOwner, RepositoryLogicalSettlement,
        RepositoryPhaseResult, RepositoryPlan, RepositoryReservation, RepositorySqlBoundary,
    },
};
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, timeout};

const WAIT: Duration = Duration::from_secs(2);

// The actual task's eligibility owner MUST stay outside generic K: Git quarantine
// retains K. Keeping the owner in K would hide task loss from Storage's drain.
#[derive(Debug)]
struct SqlKeepalive {
    reservation: Option<RepositoryReservation>,
    command: Option<RepositoryCommand>,
    witness: Arc<()>,
}

async fn fixture() -> (tempfile::TempDir, Database, AccountId) {
    let directory = tempfile::tempdir().unwrap();
    let db = Database::open(&directory.path().join("db.sqlite")).await.unwrap();
    let user = db
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "fixture".into(),
            is_admin: true,
        })
        .await
        .unwrap();
    db.repositories().create(new_repository(user.id())).await.unwrap();
    (directory, db, user.id())
}

fn new_repository(owner_id: AccountId) -> NewRepository {
    NewRepository {
        owner_id,
        name: "existing".parse().unwrap(),
        description: Some("unchanged".into()),
        visibility: Visibility::Private,
        default_branch: "main".into(),
    }
}

async fn duplicate_rollback(
    db: &Database,
    storage: &RepositoryStorage,
    owner_id: AccountId,
) -> (rbit_git::owned_edit::RepositoryEdit<SqlKeepalive>, RepositoryFlowOwner) {
    let reservation = db.repository_commands().reserve(RepositoryPlan::Create(new_repository(owner_id))).unwrap();
    let mut edit = storage
        .reserve_edit()
        .unwrap()
        .attach(SqlKeepalive { reservation: Some(reservation), command: None, witness: Arc::new(()) })
        .unwrap();
    // Attach before first SQL. Move ONLY the command into K synchronously after
    // acquire; the owner remains on this actual task's stack, outside retained K.
    let reservation = edit.context_mut().reservation.take().unwrap();
    let AdmittedRepositoryCommand { command, owner } = reservation.acquire().await.unwrap();
    edit.context_mut().command = Some(command);
    let command = edit.context_mut().command.as_mut().unwrap();
    let RepositoryPhaseResult::Settled(report) = command.execute_next().await else {
        panic!("duplicate create must have an explicit settled boundary")
    };
    assert_eq!(report.boundary, RepositorySqlBoundary::RolledBack);
    assert!(matches!(report.result, Err(StorageError::Conflict("repository name"))));
    assert_eq!(report.repository_id, None);
    assert_eq!(command.repository_id(), None);
    (edit, owner)
}

#[tokio::test]
async fn duplicate_sql_rollback_returns_unbound_command_and_reuses_both_credits() {
    let (directory, db, user) = fixture().await;
    let root = directory.path().join("never-created");
    let storage = RepositoryStorage::new(&root);
    for _ in 0..4 {
        let (mut edit, owner) = duplicate_rollback(&db, &storage, user).await;
        let witness = Arc::downgrade(&edit.context_mut().witness);
        // No dummy RepoId and no pre-effect cancellation after the real SQL attempt.
        let mut returned = edit.finish_without_backend().unwrap();
        assert!(witness.upgrade().is_some());
        assert!(returned.reservation.is_none());
        let command = returned.command.take().unwrap();
        assert_eq!(command.repository_id(), None);
        let completion = timeout(WAIT, command.finish_failed_backend(owner)).await.unwrap().unwrap();
        assert_eq!(completion.logical, RepositoryLogicalSettlement::FailureRecorded);
        assert_eq!(completion.progress.last_forward_boundary, Some(RepositorySqlBoundary::RolledBack));
        assert_eq!(completion.progress.committed_forward_steps, 0);
        drop(returned);
        assert!(witness.upgrade().is_none());
        assert!(!root.exists());
    }
    let existing = db.repositories().find_by_path("alice", "existing").await.unwrap().unwrap().repo;
    assert_eq!(existing.description.as_deref(), Some("unchanged"));
    storage.prepare_owned_shutdown();
    storage.drain_owned(Instant::now() + WAIT).await.unwrap();
    // Credits are reusable while the failed logical outcome remains sticky.
    assert_eq!(db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::FailureRecorded));
    db.close().await;
}

#[tokio::test]
async fn git_retained_sql_command_cannot_hide_separate_storage_owner_loss() {
    let (directory, db, user) = fixture().await;
    let root = directory.path().join("never-created");
    let storage = RepositoryStorage::new(&root);
    let (mut edit, owner) = duplicate_rollback(&db, &storage, user).await;
    let witness = Arc::downgrade(&edit.context_mut().witness);
    let retained = edit.quarantine();
    assert!(witness.upgrade().is_some());
    // Model actual task loss while Git still retains the recoverable command.
    drop(owner);
    assert_eq!(
        timeout(WAIT, db.drain_repository_commands(Instant::now() + WAIT)).await.unwrap(),
        Err(RepositoryCommandDrainError::Unconfirmed)
    );
    storage.prepare_owned_shutdown();
    assert_eq!(storage.drain_owned(Instant::now() + WAIT).await, Err(OwnedGitDrainError::Unconfirmed));
    drop(retained);
    drop(storage);
    assert!(witness.upgrade().is_some(), "restart-only Git quarantine still retains K");
    assert!(!root.exists());
    // Unknown is logical retention, never proof that all SQL/FS workers ended.
    db.close().await;
}
