//! Scratch refs against a real, migrated `SQLite` database: a push records one, a newer push
//! restarts its lifetime, the owner's count leaves out the ref being pushed, and only refs
//! past their lifetime come back to the sweep, which forgets one only at the commit it saw.

use rbit_core::{RepoId, Timestamp, Visibility};
use rbit_storage::{Database, NewRepository, NewUser};

const A: &str = "0123456789abcdef0123456789abcdef01234567";
const B: &str = "89abcdef0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn scratch_refs_are_recorded_counted_and_expired() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = NewUser {
        name: "alice".parse().unwrap(),
        email: "alice@example.com".parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    let alice = db.accounts().create_user(user).await.unwrap().id();
    let new = NewRepository { owner_id: alice, name: "rocket".parse().unwrap(), description: None, visibility: Visibility::Public, default_branch: "main".to_owned() };
    let repo: RepoId = db.repositories().create(new).await.unwrap().id;
    let store = db.actions_runs();

    store.record_scratch(repo, "refs/scratch/alice/one", Some(alice), A, Timestamp(100)).await.unwrap();
    store.record_scratch(repo, "refs/scratch/alice/two", Some(alice), A, Timestamp(200)).await.unwrap();
    assert_eq!(store.count_scratch(repo, alice, "refs/scratch/alice/new").await.unwrap(), 2);
    assert_eq!(store.count_scratch(repo, alice, "refs/scratch/alice/one").await.unwrap(), 1, "pushing again is not a new ref");

    // A newer push to `one` restarts its lifetime: only `two` is past it.
    store.record_scratch(repo, "refs/scratch/alice/one", Some(alice), B, Timestamp(300)).await.unwrap();
    let expired = store.expired_scratch(Timestamp(250), 10).await.unwrap();
    assert_eq!(expired.iter().map(|scratch| (scratch.name.as_str(), scratch.sha.as_str())).collect::<Vec<_>>(), [("refs/scratch/alice/two", A)]);

    // Forgetting at a commit the ref no longer has leaves it.
    store.forget_scratch(repo, "refs/scratch/alice/one", A).await.unwrap();
    store.forget_scratch(repo, "refs/scratch/alice/two", A).await.unwrap();
    assert_eq!(store.count_scratch(repo, alice, "").await.unwrap(), 1);
    assert_eq!(store.expired_scratch(Timestamp(1000), 10).await.unwrap().len(), 1);
}
