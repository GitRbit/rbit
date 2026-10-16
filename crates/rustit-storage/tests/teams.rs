//! Teams, their members and repository grants against a real, migrated SQLite database.

use rustit_core::{
    AccountId, OrgRole, RepoId, User, Visibility,
    membership::{Permission, TeamName},
};
use rustit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::{orgs::NewOrganization, teams::TeamChange},
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

async fn create_repo(database: &Database, owner: AccountId, name: &str, visibility: Visibility) -> RepoId {
    database
        .repositories()
        .create(NewRepository {
            owner_id: owner,
            name: name.parse().unwrap(),
            description: None,
            visibility,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap()
        .id
}

fn name(value: &str) -> TeamName {
    value.parse().unwrap()
}

#[tokio::test]
async fn teams_are_unique_per_organisation_and_listed_with_counts() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let acme = create_org(&database, "acme", &alice).await;
    let other = create_org(&database, "other", &alice).await;
    let teams = database.teams();

    let core = teams.create(acme, &name("core"), Some("The core team")).await.unwrap();
    assert_eq!(core.description.as_deref(), Some("The core team"));
    assert!(matches!(teams.create(acme, &name("CORE"), None).await, Err(StorageError::Conflict("team name"))));
    teams.create(other, &name("core"), None).await.expect("names are unique per organisation only");
    let docs = teams.create(acme, &name("Docs"), None).await.unwrap();

    let found = teams.find_by_name(acme, "Core").await.unwrap().unwrap();
    assert_eq!(found.id, core.id);
    assert!(teams.find_by_name(acme, "missing").await.unwrap().is_none());

    let rocket = create_repo(&database, acme, "rocket", Visibility::Private).await;
    teams.set_repository(core.id, rocket, Permission::Write).await.unwrap();
    teams.add_member(core.id, alice.id()).await.unwrap();
    let listed = teams.list(acme).await.unwrap();
    let summary: Vec<_> = listed.iter().map(|t| (t.team.name.to_string(), t.member_count, t.repo_count)).collect();
    assert_eq!(summary, [("core".to_owned(), 1, 1), ("Docs".to_owned(), 0, 0)]);

    assert!(matches!(teams.update(docs.id, &name("core"), None).await, Err(StorageError::Conflict("team name"))));
    teams.update(docs.id, &name("writers"), Some("Docs")).await.unwrap();
    assert_eq!(teams.find_by_name(acme, "writers").await.unwrap().unwrap().description.as_deref(), Some("Docs"));

    teams.delete(core.id).await.unwrap();
    assert!(teams.find_by_name(acme, "core").await.unwrap().is_none());
    assert!(teams.grants_for_repository(rocket).await.unwrap().is_empty(), "grants go with the team");
}

#[tokio::test]
async fn members_must_belong_to_the_organisation() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let eve = create_user(&database, "eve").await;
    let acme = create_org(&database, "acme", &alice).await;
    database.orgs().add_member(acme, bob.id(), OrgRole::Member).await.unwrap();
    let teams = database.teams();
    let core = teams.create(acme, &name("core"), None).await.unwrap();

    assert_eq!(teams.add_member(core.id, bob.id()).await.unwrap(), TeamChange::Done);
    assert_eq!(teams.add_member(core.id, alice.id()).await.unwrap(), TeamChange::Done);
    assert!(matches!(teams.add_member(core.id, bob.id()).await, Err(StorageError::Conflict("team member"))));
    assert_eq!(teams.add_member(core.id, eve.id()).await.unwrap(), TeamChange::OutsideOrganization);

    let names: Vec<_> = teams.members(core.id).await.unwrap().iter().map(|a| a.name.to_string()).collect();
    assert_eq!(names, ["alice", "bob"]);
    assert!(teams.is_member(core.id, bob.id()).await.unwrap());
    assert!(teams.remove_member(core.id, bob.id()).await.unwrap());
    assert!(!teams.remove_member(core.id, bob.id()).await.unwrap());
    assert!(!teams.is_member(core.id, bob.id()).await.unwrap());
}

#[tokio::test]
async fn teams_are_only_granted_their_organisations_repositories() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let acme = create_org(&database, "acme", &alice).await;
    let other = create_org(&database, "other", &alice).await;
    database.orgs().add_member(acme, bob.id(), OrgRole::Member).await.unwrap();
    let rocket = create_repo(&database, acme, "rocket", Visibility::Private).await;
    let brochure = create_repo(&database, acme, "brochure", Visibility::Public).await;
    let foreign = create_repo(&database, other, "foreign", Visibility::Public).await;
    let personal = create_repo(&database, alice.id(), "personal", Visibility::Public).await;
    let teams = database.teams();
    let core = teams.create(acme, &name("core"), None).await.unwrap();

    assert_eq!(teams.set_repository(core.id, rocket, Permission::Read).await.unwrap(), TeamChange::Done);
    assert_eq!(teams.set_repository(core.id, rocket, Permission::Admin).await.unwrap(), TeamChange::Done, "upsert");
    assert_eq!(teams.set_repository(core.id, brochure, Permission::Write).await.unwrap(), TeamChange::Done);
    assert_eq!(teams.set_repository(core.id, foreign, Permission::Read).await.unwrap(), TeamChange::OutsideOrganization);
    assert_eq!(teams.set_repository(core.id, personal, Permission::Read).await.unwrap(), TeamChange::OutsideOrganization);

    let grants = teams.grants_for_repository(rocket).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!((grants[0].team.id, grants[0].permission), (core.id, Permission::Admin));

    // The team page lists what the viewer may read: everything for a site admin or an
    // organisation member with the default base permission, public ones for anonymous visitors.
    let names = |list: Vec<(rustit_core::OwnedRepository, Permission)>| {
        list.into_iter().map(|(repo, permission)| format!("{}:{}", repo.repo.name, permission)).collect::<Vec<_>>()
    };
    assert_eq!(names(teams.repositories(core.id, None, true).await.unwrap()), ["brochure:write", "rocket:admin"]);
    assert_eq!(names(teams.repositories(core.id, Some(bob.id()), false).await.unwrap()), ["brochure:write", "rocket:admin"]);
    assert_eq!(names(teams.repositories(core.id, None, false).await.unwrap()), ["brochure:write"]);

    assert!(teams.remove_repository(core.id, rocket).await.unwrap());
    assert!(!teams.remove_repository(core.id, rocket).await.unwrap());
    assert!(teams.grants_for_repository(rocket).await.unwrap().is_empty());
}
