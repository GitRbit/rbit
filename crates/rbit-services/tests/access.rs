//! Repository access through collaborators, teams and the organisation base permission,
//! and agreement between the pure policy (via `RepositoryService::open`) and the SQL
//! readable-by predicate used by every listing.

use std::{collections::BTreeSet, sync::Arc};

use rbit_core::{
    Account, Config, OrgRole, RepoAccess, User, Visibility,
    membership::{BasePermission, Permission},
};
use rbit_services::{CreateRepository, ServiceError, Services, orgs::CreateOrganization};
use rbit_storage::{Database, KvStore, NewUser};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { _dir: dir, database, services }
    }

    async fn user(&self, name: &str, is_admin: bool) -> User {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin,
            })
            .await
            .unwrap()
    }

    async fn org(&self, owner: &User, name: &str, base: BasePermission) -> Account {
        let input = CreateOrganization { name: name.to_owned(), ..CreateOrganization::default() };
        let org = self.services.orgs.create(owner, input).await.unwrap();
        self.database.orgs().set_base_permission(org.account.id, base).await.unwrap();
        org.account
    }

    async fn repo(&self, actor: &User, owner: &str, name: &str, visibility: Visibility) -> rbit_core::RepoId {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility };
        self.services.repositories.create_in(actor, owner, input).await.unwrap().repo.id
    }

    async fn open_as(&self, viewer: &User, owner: &str, name: &str) -> rbit_services::RepoContext {
        self.services.repositories.open(Some(viewer), owner, name).await.unwrap()
    }

    async fn access(&self, viewer: Option<&User>, owner: &str, name: &str) -> RepoAccess {
        match self.services.repositories.open(viewer, owner, name).await {
            Ok(ctx) => ctx.access,
            Err(ServiceError::NotFound) => RepoAccess::None,
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}

fn names(repos: &[rbit_core::OwnedRepository]) -> BTreeSet<String> {
    repos.iter().map(rbit_core::OwnedRepository::full_name).collect()
}

#[tokio::test]
async fn grants_combine_by_maximum() {
    let fixture = Fixture::new().await;
    let olga = fixture.user("olga", false).await;
    let bob = fixture.user("bob", false).await;
    let carol = fixture.user("carol", false).await;
    let closed = fixture.org(&olga, "closed", BasePermission::None).await;
    fixture
        .services
        .orgs
        .add_member(&olga, &fixture.services.orgs.open(None, "closed").await.unwrap().org, "bob", OrgRole::Member)
        .await
        .unwrap();
    let vault = fixture.repo(&olga, "closed", "vault", Visibility::Private).await;
    fixture.repo(&olga, "closed", "brochure", Visibility::Public).await;

    // Base permission `none`: members see public repositories only.
    assert_eq!(fixture.access(Some(&bob), "closed", "vault").await, RepoAccess::None);
    assert_eq!(fixture.access(Some(&bob), "closed", "brochure").await, RepoAccess::Read);
    fixture.database.orgs().set_base_permission(closed.id, BasePermission::Read).await.unwrap();
    assert_eq!(fixture.access(Some(&bob), "closed", "vault").await, RepoAccess::Read);

    // A team grant adds to the base permission.
    let team = fixture.database.teams().create(closed.id, &"core".parse().unwrap(), None).await.unwrap();
    fixture.database.teams().add_member(team.id, bob.id()).await.unwrap();
    fixture.database.teams().set_repository(team.id, vault, Permission::Admin).await.unwrap();
    assert_eq!(fixture.access(Some(&bob), "closed", "vault").await, RepoAccess::Admin);
    fixture.database.teams().set_repository(team.id, vault, Permission::Write).await.unwrap();
    assert_eq!(fixture.access(Some(&bob), "closed", "vault").await, RepoAccess::Write);

    // Outside collaborators get exactly their permission.
    assert_eq!(fixture.access(Some(&carol), "closed", "vault").await, RepoAccess::None);
    fixture.database.collaborators().add(vault, carol.id(), Permission::Read, olga.id()).await.unwrap();
    assert_eq!(fixture.access(Some(&carol), "closed", "vault").await, RepoAccess::Read);
    fixture.database.collaborators().set_permission(vault, carol.id(), Permission::Admin).await.unwrap();
    assert_eq!(fixture.access(Some(&carol), "closed", "vault").await, RepoAccess::Admin);
    assert_eq!(fixture.access(None, "closed", "vault").await, RepoAccess::None);
}

#[tokio::test]
async fn members_administer_repositories_they_create_until_they_leave() {
    let fixture = Fixture::new().await;
    let olga = fixture.user("olga", false).await;
    let bob = fixture.user("bob", false).await;
    fixture.org(&olga, "closed", BasePermission::None).await;
    let org = fixture.services.orgs.open(None, "closed").await.unwrap().org;
    fixture.services.orgs.add_member(&olga, &org, "bob", OrgRole::Member).await.unwrap();

    fixture.repo(&bob, "closed", "sidecar", Visibility::Private).await;
    assert_eq!(fixture.access(Some(&bob), "closed", "sidecar").await, RepoAccess::Admin);
    assert_eq!(fixture.access(Some(&olga), "closed", "sidecar").await, RepoAccess::Admin);

    fixture.services.orgs.remove_member(&olga, &org, "bob").await.unwrap();
    assert_eq!(fixture.access(Some(&bob), "closed", "sidecar").await, RepoAccess::None, "leaving revokes the creator's entry");
}

/// Every listing filtered in SQL must show exactly the repositories `open` lets the viewer read.
#[tokio::test]
async fn sql_listings_agree_with_the_policy() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let olga = fixture.user("olga", false).await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let carol = fixture.user("carol", false).await;
    let dave = fixture.user("dave", false).await;
    let eve = fixture.user("eve", false).await;
    let fan = fixture.user("fan", false).await;

    let open_org = fixture.org(&olga, "open", BasePermission::Write).await;
    let reading = fixture.org(&olga, "reading", BasePermission::Read).await;
    let closed = fixture.org(&olga, "closed", BasePermission::None).await;
    let owners: Vec<Account> = vec![alice.account.clone(), open_org.clone(), reading.clone(), closed.clone()];

    let mut repos = Vec::new();
    for owner in &owners {
        let actor = if owner.is_organization() { &olga } else { &alice };
        for (name, visibility) in [("pub", Visibility::Public), ("priv", Visibility::Private), ("team", Visibility::Private)] {
            repos.push((owner.name.to_string(), name, fixture.repo(actor, owner.name.as_str(), name, visibility).await));
        }
    }

    // Memberships: bob in every organisation, dave in `closed` with a team; carol is an
    // outside collaborator; eve has nothing.
    for org in [&open_org, &reading, &closed] {
        let org = fixture.services.orgs.open(None, org.name.as_str()).await.unwrap().org;
        fixture.services.orgs.add_member(&olga, &org, "bob", OrgRole::Member).await.unwrap();
    }
    let closed_org = fixture.services.orgs.open(None, "closed").await.unwrap().org;
    fixture.services.orgs.add_member(&olga, &closed_org, "dave", OrgRole::Member).await.unwrap();
    let team = fixture.database.teams().create(closed.id, &"core".parse().unwrap(), None).await.unwrap();
    fixture.database.teams().add_member(team.id, dave.id()).await.unwrap();
    for (owner, name, id) in &repos {
        if *name == "team" && owner == "closed" {
            fixture.database.teams().set_repository(team.id, *id, Permission::Read).await.unwrap();
        }
        if *name == "priv" && (owner == "alice" || owner == "closed") {
            fixture.database.collaborators().add(*id, carol.id(), Permission::Read, olga.id()).await.unwrap();
        }
        fixture.database.stars().add(fan.id(), *id).await.unwrap();
    }

    let fan_account = fan.account.clone();
    let viewers = [None, Some(&root), Some(&olga), Some(&alice), Some(&bob), Some(&carol), Some(&dave), Some(&eve), Some(&fan)];
    assert_bulk_grants(&fixture, &repos, &viewers).await;
    assert_personal_dashboard_visibility(&fixture, &repos, &viewers, &alice).await;
    for viewer in viewers {
        let label = viewer.map_or("anonymous".to_owned(), |user| user.name().to_string());
        let mut readable = BTreeSet::new();
        for (owner, name, _) in &repos {
            if fixture.access(viewer, owner, name).await.can_read() {
                readable.insert(format!("{owner}/{name}"));
            }
        }

        let mut listed = BTreeSet::new();
        for owner in &owners {
            listed.extend(names(&fixture.services.repositories.list_for_owner(viewer, owner).await.unwrap()));
        }
        assert_eq!(listed, readable, "list_for_owner for {label}");

        let starred = names(&fixture.services.stars.starred_by(viewer, &fan_account).await.unwrap());
        assert_eq!(starred, readable, "starred_by for {label}");

        if let Some(user) = viewer {
            let memberships = fixture.services.orgs.memberships(user).await.unwrap();
            let mut expected = BTreeSet::new();
            for (owner, name, id) in &repos {
                let related = owner == user.name().as_str()
                    || memberships.iter().any(|m| m.org.name.as_str() == owner)
                    || fixture.database.collaborators().permission(*id, user.id()).await.unwrap().is_some();
                if related && readable.contains(&format!("{owner}/{name}")) {
                    expected.insert(format!("{owner}/{name}"));
                }
            }
            let dashboard = names(&fixture.services.repositories.list_for_dashboard(user).await.unwrap());
            assert_eq!(dashboard, expected, "dashboard for {label}");
        }
    }

    // Spot checks, so the agreement above is not vacuous.
    assert_eq!(fixture.access(Some(&bob), "reading", "priv").await, RepoAccess::Read);
    assert_eq!(fixture.access(Some(&bob), "closed", "priv").await, RepoAccess::None);
    assert_eq!(fixture.access(Some(&dave), "closed", "team").await, RepoAccess::Read);
    assert_eq!(fixture.access(Some(&carol), "alice", "priv").await, RepoAccess::Read);
    assert_eq!(fixture.access(Some(&eve), "open", "priv").await, RepoAccess::None);
    let dashboard = names(&fixture.services.repositories.list_for_dashboard(&carol).await.unwrap());
    assert_eq!(dashboard, BTreeSet::from(["alice/priv".to_owned(), "closed/priv".to_owned()]), "shared repositories reach the dashboard");
}

/// Bulk lookup and single-viewer authorization must agree for the fixture's full
/// matrix, with duplicate and unknown IDs spanning multiple query batches.
async fn assert_bulk_grants(fixture: &Fixture, repos: &[(String, &str, rbit_core::RepoId)], viewers: &[Option<&User>]) {
    let mut viewer_ids: Vec<_> = viewers.iter().flatten().map(|user| user.id()).collect();
    viewer_ids.extend((10_000..10_300).map(rbit_core::AccountId));
    viewer_ids.push(viewer_ids[0]);
    let store = fixture.database.repositories();
    for (owner, name, id) in repos {
        let owned = store.find_by_path(owner, name).await.unwrap().unwrap();
        let batch = store.grants_for(*id, owned.owner.id, &viewer_ids).await.unwrap();
        assert_eq!(batch.len(), viewer_ids.len() - 1);
        for viewer in viewers.iter().flatten() {
            assert_eq!(batch[&viewer.id()], store.grants(*id, owned.owner.id, viewer.id()).await.unwrap());
        }
    }
}

fn invalid(result: Result<(), ServiceError>, needle: &str) {
    match result {
        Err(ServiceError::Invalid(message)) => assert!(message.contains(needle), "{message:?} should mention {needle:?}"),
        other => panic!("expected a validation error mentioning {needle:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn administrators_manage_collaborators() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let carol = fixture.user("carol", false).await;
    let mallory = fixture.user("mallory", false).await;
    let root = fixture.user("root", true).await;
    fixture.repo(&alice, "", "project", Visibility::Private).await;
    let collaborators = &fixture.services.collaborators;
    let as_alice = fixture.open_as(&alice, "alice", "project").await;

    collaborators.add(&alice, &as_alice, " bob ", Permission::Write).await.unwrap();
    invalid(collaborators.add(&alice, &as_alice, "bob", Permission::Read).await, "already a collaborator");
    invalid(collaborators.add(&alice, &as_alice, "alice", Permission::Read).await, "owns this repository");
    invalid(collaborators.add(&alice, &as_alice, "ghost", Permission::Read).await, "no user called ghost");
    invalid(collaborators.add(&alice, &as_alice, "", Permission::Read).await, "enter a username");
    fixture.database.admin().set_suspended(mallory.id(), true).await.unwrap();
    invalid(collaborators.add(&alice, &as_alice, "mallory", Permission::Read).await, "suspended");
    let org = CreateOrganization { name: "acme".into(), ..CreateOrganization::default() };
    fixture.services.orgs.create(&alice, org).await.unwrap();
    invalid(collaborators.add(&alice, &as_alice, "acme", Permission::Read).await, "no user called acme");

    // Bob can push but not administer: he can neither see nor change the collaborator list.
    let as_bob = fixture.open_as(&bob, "alice", "project").await;
    assert_eq!(as_bob.access, RepoAccess::Write);
    assert!(matches!(collaborators.list(&as_bob).await, Err(ServiceError::Forbidden)));
    assert!(matches!(collaborators.add(&bob, &as_bob, "carol", Permission::Admin).await, Err(ServiceError::Forbidden)));
    assert_eq!(collaborators.permission_of(&bob, &as_bob).await.unwrap(), Some(Permission::Write));
    assert_eq!(as_bob.collaborator, Some(Permission::Write), "read with the grants that decided access");

    // Promoted to admin, he may.
    collaborators.change(&alice, &as_alice, "bob", Permission::Admin).await.unwrap();
    let as_bob = fixture.open_as(&bob, "alice", "project").await;
    assert_eq!(as_bob.access, RepoAccess::Admin);
    collaborators.add(&bob, &as_bob, "carol", Permission::Read).await.unwrap();
    invalid(collaborators.change(&alice, &as_alice, "root", Permission::Read).await, "not a collaborator");

    let listed = collaborators.list(&as_alice).await.unwrap();
    let rows: Vec<_> = listed.collaborators.iter().map(|c| (c.user.name.to_string(), c.permission)).collect();
    assert_eq!(rows, [("bob".to_owned(), Permission::Admin), ("carol".to_owned(), Permission::Read)]);
    assert!(listed.teams.is_empty(), "personal repositories have no teams");

    // Carol (read) may not remove Bob, but may leave herself.
    let as_carol = fixture.open_as(&carol, "alice", "project").await;
    assert!(matches!(collaborators.remove(&carol, &as_carol, "bob").await, Err(ServiceError::Forbidden)));
    collaborators.remove(&carol, &as_carol, "carol").await.unwrap();
    assert_eq!(fixture.access(Some(&carol), "alice", "project").await, RepoAccess::None);
    // Site admins administer everything anyway.
    let as_root = fixture.open_as(&root, "alice", "project").await;
    collaborators.remove(&root, &as_root, "bob").await.unwrap();
    assert_eq!(fixture.access(Some(&bob), "alice", "project").await, RepoAccess::None);
    invalid(collaborators.remove(&alice, &as_alice, "bob").await, "not a collaborator");

    // Every change is audited.
    let log = fixture.services.audit.list(&root, None, &rbit_services::admin::ListQuery::default()).await.unwrap();
    let actions: Vec<_> = log.items.iter().map(|event| event.action.as_str()).collect();
    for expected in ["collaborator_added", "collaborator_changed", "collaborator_removed"] {
        assert!(actions.contains(&expected), "{expected} in {actions:?}");
    }
}

#[tokio::test]
async fn organisation_repositories_list_team_grants() {
    let fixture = Fixture::new().await;
    let olga = fixture.user("olga", false).await;
    fixture.org(&olga, "acme", BasePermission::Write).await;
    let rocket = fixture.repo(&olga, "acme", "rocket", Visibility::Private).await;
    let acme = fixture.services.orgs.open(None, "acme").await.unwrap().org;
    let team = fixture.database.teams().create(acme.account.id, &"core".parse().unwrap(), None).await.unwrap();
    fixture.database.teams().set_repository(team.id, rocket, Permission::Write).await.unwrap();

    let ctx = fixture.services.repositories.open(Some(&olga), "acme", "rocket").await.unwrap();
    let listed = fixture.services.collaborators.list(&ctx).await.unwrap();
    assert_eq!(listed.teams.len(), 1);
    assert_eq!((listed.teams[0].team.name.as_str(), listed.teams[0].permission), ("core", Permission::Write));
}

#[tokio::test]
async fn collaborators_are_set_idempotently_and_access_is_explained() {
    let fixture = Fixture::new().await;
    let olga = fixture.user("olga", false).await;
    let bob = fixture.user("bob", false).await;
    fixture.user("carol", false).await;
    fixture.user("root", true).await;
    fixture.org(&olga, "acme", BasePermission::Read).await;
    let acme = fixture.services.orgs.open(None, "acme").await.unwrap().org;
    fixture.services.orgs.add_member(&olga, &acme, "carol", OrgRole::Member).await.unwrap();
    fixture.repo(&olga, "acme", "rocket", Visibility::Private).await;
    let collaborators = &fixture.services.collaborators;
    let as_olga = fixture.open_as(&olga, "acme", "rocket").await;

    assert!(collaborators.set(&olga, &as_olga, "bob", Permission::Read).await.unwrap(), "added");
    assert!(!collaborators.set(&olga, &as_olga, "bob", Permission::Read).await.unwrap(), "unchanged");
    assert!(!collaborators.set(&olga, &as_olga, "bob", Permission::Write).await.unwrap(), "changed");
    assert_eq!(fixture.access(Some(&bob), "acme", "rocket").await, RepoAccess::Write);
    invalid(collaborators.set(&olga, &as_olga, "ghost", Permission::Read).await.map(|_| ()), "no user called ghost");

    // Every grant counts: the collaborator entry, the base permission, ownership, site administration.
    for (name, expected) in
        [("bob", RepoAccess::Write), ("carol", RepoAccess::Read), ("olga", RepoAccess::Admin), ("root", RepoAccess::Admin)]
    {
        let (account, access) = collaborators.access_of(&as_olga, name).await.unwrap();
        assert_eq!((account.name.as_str(), access), (name, expected));
    }
    invalid(collaborators.access_of(&as_olga, "ghost").await.map(|_| ()), "no user called ghost");

    // Only administrators may ask or set.
    let as_bob = fixture.open_as(&bob, "acme", "rocket").await;
    assert!(matches!(collaborators.access_of(&as_bob, "carol").await, Err(ServiceError::Forbidden)));
    assert!(matches!(collaborators.set(&bob, &as_bob, "carol", Permission::Admin).await, Err(ServiceError::Forbidden)));
}

/// Personal dashboard rows and counts use the same complete permission matrix.
async fn assert_personal_dashboard_visibility(
    fixture: &Fixture,
    repos: &[(String, &str, rbit_core::RepoId)],
    viewers: &[Option<&User>],
    author: &User,
) {
    let ids: Vec<_> = viewers.iter().flatten().map(|viewer| viewer.id()).collect();
    for (_, _, repo) in repos {
        let issue = fixture
            .database
            .issues()
            .create(&rbit_storage::sqlite::issues::NewIssue {
                repo: *repo,
                kind: rbit_core::IssueKind::Issue,
                title: rbit_core::IssueTitle::parse("Mentioned report").unwrap(),
                body: rbit_core::IssueText::body("").unwrap(),
                author: author.id(),
                at: rbit_core::Timestamp(1),
            })
            .await
            .unwrap();
        fixture.database.issue_search().record_mentions(issue.id, &ids).await.unwrap();
    }
    for viewer in viewers.iter().flatten() {
        let mut expected = BTreeSet::new();
        for (owner, name, id) in repos {
            if fixture.access(Some(viewer), owner, name).await.can_read() {
                expected.insert(id.0);
            }
        }
        let results = fixture
            .services
            .issues
            .dashboard(
                viewer,
                rbit_core::IssueKind::Issue,
                rbit_services::issues::DashboardRelation::Mentioned,
                &rbit_core::issue_query::IssueQuery::parse("is:open"),
                1,
            )
            .await
            .unwrap();
        assert_eq!(results.total, expected.len() as u64);
        assert_eq!(results.open, expected.len() as u64);
        assert_eq!(results.issues.iter().map(|issue| issue.repo_id.0).collect::<BTreeSet<_>>(), expected);
    }
}

#[tokio::test]
async fn personal_dashboards_drop_private_threads_after_membership_revocation() {
    let fixture = Fixture::new().await;
    let olga = fixture.user("olga", false).await;
    let bob = fixture.user("bob", false).await;
    let org = fixture.org(&olga, "closed", BasePermission::Read).await;
    fixture.database.orgs().add_member(org.id, bob.id(), OrgRole::Member).await.unwrap();
    let repo = fixture.repo(&olga, "closed", "vault", Visibility::Private).await;
    let context = fixture.open_as(&bob, "closed", "vault").await;
    fixture
        .services
        .issues
        .create(&bob, &context, rbit_services::issues::NewIssueInput { title: "My private report".into(), ..Default::default() })
        .await
        .unwrap();
    let query = rbit_core::issue_query::IssueQuery::parse("is:open");
    let relation = rbit_services::issues::DashboardRelation::Created;
    let results = fixture.services.issues.dashboard(&bob, rbit_core::IssueKind::Issue, relation, &query, 1).await.unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.repositories[&repo].full_name(), "closed/vault", "each row's repository comes with it");
    fixture.database.orgs().remove_member(org.id, bob.id()).await.unwrap();
    let results = fixture.services.issues.dashboard(&bob, rbit_core::IssueKind::Issue, relation, &query, 1).await.unwrap();
    assert_eq!(results.total, 0);
    assert_eq!(results.open, 0);
    assert_eq!(results.issues.len(), 0);
    assert!(!results.repositories.contains_key(&repo));
}
