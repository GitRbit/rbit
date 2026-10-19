//! Push mirror settings, write-only secrets and scheduling against a real, migrated
//! `SQLite` database.

use rbit_core::{AccountId, RepoId, Timestamp, Visibility};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::push_mirrors::{NewPushMirror, PushMirrorChanges},
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
            name: "rocket".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap();
    (dir, database, user.id(), repo.id)
}

fn new_mirror(repo: RepoId, by: AccountId, url: &str) -> NewPushMirror<'_> {
    NewPushMirror { repo, url, username: "bot", secret: Some("s3cret"), sync_on_push: true, created_by: by }
}

#[tokio::test]
async fn secrets_are_write_only_and_kept_unless_replaced() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.push_mirrors();
    let mirror = store.create(&new_mirror(repo, alice, "https://example.com/a.git")).await.unwrap();
    assert!(mirror.has_secret && mirror.enabled && mirror.sync_on_push);
    assert_eq!((mirror.username.as_str(), mirror.created_by), ("bot", Some(alice)));
    assert!(matches!(store.create(&new_mirror(repo, alice, "https://example.com/a.git")).await, Err(StorageError::Conflict(_))));
    assert_eq!(store.count(repo).await.unwrap(), 1);

    let target = store.target(mirror.id).await.unwrap().unwrap();
    assert_eq!(target.secret.as_deref(), Some("s3cret"));

    let changes =
        PushMirrorChanges { url: "https://example.com/b.git", username: "other", secret: None, enabled: false, sync_on_push: false };
    assert!(store.update(repo, mirror.id, &changes).await.unwrap());
    let target = store.target(mirror.id).await.unwrap().unwrap();
    assert_eq!(target.secret.as_deref(), Some("s3cret"), "a blank secret keeps the stored one");
    assert_eq!((target.mirror.url.as_str(), target.mirror.enabled), ("https://example.com/b.git", false));
    let replaced = PushMirrorChanges { secret: Some("new"), ..changes };
    store.update(repo, mirror.id, &replaced).await.unwrap();
    assert_eq!(store.target(mirror.id).await.unwrap().unwrap().secret.as_deref(), Some("new"));

    assert!(!store.update(RepoId(repo.0 + 1), mirror.id, &replaced).await.unwrap(), "only through its own repository");
    assert!(!store.delete(RepoId(repo.0 + 1), mirror.id).await.unwrap());
    assert!(store.delete(repo, mirror.id).await.unwrap());
    assert_eq!(store.list(repo).await.unwrap().len(), 0);
}

#[tokio::test]
async fn pushes_schedule_enabled_mirrors_and_attempts_are_recorded() {
    let (_dir, database, alice, repo) = setup().await;
    let store = database.push_mirrors();
    let on_push = store.create(&new_mirror(repo, alice, "https://example.com/a.git")).await.unwrap();
    let interval_only =
        store.create(&NewPushMirror { sync_on_push: false, ..new_mirror(repo, alice, "https://example.com/b.git") }).await.unwrap();
    let now = Timestamp::now();
    assert_eq!(store.due(now, 10).await.unwrap().len(), 2, "new mirrors are due at once");

    let later = Timestamp(now.seconds() + 3_600);
    for mirror in [on_push.id, interval_only.id] {
        store.claim(mirror, later).await.unwrap();
        store.record_attempt(mirror, now, None).await.unwrap();
    }
    assert_eq!(store.due(now, 10).await.unwrap().len(), 0);
    assert_eq!(store.next_due().await.unwrap(), Some(later));
    let found = store.find(repo, on_push.id).await.unwrap().unwrap();
    assert_eq!((found.last_attempt_at, found.last_success_at, found.last_error), (Some(now), Some(now), None));

    let soon = Timestamp(now.seconds() + 10);
    assert_eq!(store.schedule_after_push(repo, soon).await.unwrap(), 1, "only mirrors that sync on push");
    assert_eq!(store.schedule_after_push(repo, Timestamp(now.seconds() + 20)).await.unwrap(), 1);
    let due: Vec<_> = store.due(soon, 10).await.unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(due, [on_push.id], "a burst of pushes keeps the earliest time");

    // A push during an attempt is not absorbed by it.
    store.claim(on_push.id, later).await.unwrap();
    store.schedule_after_push(repo, soon).await.unwrap();
    store.record_attempt(on_push.id, now, Some("rejected")).await.unwrap();
    let found = store.find(repo, on_push.id).await.unwrap().unwrap();
    assert_eq!((found.next_sync_at, found.last_error.as_deref(), found.last_success_at), (soon, Some("rejected"), Some(now)));

    store.schedule_now(repo, interval_only.id).await.unwrap();
    assert_eq!(store.due(Timestamp::now(), 10).await.unwrap().len(), 1);
}
