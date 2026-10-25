//! Old repository names (migration `0016`) against a real, migrated `SQLite` database: the
//! triggers record a redirect whenever a repository leaves a location and drop it
//! whenever a repository arrives at one.

use rbit_core::{AccountId, RepoId, Repository, User, Visibility};
use rbit_storage::{Database, NewRepository, NewUser};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    alice: User,
    bob: User,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        Self { _dir: dir, database, alice, bob }
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

    async fn rename(&self, repo: RepoId, name: &str) {
        self.database.repo_settings().rename(repo, &name.parse().unwrap()).await.unwrap();
    }

    /// The id of the repository `owner/name` redirects to, if any.
    async fn redirect(&self, owner: &str, name: &str) -> Option<RepoId> {
        self.database.repositories().find_by_old_path(owner, name).await.unwrap().map(|repo| repo.repo.id)
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
async fn renames_leave_a_redirect_at_the_old_name() {
    let fixture = Fixture::new().await;
    let repo = fixture.create(fixture.alice.id(), "old").await;
    assert_eq!(fixture.redirect("alice", "old").await, None, "nothing moved yet");

    fixture.rename(repo.id, "new").await;
    let moved = fixture.database.repositories().find_by_old_path("ALICE", "Old").await.unwrap().unwrap();
    assert_eq!(moved.repo.id, repo.id, "names match case-insensitively");
    assert_eq!(moved.full_name(), "alice/new", "the redirect leads to the current location");
    assert_eq!(fixture.redirect("alice", "new").await, None, "the current name is not a redirect");
}

#[tokio::test]
async fn every_old_name_leads_to_the_current_location() {
    let fixture = Fixture::new().await;
    let repo = fixture.create(fixture.alice.id(), "one").await;
    fixture.rename(repo.id, "two").await;
    fixture.rename(repo.id, "three").await;
    for old in ["one", "two"] {
        let moved = fixture.database.repositories().find_by_old_path("alice", old).await.unwrap().unwrap();
        assert_eq!(moved.full_name(), "alice/three", "{old} resolves without a chain of hops");
    }
}

#[tokio::test]
async fn changing_only_the_letter_case_is_not_a_move() {
    let fixture = Fixture::new().await;
    let repo = fixture.create(fixture.alice.id(), "project").await;
    fixture.rename(repo.id, "Project").await;
    assert_eq!(fixture.redirect("alice", "project").await, None);
}

#[tokio::test]
async fn account_redirects_follow_identity_across_moves_and_name_reuse() {
    let fixture = Fixture::new().await;
    let accounts = fixture.database.accounts();
    let repo = fixture.create(fixture.alice.id(), "before").await;
    accounts.rename(fixture.alice.id(), &"alice".parse().unwrap()).await.unwrap();
    assert!(accounts.find_by_old_name("alice").await.unwrap().is_none(), "unchanged names do not redirect");
    accounts.rename(fixture.alice.id(), &"Alice".parse().unwrap()).await.unwrap();
    assert!(accounts.find_by_old_name("alice").await.unwrap().is_none());
    accounts.rename(fixture.alice.id(), &"renamed".parse().unwrap()).await.unwrap();
    fixture.rename(repo.id, "after").await;
    accounts.rename(fixture.alice.id(), &"latest".parse().unwrap()).await.unwrap();
    for old in ["ALICE", "Renamed"] {
        let account = accounts.find_by_old_name(old).await.unwrap().unwrap();
        assert_eq!(account.id, fixture.alice.id());
        assert_eq!(account.name.as_str(), "latest");
        assert_eq!(fixture.redirect(old, "before").await, Some(repo.id));
        assert_eq!(fixture.redirect(old, "after").await, Some(repo.id));
        assert!(accounts.find_login(old).await.unwrap().is_none(), "old names cannot sign in");
    }
    accounts.rename(fixture.alice.id(), &"alice".parse().unwrap()).await.unwrap();
    assert!(accounts.find_by_old_name("alice").await.unwrap().is_none(), "renaming back cannot loop");
    accounts.rename(fixture.alice.id(), &"final".parse().unwrap()).await.unwrap();
    accounts.rename(fixture.bob.id(), &"alice".parse().unwrap()).await.unwrap();
    let newcomer = accounts.find_user(fixture.bob.id()).await.unwrap().unwrap();
    assert!(accounts.find_by_old_name("alice").await.unwrap().is_none());
    assert_eq!(fixture.redirect("alice", "before").await, None, "new account shadows the entire old namespace");
    accounts.rename(newcomer.id(), &"newcomer".parse().unwrap()).await.unwrap();
    assert_eq!(accounts.find_by_old_name("alice").await.unwrap().unwrap().id, newcomer.id());
    assert_eq!(fixture.redirect("alice", "after").await, None, "former owner's redirects never reappear");
}

#[tokio::test]
async fn renaming_back_reclaims_the_old_name() {
    let fixture = Fixture::new().await;
    let repo = fixture.create(fixture.alice.id(), "first").await;
    fixture.rename(repo.id, "second").await;
    fixture.rename(repo.id, "first").await;
    assert_eq!(fixture.redirect("alice", "first").await, None, "the repository lives there again");
    assert_eq!(fixture.redirect("alice", "second").await, Some(repo.id));
}

#[tokio::test]
async fn a_new_repository_at_an_old_name_takes_precedence() {
    let fixture = Fixture::new().await;
    let moved = fixture.create(fixture.alice.id(), "app").await;
    fixture.rename(moved.id, "app-legacy").await;
    assert_eq!(fixture.redirect("alice", "app").await, Some(moved.id));

    let newcomer = fixture.create(fixture.alice.id(), "app").await;
    assert_eq!(fixture.redirect("alice", "app").await, None, "creating the repository removed the redirect");
    let found = fixture.database.repositories().find_by_path("alice", "app").await.unwrap().unwrap();
    assert_eq!(found.repo.id, newcomer.id);

    // Moving the newcomer away records the name for the newcomer, not for the first one.
    fixture.rename(newcomer.id, "app-next").await;
    assert_eq!(fixture.redirect("alice", "app").await, Some(newcomer.id));
}

#[tokio::test]
async fn renaming_onto_an_old_name_takes_precedence_too() {
    let fixture = Fixture::new().await;
    let moved = fixture.create(fixture.alice.id(), "docs").await;
    fixture.rename(moved.id, "handbook").await;
    let other = fixture.create(fixture.alice.id(), "notes").await;
    fixture.rename(other.id, "docs").await;
    assert_eq!(fixture.redirect("alice", "docs").await, None);
    assert_eq!(fixture.redirect("alice", "notes").await, Some(other.id));
}

#[tokio::test]
async fn transfers_leave_a_redirect_under_the_old_owner() {
    let fixture = Fixture::new().await;
    let repo = fixture.create(fixture.alice.id(), "tool").await;
    fixture.database.repositories().transfer(repo.id, fixture.bob.id()).await.unwrap();
    let moved = fixture.database.repositories().find_by_old_path("alice", "tool").await.unwrap().unwrap();
    assert_eq!(moved.full_name(), "bob/tool");
    assert_eq!(fixture.redirect("bob", "tool").await, None);

    // Alice may create a new `tool`, which wins over the redirect.
    fixture.create(fixture.alice.id(), "tool").await;
    assert_eq!(fixture.redirect("alice", "tool").await, None);
}

#[tokio::test]
async fn deleting_a_repository_removes_its_redirects() {
    let fixture = Fixture::new().await;
    let repo = fixture.create(fixture.alice.id(), "gone").await;
    fixture.rename(repo.id, "going").await;
    fixture.database.repositories().delete(repo.id).await.unwrap();
    assert_eq!(fixture.redirect("alice", "gone").await, None);
}
