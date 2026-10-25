//! Archived repositories (migration `0046`) against a real, migrated `SQLite` database: the
//! archive row is read back with every repository query and goes with its repository.

use rbit_core::{AccountId, Repository, Timestamp, User, Visibility};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{admin::ListFilter, push_mirrors::NewPushMirror},
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    alice: User,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&database, "alice").await;
        Self { _dir: dir, database, alice }
    }

    async fn create(&self, owner: AccountId, name: &str) -> Repository {
        let new = NewRepository {
            owner_id: owner,
            name: name.parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        };
        self.database.repositories().create(new).await.unwrap()
    }

    async fn archived_at(&self, name: &str) -> Option<Timestamp> {
        self.database.repositories().find_by_path("alice", name).await.unwrap().unwrap().repo.archived_at
    }
}

async fn user(database: &Database, name: &str) -> User {
    let new = NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    database.accounts().create_user(new).await.unwrap()
}

#[tokio::test]
async fn archiving_is_recorded_once_and_undone_by_unarchiving() {
    let fixture = Fixture::new().await;
    let repo = fixture.create(fixture.alice.id(), "rocket").await;
    assert_eq!(repo.archived_at, None, "new repositories accept changes");
    let settings = fixture.database.repo_settings();

    assert!(settings.archive(repo.id, fixture.alice.id(), Timestamp(100)).await.unwrap());
    assert_eq!(fixture.archived_at("rocket").await, Some(Timestamp(100)));
    assert!(!settings.archive(repo.id, fixture.alice.id(), Timestamp(200)).await.unwrap(), "already archived");
    assert_eq!(fixture.archived_at("rocket").await, Some(Timestamp(100)), "the first archiving is kept");

    assert!(settings.unarchive(repo.id).await.unwrap());
    assert_eq!(fixture.archived_at("rocket").await, None);
    assert!(!settings.unarchive(repo.id).await.unwrap(), "not archived any more");
}

#[tokio::test]
async fn every_repository_query_reads_the_archive() {
    let fixture = Fixture::new().await;
    let archived = fixture.create(fixture.alice.id(), "archived").await;
    let active = fixture.create(fixture.alice.id(), "active").await;
    fixture.database.repo_settings().archive(archived.id, fixture.alice.id(), Timestamp(100)).await.unwrap();

    let by_id = fixture.database.admin().find_repository(archived.id).await.unwrap().unwrap();
    assert!(by_id.repo.is_archived());
    let owned = fixture.database.repositories().list_by_owner(fixture.alice.id(), None, false).await.unwrap();
    let flags: Vec<(String, bool)> = owned.iter().map(|repo| (repo.repo.name.to_string(), repo.repo.is_archived())).collect();
    assert!(flags.contains(&("archived".to_owned(), true)) && flags.contains(&("active".to_owned(), false)), "{flags:?}");
    let filter = ListFilter { search: "", limit: 10, offset: 0 };
    let listed = fixture.database.admin().list_repositories(filter).await.unwrap();
    assert_eq!(listed.items.iter().filter(|repo| repo.repo.is_archived()).count(), 1);
    let explore = fixture.database.repositories().list_recent_public(10).await.unwrap();
    assert_eq!(explore.iter().filter(|repo| repo.repo.is_archived()).count(), 1);

    // Page headers and cross-repository dashboards use separate optimized projections.
    let store = fixture.database.repositories();
    for (name, expected) in [("archived", Some(Timestamp(100))), ("active", None)] {
        let page = store.find_page("alice", name, Some(fixture.alice.id()), false).await.unwrap().unwrap();
        assert_eq!(page.repo.repo.archived_at, expected, "the optimized page keeps {name}'s archive state");
    }
    let readable = store.readable_among(&[archived.id, active.id], fixture.alice.id(), false).await.unwrap();
    assert_eq!(readable[&archived.id].repo.archived_at, Some(Timestamp(100)));
    assert_eq!(readable[&active.id].repo.archived_at, None);

    fixture.database.repo_settings().unarchive(archived.id).await.unwrap();
    let page = store.find_page("alice", "archived", Some(fixture.alice.id()), false).await.unwrap().unwrap();
    assert_eq!(page.repo.repo.archived_at, None, "unarchiving is visible to the optimized page immediately");
    let readable = store.readable_among(&[archived.id], fixture.alice.id(), false).await.unwrap();
    assert_eq!(readable[&archived.id].repo.archived_at, None, "unarchiving is visible to dashboards immediately");
}

#[tokio::test]
async fn the_archive_goes_with_its_repository_and_outlives_its_actor() {
    let fixture = Fixture::new().await;
    let bob = user(&fixture.database, "bob").await;
    let repo = fixture.create(fixture.alice.id(), "rocket").await;
    fixture.database.repo_settings().archive(repo.id, bob.id(), Timestamp(100)).await.unwrap();
    fixture.database.admin().delete_user(bob.id()).await.unwrap();
    assert_eq!(fixture.archived_at("rocket").await, Some(Timestamp(100)), "deleting the actor keeps the archive");

    fixture.database.repo_settings().delete(repo.id).await.unwrap();
    assert!(!fixture.database.repo_settings().unarchive(repo.id).await.unwrap(), "the archive row went with the repository");
}

#[tokio::test]
async fn mirrors_of_archived_repositories_are_never_due() {
    let fixture = Fixture::new().await;
    let pulled = fixture.create(fixture.alice.id(), "pulled").await;
    let pushed = fixture.create(fixture.alice.id(), "pushed").await;
    fixture.database.mirrors().create(pulled.id, "https://example.com/upstream.git", 60).await.unwrap();
    let new = NewPushMirror {
        repo: pushed.id,
        url: "https://example.com/copy.git",
        username: "",
        secret: None,
        sync_on_push: true,
        created_by: fixture.alice.id(),
    };
    fixture.database.push_mirrors().create(&new).await.unwrap();
    let later = Timestamp(Timestamp::now().seconds() + 1);
    let due = async || {
        let pull = fixture.database.mirrors().due(later, 10).await.unwrap().len();
        let push = fixture.database.push_mirrors().due(later, 10).await.unwrap().len();
        (pull, push, fixture.database.push_mirrors().next_due().await.unwrap().is_some())
    };
    assert_eq!(due().await, (1, 1, true), "both are due at first");

    let settings = fixture.database.repo_settings();
    settings.archive(pulled.id, fixture.alice.id(), Timestamp(100)).await.unwrap();
    settings.archive(pushed.id, fixture.alice.id(), Timestamp(100)).await.unwrap();
    assert_eq!(due().await, (0, 0, false), "archived repositories pause their mirrors");

    settings.unarchive(pulled.id).await.unwrap();
    settings.unarchive(pushed.id).await.unwrap();
    assert_eq!(due().await, (1, 1, true), "unarchiving resumes them, overdue at once");
}
