//! Collaborators, per-viewer grants and repository transfer against a real, migrated
//! SQLite database.

use rustit_core::{
    AccountId, OrgRole, RepoId, User, Visibility,
    membership::{BasePermission, Permission},
};
use rustit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::{collaborators::ViewerGrants, orgs::NewOrganization, teams::TeamChange},
};

async fn open_database(dir: &tempfile::TempDir) -> Database {
    Database::open(&dir.path().join("rustit.db")).await.unwrap()
}

async fn create_user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

async fn create_org(database: &Database, name: &str, creator: &User) -> AccountId {
    let new = NewOrganization { name: name.parse().unwrap(), display_name: None, description: None, creator: creator.id() };
    database.orgs().create(new).await.unwrap().account.id
}

async fn create_repo(database: &Database, owner: AccountId, name: &str) -> RepoId {
    database
        .repositories()
        .create(NewRepository {
            owner_id: owner,
            name: name.parse().unwrap(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn collaborators_are_added_changed_listed_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let carol = create_user(&database, "Carol").await;
    let repo = create_repo(&database, alice.id(), "project").await;
    let store = database.collaborators();

    store.add(repo, carol.id(), Permission::Write, alice.id()).await.unwrap();
    store.add(repo, bob.id(), Permission::Read, alice.id()).await.unwrap();
    assert!(matches!(store.add(repo, bob.id(), Permission::Admin, alice.id()).await, Err(StorageError::Conflict("collaborator"))));

    let listed = store.list(repo).await.unwrap();
    let names: Vec<_> = listed.iter().map(|c| (c.user.name.to_string(), c.permission)).collect();
    assert_eq!(names, [("bob".to_owned(), Permission::Read), ("Carol".to_owned(), Permission::Write)], "by name, case-insensitively");
    assert_eq!(store.permission(repo, bob.id()).await.unwrap(), Some(Permission::Read));
    assert_eq!(store.permission(repo, alice.id()).await.unwrap(), None);

    assert!(store.set_permission(repo, bob.id(), Permission::Admin).await.unwrap());
    assert_eq!(store.permission(repo, bob.id()).await.unwrap(), Some(Permission::Admin));
    assert!(!store.set_permission(repo, alice.id(), Permission::Read).await.unwrap(), "not a collaborator");

    assert!(store.remove(repo, bob.id()).await.unwrap());
    assert!(!store.remove(repo, bob.id()).await.unwrap());
    assert_eq!(store.list(repo).await.unwrap().len(), 1);

    // Deleting the repository (or the account) removes the rows.
    database.repositories().delete(repo).await.unwrap();
    assert!(store.list(repo).await.unwrap().is_empty());
}

#[tokio::test]
async fn grants_report_everything_in_one_query() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let eve = create_user(&database, "eve").await;
    let org = create_org(&database, "acme", &alice).await;
    let repo = create_repo(&database, org, "rocket").await;
    let repos = database.repositories();

    assert_eq!(repos.grants(repo, org, eve.id()).await.unwrap(), ViewerGrants::default());
    assert_eq!(
        repos.grants(repo, org, alice.id()).await.unwrap(),
        ViewerGrants { org_role: Some(OrgRole::Owner), org_base: BasePermission::Write, team: None, collaborator: None }
    );

    database.orgs().add_member(org, bob.id(), OrgRole::Member).await.unwrap();
    database.orgs().set_base_permission(org, BasePermission::None).await.unwrap();
    let teams = database.teams();
    let readers = teams.create(org, &"readers".parse().unwrap(), None).await.unwrap();
    let writers = teams.create(org, &"writers".parse().unwrap(), None).await.unwrap();
    for team in [&readers, &writers] {
        assert_eq!(teams.add_member(team.id, bob.id()).await.unwrap(), TeamChange::Done);
    }
    teams.set_repository(readers.id, repo, Permission::Read).await.unwrap();
    teams.set_repository(writers.id, repo, Permission::Write).await.unwrap();
    database.collaborators().add(repo, bob.id(), Permission::Read, alice.id()).await.unwrap();

    assert_eq!(
        repos.grants(repo, org, bob.id()).await.unwrap(),
        ViewerGrants {
            org_role: Some(OrgRole::Member),
            org_base: BasePermission::None,
            team: Some(Permission::Write),
            collaborator: Some(Permission::Read)
        },
        "the best team grant wins"
    );
}

#[tokio::test]
async fn leaving_an_organisation_revokes_its_teams_and_collaborator_entries() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let org = create_org(&database, "acme", &alice).await;
    let other = create_org(&database, "other", &alice).await;
    let rocket = create_repo(&database, org, "rocket").await;
    let elsewhere = create_repo(&database, other, "elsewhere").await;
    let personal = create_repo(&database, alice.id(), "personal").await;

    database.orgs().add_member(org, bob.id(), OrgRole::Member).await.unwrap();
    database.orgs().add_member(other, bob.id(), OrgRole::Member).await.unwrap();
    let team = database.teams().create(org, &"core".parse().unwrap(), None).await.unwrap();
    let other_team = database.teams().create(other, &"core".parse().unwrap(), None).await.unwrap();
    database.teams().add_member(team.id, bob.id()).await.unwrap();
    database.teams().add_member(other_team.id, bob.id()).await.unwrap();
    for repo in [rocket, elsewhere, personal] {
        database.collaborators().add(repo, bob.id(), Permission::Write, alice.id()).await.unwrap();
    }

    database.orgs().remove_member(org, bob.id()).await.unwrap();
    assert!(!database.teams().is_member(team.id, bob.id()).await.unwrap());
    assert!(database.teams().is_member(other_team.id, bob.id()).await.unwrap(), "other organisations are untouched");
    assert_eq!(database.collaborators().permission(rocket, bob.id()).await.unwrap(), None);
    assert_eq!(database.collaborators().permission(elsewhere, bob.id()).await.unwrap(), Some(Permission::Write));
    assert_eq!(database.collaborators().permission(personal, bob.id()).await.unwrap(), Some(Permission::Write));
}

#[tokio::test]
async fn transfer_moves_ownership_and_drops_old_team_grants() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let org = create_org(&database, "acme", &alice).await;
    let repo = create_repo(&database, org, "rocket").await;
    create_repo(&database, bob.id(), "taken").await;
    let team = database.teams().create(org, &"core".parse().unwrap(), None).await.unwrap();
    database.teams().set_repository(team.id, repo, Permission::Write).await.unwrap();
    database.collaborators().add(repo, bob.id(), Permission::Write, alice.id()).await.unwrap();
    let stranger = create_user(&database, "stranger").await;
    database.collaborators().add(repo, stranger.id(), Permission::Read, alice.id()).await.unwrap();
    database.stars().add(stranger.id(), repo).await.unwrap();

    database.repositories().transfer(repo, bob.id()).await.unwrap();
    let moved = database.repositories().find_by_path("bob", "rocket").await.unwrap().unwrap();
    assert_eq!(moved.repo.id, repo);
    assert!(database.repositories().find_by_path("acme", "rocket").await.unwrap().is_none());
    assert!(database.teams().grants_for_repository(repo).await.unwrap().is_empty(), "team grants belong to the old organisation");
    assert_eq!(database.collaborators().permission(repo, bob.id()).await.unwrap(), None, "the new owner needs no entry");
    assert_eq!(database.collaborators().permission(repo, stranger.id()).await.unwrap(), Some(Permission::Read), "others stay");
    assert_eq!(database.stars().count(repo, None).await.unwrap().count, 1);

    // A name clash at the target (case-insensitive) is refused and changes nothing.
    let clash = create_repo(&database, org, "Taken").await;
    assert!(matches!(database.repositories().transfer(clash, bob.id()).await, Err(StorageError::Conflict("repository name"))));
    assert_eq!(database.repositories().find_by_path("acme", "Taken").await.unwrap().unwrap().repo.id, clash);
}

#[tokio::test]
async fn base_permission_defaults_to_write_and_can_be_changed() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let org = create_org(&database, "acme", &alice).await;
    let orgs = database.orgs();

    assert_eq!(orgs.base_permission(org).await.unwrap(), BasePermission::Write);
    for base in BasePermission::ALL {
        orgs.set_base_permission(org, base).await.unwrap();
        assert_eq!(orgs.base_permission(org).await.unwrap(), base);
    }
    // Saving the profile keeps the base permission.
    orgs.update_profile(org, rustit_storage::sqlite::orgs::OrgProfileUpdate::default()).await.unwrap();
    assert_eq!(orgs.base_permission(org).await.unwrap(), BasePermission::Write);
}
