//! Git LFS object, link and lock persistence against a real, migrated SQLite database.

use rbit_core::{
    RepoId, Timestamp, User, Visibility,
    lfs::{LfsLockId, LfsOid},
};
use rbit_storage::{Database, NewRepository, NewUser, StorageError, sqlite::lfs::LockFilter};

async fn setup() -> (tempfile::TempDir, Database, User, User, RepoId, RepoId) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = |name: &str| NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    let alice = database.accounts().create_user(user("alice")).await.unwrap();
    let bob = database.accounts().create_user(user("bob")).await.unwrap();
    let repo = |name: &str| NewRepository {
        owner_id: alice.id(),
        name: name.parse().unwrap(),
        description: None,
        visibility: Visibility::Public,
        default_branch: "main".to_owned(),
    };
    let first = database.repositories().create(repo("first")).await.unwrap().id;
    let second = database.repositories().create(repo("second")).await.unwrap().id;
    (dir, database, alice, bob, first, second)
}

fn oid(byte: u8) -> LfsOid {
    LfsOid::from_digest([byte; 32])
}

/// A cutoff after everything created so far (orphans are "older" than it).
fn later() -> Timestamp {
    Timestamp(Timestamp::now().seconds() + 10)
}

#[tokio::test]
async fn objects_are_stored_once_and_linked_per_repository() {
    let (_dir, database, _, _, first, second) = setup().await;
    let store = database.lfs();
    assert_eq!(store.find_object(&oid(1)).await.unwrap(), None);
    assert!(!store.link(first, &oid(1)).await.unwrap(), "nothing to link before the bytes are recorded");

    store.record_object(&oid(1), 42, first).await.unwrap();
    store.record_object(&oid(1), 42, first).await.unwrap();
    let object = store.find_object(&oid(1)).await.unwrap().unwrap();
    assert_eq!((object.oid.clone(), object.size), (oid(1), 42));
    assert!(store.is_linked(first, &oid(1)).await.unwrap());
    assert!(!store.is_linked(second, &oid(1)).await.unwrap());
    assert_eq!(store.linked_in_any(&[second], &oid(1)).await.unwrap(), None);
    assert_eq!(store.linked_in_any(&[second, first], &oid(1)).await.unwrap(), Some(first));
    assert_eq!(store.linked_in_any(&[], &oid(1)).await.unwrap(), None);

    assert!(store.link(second, &oid(1)).await.unwrap());
    assert!(store.link(second, &oid(1)).await.unwrap(), "linking twice is fine");
    store.record_object(&oid(2), 7, first).await.unwrap();
    let linked = store.linked_objects(second, &[oid(1), oid(2), oid(3)]).await.unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked.get(&oid(1)), Some(&42));
    assert!(store.linked_objects(second, &[]).await.unwrap().is_empty());
    assert_eq!(store.usage(first).await.unwrap(), (2, 49));
    assert_eq!(store.usage(second).await.unwrap(), (1, 42));
}

#[tokio::test]
async fn forks_copy_links() {
    let (_dir, database, _, _, first, second) = setup().await;
    let store = database.lfs();
    store.record_object(&oid(1), 1, first).await.unwrap();
    store.record_object(&oid(2), 2, first).await.unwrap();
    store.record_object(&oid(2), 2, second).await.unwrap();
    assert_eq!(store.copy_links(first, second).await.unwrap(), 1, "only the missing link is added");
    assert_eq!(store.usage(second).await.unwrap(), (2, 3));
}

#[tokio::test]
async fn orphans_appear_once_unlinked_and_only_after_the_grace_period() {
    let (_dir, database, _, _, first, second) = setup().await;
    let store = database.lfs();
    store.record_object(&oid(1), 1, first).await.unwrap();
    store.record_object(&oid(2), 2, first).await.unwrap();
    store.link(second, &oid(2)).await.unwrap();
    assert!(store.orphans(later(), 10).await.unwrap().is_empty(), "everything is linked");

    // Deleting a repository unlinks its objects (by cascade, which stamps `unlinked_at`).
    database.repositories().delete(first).await.unwrap();
    assert_eq!(store.orphans(later(), 10).await.unwrap(), [oid(1)], "the other object is still used by `second`");
    assert!(store.orphans(Timestamp(0), 10).await.unwrap().is_empty(), "not before the grace period ends");
    assert!(!store.delete_orphan(&oid(1), Timestamp(0)).await.unwrap());
    assert!(!store.delete_orphan(&oid(2), later()).await.unwrap(), "linked objects are never removed");
    assert!(store.delete_orphan(&oid(1), later()).await.unwrap());
    assert_eq!(store.find_object(&oid(1)).await.unwrap(), None);
    assert!(store.find_object(&oid(2)).await.unwrap().is_some());
}

#[tokio::test]
async fn a_relinked_orphan_is_kept() {
    let (_dir, database, _, _, first, second) = setup().await;
    let store = database.lfs();
    store.record_object(&oid(1), 1, first).await.unwrap();
    database.repositories().delete(first).await.unwrap();
    assert_eq!(store.orphans(later(), 10).await.unwrap(), [oid(1)]);
    store.record_object(&oid(1), 1, second).await.unwrap();
    assert!(!store.delete_orphan(&oid(1), later()).await.unwrap());
}

#[tokio::test]
async fn locks_are_unique_per_path_and_listed_by_filter() {
    let (_dir, database, alice, bob, first, second) = setup().await;
    let store = database.lfs();
    let logo = store.create_lock(first, "assets/logo.psd", alice.id()).await.unwrap();
    assert_eq!((logo.path.as_str(), logo.owner.name.as_str(), logo.repo_id), ("assets/logo.psd", "alice", first));
    assert!(matches!(store.create_lock(first, "assets/logo.psd", bob.id()).await, Err(StorageError::Conflict(_))));
    let other_repo = store.create_lock(second, "assets/logo.psd", bob.id()).await.unwrap();
    let song = store.create_lock(first, "audio/song.wav", bob.id()).await.unwrap();
    let video = store.create_lock(first, "video.mp4", bob.id()).await.unwrap();

    let ids = |locks: Vec<rbit_core::lfs::LfsLock>| locks.into_iter().map(|lock| lock.id).collect::<Vec<_>>();
    let all = LockFilter { limit: 10, ..LockFilter::default() };
    assert_eq!(ids(store.list_locks(first, &all).await.unwrap()), [logo.id, song.id, video.id]);
    let page = LockFilter { limit: 2, ..LockFilter::default() };
    assert_eq!(ids(store.list_locks(first, &page).await.unwrap()), [logo.id, song.id]);
    let next = LockFilter { after: Some(song.id), limit: 2, ..LockFilter::default() };
    assert_eq!(ids(store.list_locks(first, &next).await.unwrap()), [video.id]);
    let by_path = LockFilter { path: Some("audio/song.wav"), limit: 10, ..LockFilter::default() };
    assert_eq!(ids(store.list_locks(first, &by_path).await.unwrap()), [song.id]);
    let by_id = LockFilter { id: Some(other_repo.id), limit: 10, ..LockFilter::default() };
    assert!(store.list_locks(first, &by_id).await.unwrap().is_empty(), "scoped to the repository");

    assert_eq!(store.find_lock_by_path(first, "video.mp4").await.unwrap().map(|l| l.id), Some(video.id));
    assert_eq!(store.find_lock(second, logo.id).await.unwrap(), None);
    assert!(!store.delete_lock(second, logo.id).await.unwrap());
    assert!(store.delete_lock(first, logo.id).await.unwrap());
    assert!(store.find_lock(first, logo.id).await.unwrap().is_none());
    assert!(!store.delete_lock(first, LfsLockId(999)).await.unwrap());
}
