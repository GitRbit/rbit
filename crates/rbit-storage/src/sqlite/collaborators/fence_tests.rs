//! Four direct assignment APIs only. Trigger/cascade compatibility is not enrollment.
use super::*;
use crate::{Database, NewRepository, NewUser, RepositoryStore};
use rbit_core::Visibility;
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::{Instant, timeout};

const WAIT: Duration = Duration::from_secs(5);
struct Fixture {
    _directory: tempfile::TempDir,
    db: Database,
    alice: AccountId,
    bob: AccountId,
    carol: AccountId,
    repo: RepoId,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("sql.db")).await.unwrap();
        let mut users = Vec::new();
        for name in ["alice", "bob", "carol"] {
            users.push(
                db.accounts()
                    .create_user(NewUser {
                        name: name.parse().unwrap(),
                        email: format!("{name}@example.com").parse().unwrap(),
                        display_name: None,
                        password_hash: "fixture".into(),
                        is_admin: true,
                    })
                    .await
                    .unwrap()
                    .id(),
            );
        }
        let (alice, bob, carol) = (users[0], users[1], users[2]);
        let repo = db
            .repositories()
            .create(NewRepository {
                owner_id: alice,
                name: "private".parse().unwrap(),
                description: None,
                visibility: Visibility::Private,
                default_branch: "main".into(),
            })
            .await
            .unwrap()
            .id;
        db.collaborators().add(repo, bob, Permission::Read, alice).await.unwrap();
        Self { _directory: directory, db, alice, bob, carol, repo }
    }
    async fn provenance(&self, user: AccountId) -> (Option<i64>, i64) {
        sqlx::query_as("SELECT added_by, created_at FROM collaborators WHERE repo_id = ? AND user_id = ?")
            .bind(self.repo.0)
            .bind(user.0)
            .fetch_one(&self.db.pool)
            .await
            .unwrap()
    }
}
#[derive(Debug, Clone, Copy)]
enum Operation {
    Add,
    Set,
    Remove,
    Grant,
}
const OPERATIONS: [Operation; 4] = [Operation::Add, Operation::Set, Operation::Remove, Operation::Grant];
async fn apply(f: &Fixture, collaborators: &CollaboratorStore, repositories: &RepositoryStore, op: Operation) -> Result<(), StorageError> {
    match op {
        Operation::Add => collaborators.add(f.repo, f.carol, Permission::Write, f.alice).await,
        Operation::Set => collaborators.set_permission(f.repo, f.bob, Permission::Admin).await.map(|_| ()),
        Operation::Remove => collaborators.remove(f.repo, f.bob).await.map(|_| ()),
        Operation::Grant => repositories.grant_collaborator(f.repo, f.bob, Permission::Write).await,
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn acknowledged_results_preserve_permission_bools_and_grant_provenance() {
    let f = Fixture::new().await;
    let collaborators = f.db.collaborators();
    // Labeled fixture mutation makes timestamp preservation independent of wall-clock
    // resolution. It is not a supported unguarded application mutation path.
    sqlx::query("UPDATE collaborators SET created_at = 123 WHERE repo_id = ? AND user_id = ?")
        .bind(f.repo.0)
        .bind(f.bob.0)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let original = f.provenance(f.bob).await;
    assert_eq!(original, (Some(f.alice.0), 123));
    assert!(collaborators.set_permission(f.repo, f.bob, Permission::Admin).await.unwrap());
    assert!(collaborators.set_permission(f.repo, f.bob, Permission::Admin).await.unwrap());
    f.db.repositories().grant_collaborator(f.repo, f.bob, Permission::Write).await.unwrap();
    assert_eq!(f.provenance(f.bob).await, original);
    assert_eq!(f.db.repositories().grants(f.repo, f.alice, f.bob).await.unwrap().collaborator, Some(Permission::Write));
    f.db.repositories().grant_collaborator(f.repo, f.carol, Permission::Read).await.unwrap();
    assert_eq!(f.provenance(f.carol).await.0, Some(f.carol.0));
    assert!(collaborators.remove(f.repo, f.bob).await.unwrap());
    assert!(!collaborators.remove(f.repo, f.bob).await.unwrap());
    assert!(!collaborators.set_permission(f.repo, f.bob, Permission::Read).await.unwrap());
    assert_eq!(f.db.repositories().grants(f.repo, f.alice, f.bob).await.unwrap().collaborator, None);
    let quiet = timeout(WAIT, f.db.authority_domain.drain(Instant::now() + WAIT)).await.unwrap().unwrap();
    drop(quiet);
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn all_four_share_credential_queue_but_reads_do_not_acquire_serial() {
    let f = Fixture::new().await;
    let collaborators = f.db.collaborators();
    let repositories = f.db.repositories();
    let profiles = f.db.profiles();
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut credential = Box::pin(profiles.set_password_hash(f.alice, "changed"));
    pending(credential.as_mut()).await;
    let mut add = Box::pin(apply(&f, &collaborators, &repositories, Operation::Add));
    let mut set = Box::pin(apply(&f, &collaborators, &repositories, Operation::Set));
    let mut remove = Box::pin(apply(&f, &collaborators, &repositories, Operation::Remove));
    let mut grant = Box::pin(apply(&f, &collaborators, &repositories, Operation::Grant));
    pending(add.as_mut()).await;
    pending(set.as_mut()).await;
    pending(remove.as_mut()).await;
    pending(grant.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 5);
    assert_eq!(timeout(WAIT, collaborators.permission(f.repo, f.bob)).await.unwrap().unwrap(), Some(Permission::Read));
    assert_eq!(timeout(WAIT, repositories.grants(f.repo, f.alice, f.bob)).await.unwrap().unwrap().collaborator, Some(Permission::Read));
    drop(credential);
    drop(set); // Before acquisition, neither cancellation can have submitted SQL.
    assert_eq!(f.db.authority_domain.snapshot().queued, 3);
    drop(quiet);
    timeout(WAIT, add).await.unwrap().unwrap();
    timeout(WAIT, remove).await.unwrap().unwrap();
    timeout(WAIT, grant).await.unwrap().unwrap();
    assert!(!f.db.authority_domain.snapshot().uncertain);
    assert_eq!(collaborators.permission(f.repo, f.carol).await.unwrap(), Some(Permission::Write));
    assert_eq!(collaborators.permission(f.repo, f.bob).await.unwrap(), Some(Permission::Write));
}

#[tokio::test]
async fn cloned_and_new_handles_reject_all_four_after_unpolled_close() {
    let f = Fixture::new().await;
    let database = f.db.clone();
    let collaborators = f.db.collaborators().clone();
    let repositories = f.db.repositories().clone();
    drop(database.close()); // Stop executes synchronously, not on future polling.
    for fresh in [false, true] {
        let c = if fresh { database.collaborators() } else { collaborators.clone() };
        let r = if fresh { database.repositories() } else { repositories.clone() };
        for op in OPERATIONS {
            assert!(matches!(
                apply(&f, &c, &r, op).await,
                Err(StorageError::AdmissionUnavailable { scope: "collaborator", reason: crate::AdmissionReason::Stopped })
            ));
        }
    }
    assert_eq!(f.provenance(f.bob).await.0, Some(f.alice.0));
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn acquired_loss_for_each_entry_is_unknown_while_sql_writer_lock_is_held() {
    for op in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let collaborators = f.db.collaborators();
        let repositories = f.db.repositories();
        let mut operation = Box::pin(apply(&f, &collaborators, &repositories, op));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1, "registered before blocked SQL await, not worker entry proof");
        drop(operation);
        assert!(f.db.authority_domain.snapshot().uncertain);
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}

#[tokio::test]
async fn duplicate_and_foreign_key_errors_stick_but_later_passive_grants_proceed() {
    for duplicate in [false, true] {
        let f = Fixture::new().await;
        let result = if duplicate {
            f.db.collaborators().add(f.repo, f.bob, Permission::Admin, f.alice).await
        } else {
            f.db.repositories().grant_collaborator(f.repo, AccountId(i64::MAX), Permission::Admin).await
        };
        if duplicate {
            assert!(matches!(result, Err(StorageError::Conflict("collaborator"))));
        } else {
            assert!(result.is_err());
        }
        assert_eq!(f.db.collaborators().permission(f.repo, f.bob).await.unwrap(), Some(Permission::Read));
        assert!(f.db.authority_domain.snapshot().uncertain);
        f.db.repositories().grant_collaborator(f.repo, f.carol, Permission::Admin).await.unwrap();
        assert_eq!(f.db.collaborators().permission(f.repo, f.carol).await.unwrap(), Some(Permission::Admin));
        assert!(f.db.authority_domain.snapshot().uncertain);
    }
}
