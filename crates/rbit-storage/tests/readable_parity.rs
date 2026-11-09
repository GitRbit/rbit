//! SQL listing parity: every listing filtered by the shared `readable_by!` predicate must
//! return exactly the repositories the `rbit-access` policy lets the viewer read, over a
//! fixture that covers every grant the policy combines.

use std::collections::BTreeSet;

use rbit_access::grants::{self, Repository};
use rbit_core::{
    AccountId, OrgRole, RepoId, User, Visibility,
    membership::{BasePermission, Permission},
};
use rbit_storage::{Database, NewRepository, NewUser, sqlite::orgs::NewOrganization};

struct World {
    _dir: tempfile::TempDir,
    database: Database,
    users: Vec<User>,
    owners: Vec<AccountId>,
    /// `owner/name`, id, the policy's repository facts.
    repos: Vec<(String, RepoId, Repository<AccountId>)>,
}

async fn user(database: &Database, name: &str, is_admin: bool) -> User {
    let new = NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin,
    };
    database.accounts().create_user(new).await.unwrap()
}

/// Personal and organisation owners with every base permission; members, a team grant,
/// outside and member collaborators, an organisation owner, a site administrator and a
/// stranger.
async fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let mut users = Vec::new();
    for (name, admin) in [
        ("root", true),
        ("alice", false),
        ("olga", false),
        ("bob", false),
        ("carol", false),
        ("dave", false),
        ("mia", false),
        ("eve", false),
    ] {
        users.push(user(&database, name, admin).await);
    }
    let [_, alice, olga, bob, carol, dave, mia, _] = [0, 1, 2, 3, 4, 5, 6, 7].map(|index| users[index].clone());
    let mut owners = vec![alice.id()];
    let orgs = database.orgs();
    for (name, base) in [("open", BasePermission::Write), ("reading", BasePermission::Read), ("closed", BasePermission::None)] {
        let new = NewOrganization { name: name.parse().unwrap(), display_name: None, description: None, creator: olga.id() };
        let org = orgs.create(new).await.unwrap().account.id;
        orgs.set_base_permission(org, base).await.unwrap();
        orgs.add_member(org, bob.id(), OrgRole::Member).await.unwrap();
        owners.push(org);
    }
    let closed = owners[3];
    orgs.add_member(closed, dave.id(), OrgRole::Member).await.unwrap();
    orgs.add_member(closed, mia.id(), OrgRole::Member).await.unwrap();
    let team = database.teams().create(closed, &"core".parse().unwrap(), None).await.unwrap().id;
    database.teams().add_member(team, dave.id()).await.unwrap();

    let mut repos = Vec::new();
    for (owner, owner_name) in owners.iter().zip(["alice", "open", "reading", "closed"]) {
        for (name, visibility) in [("pub", Visibility::Public), ("priv", Visibility::Private), ("team", Visibility::Private)] {
            let new = NewRepository {
                owner_id: *owner,
                name: name.parse().unwrap(),
                description: None,
                visibility,
                default_branch: "main".to_owned(),
            };
            let id = database.repositories().create(new).await.unwrap().id;
            let facts = Repository { owner: *owner, public: visibility.is_public(), organization: *owner != alice.id() };
            if name == "team" && *owner == closed {
                database.teams().set_repository(team, id, Permission::Read).await.unwrap();
            }
            if name == "priv" {
                database.collaborators().add(id, carol.id(), Permission::Read, olga.id()).await.unwrap();
            }
            if name == "team" && *owner == closed {
                database.collaborators().add(id, mia.id(), Permission::Write, olga.id()).await.unwrap();
            }
            repos.push((format!("{owner_name}/{name}"), id, facts));
        }
    }
    World { _dir: dir, database, users, owners, repos }
}

/// What the policy lets `viewer` read, with grants loaded through the access port.
async fn readable(world: &World, viewer: Option<&User>) -> BTreeSet<String> {
    let store = world.database.repositories();
    let viewer = viewer.map(rbit_core::access::viewer);
    let mut names = BTreeSet::new();
    for (name, id, facts) in &world.repos {
        let grants = grants::viewer_grants(&store, viewer, *id, facts.owner).await.unwrap();
        if grants::access(viewer.as_ref(), facts, &grants).can_read() {
            names.insert(name.clone());
        }
    }
    names
}

fn full_names<'a>(repos: impl IntoIterator<Item = &'a rbit_core::repository::OwnedRepository>) -> BTreeSet<String> {
    repos.into_iter().map(|repo| format!("{}/{}", repo.owner.name, repo.repo.name)).collect()
}

#[tokio::test]
async fn every_readable_by_listing_matches_the_policy() {
    let world = world().await;
    let store = world.database.repositories();
    let ids: Vec<RepoId> = world.repos.iter().map(|(_, id, _)| *id).collect();
    let viewers: Vec<Option<&User>> = std::iter::once(None).chain(world.users.iter().map(Some)).collect();
    let mut distinct = BTreeSet::new();
    for viewer in viewers {
        let label = viewer.map_or("anonymous".to_owned(), |user| user.name().to_string());
        let expected = readable(&world, viewer).await;
        distinct.insert(expected.clone());
        let (id, admin) = (viewer.map(User::id), viewer.is_some_and(|user| user.is_admin));

        let contributions = store.contribution_repositories(id, admin).await.unwrap();
        assert_eq!(full_names(&contributions), expected, "contribution_repositories for {label}");

        let mut by_owner = BTreeSet::new();
        for owner in &world.owners {
            by_owner.extend(full_names(&store.list_by_owner(*owner, id, admin).await.unwrap()));
        }
        assert_eq!(by_owner, expected, "list_by_owner for {label}");

        if let Some(user) = viewer {
            let among = store.readable_among(&ids, user.id(), user.is_admin).await.unwrap();
            assert_eq!(full_names(among.values()), expected, "readable_among for {label}");
        }
    }
    // Nine viewers, seven different sets (the stranger reads what anonymous visitors do;
    // the team member and the member collaborator reach the same repository), so the
    // agreement is not vacuous.
    assert_eq!(distinct.len(), 7, "{distinct:#?}");
}
