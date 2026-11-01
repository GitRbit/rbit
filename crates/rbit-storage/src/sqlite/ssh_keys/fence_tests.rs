//! Four direct key writes only; cascades and service resource limits are separate.
use super::*;
use crate::{Database, NewRepository, NewUser};
use rbit_authority_fence::{DrainError, Snapshot};
use rbit_core::Visibility;
use sqlx::{
    Connection as _,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::Instant;

struct Fixture {
    dir: tempfile::TempDir,
    db: Database,
    alice: AccountId,
    bob: AccountId,
    project: RepoId,
    other: RepoId,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&db, "alice", true).await;
        let bob = user(&db, "bob", false).await;
        let project = repo(&db, alice, "project").await;
        let other = repo(&db, bob, "other").await;
        Self { dir, db, alice, bob, project, other }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db"))).await.unwrap()
    }
}
async fn user(db: &Database, name: &str, admin: bool) -> AccountId {
    db.accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: admin,
        })
        .await
        .unwrap()
        .id()
}
async fn repo(db: &Database, owner: AccountId, name: &str) -> RepoId {
    db.repositories()
        .create(NewRepository {
            owner_id: owner,
            name: name.parse().unwrap(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "main".into(),
        })
        .await
        .unwrap()
        .id
}
fn key(fingerprint: &str) -> NewPublicKey {
    NewPublicKey {
        title: "fixture".into(),
        public_key: "ssh-ed25519 fixture".into(),
        algorithm: "ssh-ed25519".into(),
        fingerprint: fingerprint.into(),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
fn stopped(result: &Result<(), StorageError>) {
    assert!(
        matches!(result, Err(StorageError::AdmissionUnavailable { scope: "SSH key", reason: crate::AdmissionReason::Stopped })),
        "{result:?}"
    );
}
async fn mutation(store: &SshKeyStore, f: &Fixture, user_id: SshKeyId, deploy_id: DeployKeyId, kind: usize) -> Result<(), StorageError> {
    match kind {
        0 => store.add_user_key(f.bob, &key("SHA256:new-user")).await.map(|_| ()),
        1 => store.delete_user_key(f.alice, user_id).await.map(|_| ()),
        2 => store.add_deploy_key(f.other, &key("SHA256:new-deploy"), true).await.map(|_| ()),
        _ => store.delete_deploy_key(f.project, deploy_id).await.map(|_| ()),
    }
}

#[tokio::test]
async fn user_and_deploy_success_and_owner_filtered_deletes_settle_cleanly() {
    let f = Fixture::new().await;
    let store = f.db.ssh_keys();
    let personal = store.add_user_key(f.alice, &key("SHA256:user")).await.unwrap();
    assert_eq!(personal.account_id, f.alice);
    let read = store.add_deploy_key(f.project, &key("SHA256:read"), true).await.unwrap();
    let write = store.add_deploy_key(f.project, &key("SHA256:write"), false).await.unwrap();
    assert!(read.read_only);
    assert!(!write.read_only);
    assert!(matches!(store.find_by_fingerprint("SHA256:write").await.unwrap(), Some(KeyHolder::Deploy { key, repo })
        if !key.read_only && repo.repo.id == f.project));
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(!store.delete_user_key(f.bob, personal.id).await.unwrap());
    assert!(!store.delete_deploy_key(f.other, read.id).await.unwrap());
    assert_eq!(store.count_user_keys(f.alice).await.unwrap(), 1);
    assert_eq!(store.count_deploy_keys(f.project).await.unwrap(), 2);
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(store.delete_user_key(f.alice, personal.id).await.unwrap());
    assert!(!store.delete_user_key(f.alice, personal.id).await.unwrap());
    assert!(store.delete_deploy_key(f.project, read.id).await.unwrap());
    assert!(!store.delete_deploy_key(f.project, read.id).await.unwrap());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn same_table_and_both_cross_table_collisions_preserve_original_bindings() {
    let f = Fixture::new().await;
    let store = f.db.ssh_keys();
    let personal = store.add_user_key(f.alice, &key("SHA256:user")).await.unwrap();
    let deploy = store.add_deploy_key(f.project, &key("SHA256:deploy"), true).await.unwrap();
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(matches!(store.add_user_key(f.bob, &key("SHA256:user")).await, Err(StorageError::Conflict(KEY_FIELD))));
    assert!(matches!(store.add_deploy_key(f.other, &key("SHA256:deploy"), false).await, Err(StorageError::Conflict(KEY_FIELD))));
    assert!(matches!(store.add_deploy_key(f.other, &key("SHA256:user"), false).await, Err(StorageError::Conflict(KEY_FIELD))));
    assert!(matches!(store.add_user_key(f.bob, &key("SHA256:deploy")).await, Err(StorageError::Conflict(KEY_FIELD))));
    assert_eq!(store.list_user_keys(f.alice).await.unwrap(), [personal]);
    assert_eq!(store.list_deploy_keys(f.project).await.unwrap(), [deploy]);
    assert_eq!(store.list_user_keys(f.bob).await.unwrap(), Vec::<SshKey>::new());
    assert_eq!(store.list_deploy_keys(f.other).await.unwrap(), Vec::<DeployKey>::new());
    assert!(f.db.authority_domain.snapshot().uncertain);
    store.add_user_key(f.bob, &key("SHA256:later-user")).await.unwrap();
    store.add_deploy_key(f.other, &key("SHA256:later-deploy"), false).await.unwrap();
    assert!(matches!(f.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    f.db.close().await;
}

#[tokio::test]
async fn all_four_key_paths_share_policy_queue_and_terminal_cloned_handle_stop() {
    let f = Fixture::new().await;
    let clone = f.db.clone();
    let store = clone.ssh_keys().clone();
    let user_id = store.add_user_key(f.alice, &key("SHA256:user")).await.unwrap().id;
    let deploy_id = store.add_deploy_key(f.project, &key("SHA256:deploy"), true).await.unwrap().id;
    let policy = f.db.plugin_policy();
    let quiet = f.db.authority_domain.drain(deadline()).await.unwrap();
    let mut operations: Vec<_> = (0..4).map(|kind| Box::pin(mutation(&store, &f, user_id, deploy_id, kind))).collect();
    for operation in &mut operations {
        pending(operation.as_mut()).await;
    }
    let mut review = Box::pin(policy.prepare(f.alice, Timestamp(1)));
    pending(review.as_mut()).await;
    assert_eq!(clone.authority_domain.snapshot(), Snapshot { active: 0, queued: 5, uncertain: false, writers_stopped: false });
    assert_eq!(store.count_user_keys(f.alice).await.unwrap(), 1);
    drop(operations.pop().unwrap());
    assert_eq!(f.db.authority_domain.snapshot().queued, 4);
    assert_eq!(store.count_deploy_keys(f.project).await.unwrap(), 1);
    drop(clone.close());
    drop(f.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    for operation in operations {
        stopped(&operation.await);
    }
    assert!(matches!(
        review.await,
        Err(StorageError::AdmissionUnavailable { scope: "package policy", reason: crate::AdmissionReason::Stopped })
    ));
    for kind in 0..4 {
        stopped(&mutation(&store, &f, user_id, deploy_id, kind).await);
        stopped(&mutation(&f.db.ssh_keys(), &f, user_id, deploy_id, kind).await);
    }
    assert_eq!(store.count_user_keys(f.alice).await.unwrap(), 1);
    assert_eq!(store.count_deploy_keys(f.project).await.unwrap(), 1);
    assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    f.db.close().await;
}

#[tokio::test]
async fn each_acquired_key_mutation_cancel_is_uncertain_with_real_sql_writer_held() {
    for kind in 0..4 {
        let f = Fixture::new().await;
        let store = f.db.ssh_keys();
        let user_id = store.add_user_key(f.alice, &key("SHA256:user")).await.unwrap().id;
        let deploy_id = store.add_deploy_key(f.project, &key("SHA256:deploy"), true).await.unwrap().id;
        let mut connection = f.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation = Box::pin(mutation(&store, &f, user_id, deploy_id, kind));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1);
        drop(operation);
        assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(f.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // Enrollment before the first SQL await; not worker entry or proof of
        // any canceled driver's commit/rollback settlement.
        f.db.close().await;
    }
}
