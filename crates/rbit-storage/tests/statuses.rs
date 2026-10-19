//! Commit statuses against a real, migrated `SQLite` database.

use rbit_core::{
    AccountId, RepoId, User, Visibility,
    status::{StatusReport, StatusState},
};
use rbit_storage::{Database, NewRepository, NewUser};

async fn open_database(dir: &tempfile::TempDir) -> Database {
    Database::open(&dir.path().join("rbit.db")).await.unwrap()
}

async fn create_user(database: &Database, name: &str) -> User {
    let new = NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    database.accounts().create_user(new).await.unwrap()
}

async fn create_repo(database: &Database, owner: AccountId, name: &str) -> RepoId {
    let new = NewRepository {
        owner_id: owner,
        name: name.parse().unwrap(),
        description: None,
        visibility: Visibility::Public,
        default_branch: "main".to_owned(),
    };
    database.repositories().create(new).await.unwrap().id
}

fn report(state: &str, context: &str) -> StatusReport {
    StatusReport::parse(state, Some(context), Some("described"), Some("https://ci.example/run")).unwrap()
}

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[tokio::test]
async fn reports_are_kept_and_the_latest_per_context_wins() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let repo = create_repo(&database, alice.id(), "rocket").await;
    let other = create_repo(&database, alice.id(), "other").await;
    let store = database.commit_statuses();

    let first = store.create(repo, A, &report("pending", "ci/build"), alice.id(), 1000).await.unwrap().unwrap();
    assert_eq!((first.state, first.context.as_str(), first.sha.as_str()), (StatusState::Pending, "ci/build", A));
    assert_eq!((first.description.as_deref(), first.target_url.as_deref()), (Some("described"), Some("https://ci.example/run")));
    assert_eq!((first.creator, first.created_at), (Some(alice.id()), first.updated_at));
    store.create(repo, A, &report("success", "ci/build"), alice.id(), 1000).await.unwrap().unwrap();
    store.create(repo, A, &report("failure", "lint"), alice.id(), 1000).await.unwrap().unwrap();
    store.create(repo, B, &report("error", "ci/build"), alice.id(), 1000).await.unwrap().unwrap();
    store.create(other, A, &report("failure", "ci/build"), alice.id(), 1000).await.unwrap().unwrap();

    assert_eq!(store.find(repo, first.id).await.unwrap(), Some(first.clone()), "superseded reports stay readable");
    assert_eq!(store.find(other, first.id).await.unwrap(), None, "ids are scoped to their repository");

    let history = store.list(repo, A, 0, 10).await.unwrap();
    let states: Vec<_> = history.iter().map(|status| (status.context.as_str(), status.state)).collect();
    assert_eq!(states, [("lint", StatusState::Failure), ("ci/build", StatusState::Success), ("ci/build", StatusState::Pending)]);
    assert_eq!(store.count(repo, A).await.unwrap(), 3);
    assert_eq!(store.list(repo, A, 1, 1).await.unwrap()[0].state, StatusState::Success, "offset and limit");

    let latest = store.latest(repo, A).await.unwrap();
    let latest: Vec<_> = latest.iter().map(|status| (status.context.as_str(), status.state)).collect();
    assert_eq!(latest, [("ci/build", StatusState::Success), ("lint", StatusState::Failure)], "by context, newest only");

    let many = store.latest_of(repo, &[A.to_owned(), B.to_owned(), "c".repeat(40)]).await.unwrap();
    let many: Vec<_> = many.iter().map(|status| (status.sha.as_str(), status.context.as_str(), status.state)).collect();
    assert_eq!(
        many,
        [(A, "ci/build", StatusState::Success), (A, "lint", StatusState::Failure), (B, "ci/build", StatusState::Error)],
        "only this repository's"
    );
    assert_eq!(store.latest_of(repo, &[]).await.unwrap().len(), 0);
}

#[tokio::test]
async fn the_cap_applies_per_commit_and_context() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let repo = create_repo(&database, alice.id(), "rocket").await;
    let store = database.commit_statuses();

    for _ in 0..3 {
        assert!(store.create(repo, A, &report("pending", "ci"), alice.id(), 3).await.unwrap().is_some());
    }
    assert!(store.create(repo, A, &report("success", "ci"), alice.id(), 3).await.unwrap().is_none(), "the fourth is refused");
    assert_eq!(store.count(repo, A).await.unwrap(), 3);
    assert!(store.create(repo, A, &report("success", "other"), alice.id(), 3).await.unwrap().is_some(), "another context");
    assert!(store.create(repo, B, &report("success", "ci"), alice.id(), 3).await.unwrap().is_some(), "another commit");
}

#[tokio::test]
async fn statuses_go_with_their_repository_and_outlive_their_creator() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bot = create_user(&database, "ci-bot").await;
    let repo = create_repo(&database, alice.id(), "rocket").await;
    let store = database.commit_statuses();
    store.create(repo, A, &report("success", "ci"), bot.id(), 1000).await.unwrap().unwrap();

    database.admin().delete_user(bot.id()).await.unwrap();
    let kept = store.latest(repo, A).await.unwrap();
    assert_eq!((kept.len(), kept[0].creator), (1, None), "the creator is forgotten, the status stays");

    database.repositories().delete(repo).await.unwrap();
    assert_eq!(store.count(repo, A).await.unwrap(), 0);
}
