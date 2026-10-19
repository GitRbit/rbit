//! Organisation and membership persistence against a real, migrated `SQLite` database.

use rbit_core::{AccountId, OrgRole, RepoSort, User, Visibility};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::orgs::{MembershipChange, NewOrganization, OrgProfileUpdate},
};

async fn open_database(dir: &tempfile::TempDir) -> Database {
    Database::open(&dir.path().join("rbit.db")).await.unwrap()
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

fn new_org(name: &str, creator: &User) -> NewOrganization {
    NewOrganization { name: name.parse().unwrap(), display_name: Some("Acme Inc.".to_owned()), description: None, creator: creator.id() }
}

async fn create_repo(database: &Database, owner: AccountId, name: &str, description: Option<&str>, visibility: Visibility) {
    database
        .repositories()
        .create(NewRepository {
            owner_id: owner,
            name: name.parse().unwrap(),
            description: description.map(str::to_owned),
            visibility,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn creator_becomes_owner_and_names_are_shared_with_users() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let orgs = database.orgs();

    let acme = orgs.create(new_org("acme", &alice)).await.unwrap();
    assert!(acme.account.is_organization());
    assert_eq!(orgs.role_of(acme.account.id, alice.id()).await.unwrap(), Some(OrgRole::Owner));

    let found = orgs.find_by_name("ACME").await.unwrap().unwrap();
    assert_eq!(found.account.id, acme.account.id);
    assert_eq!(found.label(), "Acme Inc.");
    assert!(orgs.find_by_name("alice").await.unwrap().is_none(), "users are not organisations");

    let clash = orgs.create(new_org("Alice", &alice)).await;
    assert!(matches!(clash, Err(StorageError::Conflict("name"))));

    let memberships = orgs.memberships_of(alice.id()).await.unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].org.name.as_str(), "acme");
}

#[tokio::test]
async fn profile_updates_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let orgs = database.orgs();
    let acme = orgs.create(new_org("acme", &alice)).await.unwrap();

    let update =
        OrgProfileUpdate { display_name: None, description: Some("Rockets".to_owned()), website: Some("https://acme.example".to_owned()) };
    orgs.update_profile(acme.account.id, update).await.unwrap();

    let found = orgs.find(acme.account.id).await.unwrap().unwrap();
    assert_eq!(found.account.display_name, None);
    assert_eq!(found.label(), "acme");
    assert_eq!(found.description.as_deref(), Some("Rockets"));
    assert_eq!(found.website.as_deref(), Some("https://acme.example"));
}

#[tokio::test]
async fn the_last_owner_can_never_be_removed_or_demoted() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let carol = create_user(&database, "carol").await;
    let orgs = database.orgs();
    let org = orgs.create(new_org("acme", &alice)).await.unwrap().account.id;

    orgs.add_member(org, bob.id(), OrgRole::Member).await.unwrap();
    assert!(matches!(orgs.add_member(org, bob.id(), OrgRole::Owner).await, Err(StorageError::Conflict("member"))));

    assert_eq!(orgs.set_role(org, alice.id(), OrgRole::Member).await.unwrap(), MembershipChange::LastOwner);
    assert_eq!(orgs.remove_member(org, alice.id()).await.unwrap(), MembershipChange::LastOwner);
    assert_eq!(orgs.set_role(org, alice.id(), OrgRole::Owner).await.unwrap(), MembershipChange::Done, "no-op is fine");
    assert_eq!(orgs.remove_member(org, carol.id()).await.unwrap(), MembershipChange::NotMember);
    assert_eq!(orgs.set_role(org, carol.id(), OrgRole::Owner).await.unwrap(), MembershipChange::NotMember);

    assert_eq!(orgs.set_role(org, bob.id(), OrgRole::Owner).await.unwrap(), MembershipChange::Done);
    assert_eq!(orgs.set_role(org, alice.id(), OrgRole::Member).await.unwrap(), MembershipChange::Done);
    assert_eq!(orgs.remove_member(org, bob.id()).await.unwrap(), MembershipChange::LastOwner);
    assert_eq!(orgs.remove_member(org, alice.id()).await.unwrap(), MembershipChange::Done);

    let members = orgs.members(org).await.unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!((members[0].user.id, members[0].role), (bob.id(), OrgRole::Owner));
}

#[tokio::test]
async fn members_are_listed_owners_first() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let zed = create_user(&database, "zed").await;
    let amy = create_user(&database, "amy").await;
    let bob = create_user(&database, "bob").await;
    let orgs = database.orgs();
    let org = orgs.create(new_org("acme", &zed)).await.unwrap().account.id;
    orgs.add_member(org, bob.id(), OrgRole::Member).await.unwrap();
    orgs.add_member(org, amy.id(), OrgRole::Member).await.unwrap();

    let names: Vec<_> = orgs.members(org).await.unwrap().into_iter().map(|m| m.user.name.to_string()).collect();
    assert_eq!(names, ["zed", "amy", "bob"]);
}

#[tokio::test]
async fn organisations_with_repositories_are_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let orgs = database.orgs();
    let org = orgs.create(new_org("acme", &alice)).await.unwrap().account.id;
    create_repo(&database, org, "rocket", None, Visibility::Private).await;

    assert_eq!(orgs.count_repositories(org).await.unwrap(), 1);
    assert!(!orgs.delete_if_empty(org).await.unwrap());
    assert!(!orgs.delete_if_empty(alice.id()).await.unwrap(), "never deletes users");

    let repo = database.repositories().find_by_path("acme", "rocket").await.unwrap().unwrap();
    database.repositories().delete(repo.repo.id).await.unwrap();
    assert!(orgs.delete_if_empty(org).await.unwrap());
    assert!(orgs.find(org).await.unwrap().is_none());
    assert!(orgs.memberships_of(alice.id()).await.unwrap().is_empty());
}

#[tokio::test]
async fn members_see_their_organisations_repositories() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let org = database.orgs().create(new_org("acme", &alice)).await.unwrap().account.id;
    create_repo(&database, org, "secret", None, Visibility::Private).await;
    create_repo(&database, alice.id(), "dotfiles", None, Visibility::Public).await;

    let repos = database.repositories();
    let names = |list: Vec<rbit_core::OwnedRepository>| list.into_iter().map(|r| r.full_name()).collect::<Vec<_>>();
    let mut alice_sees = names(repos.list_for_member(alice.id(), 100).await.unwrap());
    alice_sees.sort();
    assert_eq!(alice_sees, ["acme/secret", "alice/dotfiles"]);
    assert!(repos.list_for_member(bob.id(), 100).await.unwrap().is_empty());
    assert_eq!(repos.org_role(org, alice.id()).await.unwrap(), Some(OrgRole::Owner));
    assert_eq!(repos.org_role(org, bob.id()).await.unwrap(), None);
}

#[tokio::test]
async fn explore_search_sorts_pages_and_escapes_wildcards() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    create_repo(&database, alice.id(), "beta", Some("A web framework"), Visibility::Public).await;
    create_repo(&database, alice.id(), "alpha", None, Visibility::Public).await;
    create_repo(&database, alice.id(), "Web-site", Some("100% static"), Visibility::Public).await;
    create_repo(&database, alice.id(), "webhidden", None, Visibility::Private).await;
    let repos = database.repositories();
    let names = |list: Vec<rbit_core::OwnedRepository>| list.into_iter().map(|r| r.repo.name.to_string()).collect::<Vec<_>>();

    assert_eq!(repos.count_public("").await.unwrap(), 3);
    assert_eq!(names(repos.search_public("", RepoSort::Name, 10, 0).await.unwrap()), ["alpha", "beta", "Web-site"]);
    assert_eq!(names(repos.search_public("", RepoSort::Newest, 10, 0).await.unwrap()), ["Web-site", "alpha", "beta"]);
    assert_eq!(names(repos.search_public("", RepoSort::Name, 2, 2).await.unwrap()), ["Web-site"]);

    assert_eq!(repos.count_public("WEB").await.unwrap(), 2, "name or description, case-insensitive, public only");
    assert_eq!(names(repos.search_public("web", RepoSort::Name, 10, 0).await.unwrap()), ["beta", "Web-site"]);
    assert_eq!(names(repos.search_public("0%", RepoSort::Name, 10, 0).await.unwrap()), ["Web-site"]);
    assert_eq!(repos.count_public("_").await.unwrap(), 0, "underscore is literal");
}
