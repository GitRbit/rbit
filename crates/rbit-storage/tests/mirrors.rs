//! Pull mirror scheduling state against a real, migrated SQLite database.

use rbit_core::{RepoId, Timestamp, Visibility};
use rbit_storage::{Database, NewRepository, NewUser};

async fn setup() -> (tempfile::TempDir, Database, RepoId) {
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
            name: "mirror".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap();
    (dir, database, repo.id)
}

#[tokio::test]
async fn mirrors_are_due_immediately_then_after_each_interval() {
    let (_dir, database, repo) = setup().await;
    let store = database.mirrors();
    assert!(!store.is_mirror(repo).await.unwrap());
    store.create(repo, "https://example.com/x.git", 10).await.unwrap();
    assert!(store.is_mirror(repo).await.unwrap());

    let now = Timestamp::now();
    assert_eq!(store.due(now, 10).await.unwrap().len(), 1, "due right away");

    store.record_sync(repo, now, Some("boom")).await.unwrap();
    let mirror = store.find(repo).await.unwrap().unwrap();
    assert_eq!((mirror.last_sync_at, mirror.last_error.as_deref()), (Some(now), Some("boom")));
    assert_eq!(mirror.next_sync_at, Timestamp(now.seconds() + 600));
    assert_eq!(store.due(now, 10).await.unwrap().len(), 0, "not due before the interval passed");
    assert_eq!(store.due(Timestamp(now.seconds() + 600), 10).await.unwrap().len(), 1);

    store.set_interval(repo, 60).await.unwrap();
    assert_eq!(store.find(repo).await.unwrap().unwrap().next_sync_at, Timestamp(now.seconds() + 3600));
    store.schedule_now(repo).await.unwrap();
    assert_eq!(store.due(Timestamp::now(), 10).await.unwrap().len(), 1);

    store.record_sync(repo, now, None).await.unwrap();
    assert_eq!(store.find(repo).await.unwrap().unwrap().last_error, None);
    store.delete(repo).await.unwrap();
    assert!(store.find(repo).await.unwrap().is_none());
}
