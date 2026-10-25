//! Organisation use-cases and membership-aware repository access against real, temporary
//! `SQLite`/redb stores and bare repositories.

use std::sync::Arc;

use rbit_core::{Config, OrgCreationPolicy, OrgRole, Organization, RepoAccess, RepoSort, User, Visibility};
use rbit_services::{
    CreateRepository, ServiceError, Services,
    orgs::{CreateOrganization, UpdateOrganization},
    repositories::RepoSearch,
};
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

    /// Inserts a user directly; password hashing is irrelevant here.
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

    async fn org(&self, owner: &User, name: &str) -> Organization {
        let input = CreateOrganization { name: name.to_owned(), ..CreateOrganization::default() };
        self.services.orgs.create(owner, input).await.unwrap()
    }

    async fn repo_in(&self, actor: &User, owner: &str, name: &str, visibility: Visibility) -> Result<(), ServiceError> {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility };
        self.services.repositories.create_in(actor, owner, input).await.map(|_| ())
    }

    async fn access(&self, viewer: Option<&User>, owner: &str, name: &str) -> RepoAccess {
        match self.services.repositories.open(viewer, owner, name).await {
            Ok(ctx) => ctx.access,
            Err(ServiceError::NotFound) => RepoAccess::None,
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    async fn set_org_creation(&self, admin: &User, policy: OrgCreationPolicy) {
        let mut settings = self.services.instance.current();
        settings.org_creation = policy;
        self.services.instance.update(admin, settings).await.unwrap();
    }
}

fn invalid<T>(result: Result<T, ServiceError>) -> bool {
    result.is_err_and(|error| matches!(error, ServiceError::Invalid(_)))
}

#[tokio::test]
async fn create_makes_the_creator_owner() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let input = CreateOrganization { name: " acme ".into(), display_name: " Acme Inc. ".into(), description: "Rockets".into() };
    let acme = fixture.services.orgs.create(&alice, input).await.unwrap();
    assert_eq!(acme.name().as_str(), "acme");
    assert_eq!(acme.label(), "Acme Inc.");
    assert_eq!(acme.description.as_deref(), Some("Rockets"));

    let ctx = fixture.services.orgs.open(Some(&alice), "ACME").await.unwrap();
    assert_eq!(ctx.role, Some(OrgRole::Owner));
    assert!(ctx.can_manage);
    let memberships = fixture.services.orgs.memberships(&alice).await.unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].role, OrgRole::Owner);

    let anonymous = fixture.services.orgs.open(None, "acme").await.unwrap();
    assert!(anonymous.role.is_none() && !anonymous.can_manage);
    assert!(matches!(fixture.services.orgs.open(None, "alice").await, Err(ServiceError::NotFound)), "users are not organisations");
}

#[tokio::test]
async fn create_validates_and_shares_the_user_namespace() {
    {
        eprintln!("organization namespace validation: fixture initialization begin");
        let fixture = Fixture::new().await;
        eprintln!("organization namespace validation: fixture initialization complete");
        eprintln!("organization namespace validation: user insert begin");
        let alice = fixture.user("alice", false).await;
        eprintln!("organization namespace validation: user insert complete");
        let create = |name: &str| CreateOrganization { name: name.to_owned(), ..CreateOrganization::default() };

        assert!(invalid(fixture.services.orgs.create(&alice, create("")).await));
        assert!(invalid(fixture.services.orgs.create(&alice, create("bad name")).await));
        assert!(invalid(fixture.services.orgs.create(&alice, create("settings")).await));
        assert!(matches!(
            {
                eprintln!("organization namespace validation: duplicate name await begin");
                let result = fixture.services.orgs.create(&alice, create("Alice")).await;
                eprintln!("organization namespace validation: duplicate name await complete");
                result
            },
            Err(ServiceError::Conflict("name"))
        ));
        let long = CreateOrganization { name: "ok".into(), display_name: "x".repeat(101), ..CreateOrganization::default() };
        assert!(invalid(fixture.services.orgs.create(&alice, long).await));
        eprintln!("organization namespace validation: assertions complete; leaving fixture scope");
    }
    eprintln!("organization namespace validation: fixture scope teardown complete");
}

#[tokio::test]
async fn creation_can_be_limited_to_admins() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    fixture.set_org_creation(&root, OrgCreationPolicy::AdminsOnly).await;

    assert!(!fixture.services.orgs.can_create(&alice));
    let refused = fixture.services.orgs.create(&alice, CreateOrganization { name: "acme".into(), ..Default::default() }).await;
    assert!(matches!(refused, Err(ServiceError::Forbidden)));
    assert!(fixture.services.orgs.can_create(&root));
    fixture.org(&root, "acme").await;
}

#[tokio::test]
async fn owners_manage_members() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let carol = fixture.user("carol", false).await;
    let acme = fixture.org(&alice, "acme").await;
    let orgs = &fixture.services.orgs;

    orgs.add_member(&alice, &acme, " bob ", OrgRole::Member).await.unwrap();
    assert!(invalid(orgs.add_member(&alice, &acme, "bob", OrgRole::Owner).await), "already a member");
    assert!(invalid(orgs.add_member(&alice, &acme, "nobody", OrgRole::Member).await));
    assert!(invalid(orgs.add_member(&alice, &acme, "acme", OrgRole::Member).await), "organisations cannot join");

    // Members cannot manage; strangers are told nothing.
    assert!(matches!(orgs.add_member(&bob, &acme, "carol", OrgRole::Member).await, Err(ServiceError::Forbidden)));
    assert!(matches!(orgs.remove_member(&carol, &acme, "bob").await, Err(ServiceError::NotFound)));

    orgs.change_role(&alice, &acme, "bob", OrgRole::Owner).await.unwrap();
    orgs.add_member(&bob, &acme, "carol", OrgRole::Member).await.unwrap();
    orgs.remove_member(&bob, &acme, "carol").await.unwrap();
    assert!(invalid(orgs.remove_member(&bob, &acme, "carol").await), "no longer a member");

    let members: Vec<_> = orgs.members(&acme).await.unwrap().into_iter().map(|m| (m.user.name.to_string(), m.role)).collect();
    assert_eq!(members, [("alice".to_owned(), OrgRole::Owner), ("bob".to_owned(), OrgRole::Owner)]);
}

#[tokio::test]
async fn the_last_owner_stays() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let acme = fixture.org(&alice, "acme").await;
    let orgs = &fixture.services.orgs;
    orgs.add_member(&alice, &acme, "bob", OrgRole::Member).await.unwrap();

    assert!(invalid(orgs.change_role(&alice, &acme, "alice", OrgRole::Member).await));
    assert!(invalid(orgs.remove_member(&alice, &acme, "alice").await));
    assert!(invalid(orgs.leave(&alice, &acme).await));

    orgs.leave(&bob, &acme).await.unwrap();
    assert!(matches!(orgs.leave(&bob, &acme).await, Err(ServiceError::NotFound)));

    orgs.add_member(&alice, &acme, "bob", OrgRole::Owner).await.unwrap();
    orgs.leave(&alice, &acme).await.unwrap();
    assert_eq!(orgs.open(Some(&bob), "acme").await.unwrap().role, Some(OrgRole::Owner));
}

#[tokio::test]
async fn site_admins_manage_any_organisation() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    let acme = fixture.org(&alice, "acme").await;

    let ctx = fixture.services.orgs.open(Some(&root), "acme").await.unwrap();
    assert!(ctx.can_manage && ctx.role.is_none());
    fixture.services.orgs.add_member(&root, &acme, "root", OrgRole::Member).await.unwrap();
}

#[tokio::test]
async fn profile_updates_are_validated() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let acme = fixture.org(&alice, "acme").await;
    let orgs = &fixture.services.orgs;

    let update = |website: &str| UpdateOrganization { display_name: "Acme".into(), description: "Rockets".into(), website: website.into() };
    assert!(invalid(orgs.update_profile(&alice, &acme, update("javascript:alert(1)")).await));
    assert!(matches!(orgs.update_profile(&bob, &acme, update("")).await, Err(ServiceError::NotFound)));
    orgs.update_profile(&alice, &acme, update("https://acme.example")).await.unwrap();

    let org = orgs.open(None, "acme").await.unwrap().org;
    assert_eq!(org.label(), "Acme");
    assert_eq!(org.website.as_deref(), Some("https://acme.example"));

    orgs.update_profile(&alice, &acme, UpdateOrganization::default()).await.unwrap();
    let org = orgs.open(None, "acme").await.unwrap().org;
    assert_eq!((org.label(), org.description.as_deref(), org.website.as_deref()), ("acme", None, None));
}

#[tokio::test]
async fn delete_requires_confirmation_and_no_repositories() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let acme = fixture.org(&alice, "acme").await;
    fixture.repo_in(&alice, "acme", "rocket", Visibility::Public).await.unwrap();
    let orgs = &fixture.services.orgs;

    assert!(invalid(orgs.delete(&alice, &acme, "wrong").await));
    let Err(ServiceError::Invalid(message)) = orgs.delete(&alice, &acme, "acme").await else { panic!("expected refusal") };
    assert!(message.contains("1 repository"), "{message}");

    let repo = fixture.database.repositories().find_by_path("acme", "rocket").await.unwrap().unwrap();
    fixture.database.repositories().delete(repo.repo.id).await.unwrap();
    orgs.delete(&alice, &acme, "ACME").await.unwrap();
    assert!(matches!(orgs.open(None, "acme").await, Err(ServiceError::NotFound)));
    assert!(orgs.memberships(&alice).await.unwrap().is_empty());
}

#[tokio::test]
async fn only_members_create_repositories_in_an_organisation() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let eve = fixture.user("eve", false).await;
    let acme = fixture.org(&alice, "acme").await;
    fixture.services.orgs.add_member(&alice, &acme, "bob", OrgRole::Member).await.unwrap();

    fixture.repo_in(&bob, "acme", "by-member", Visibility::Public).await.unwrap();
    fixture.repo_in(&bob, "", "personal", Visibility::Public).await.unwrap();
    assert!(invalid(fixture.repo_in(&eve, "acme", "intruder", Visibility::Public).await));
    assert!(invalid(fixture.repo_in(&eve, "alice", "intruder", Visibility::Public).await), "never another user");
    assert!(invalid(fixture.repo_in(&eve, "ghost", "intruder", Visibility::Public).await));

    let created = fixture.services.repositories.open(Some(&bob), "acme", "by-member").await.unwrap();
    assert_eq!(created.repo.owner.id, acme.account.id);
    assert!(created.disk_path.exists());
}

#[tokio::test]
async fn repository_access_follows_membership() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let eve = fixture.user("eve", false).await;
    let root = fixture.user("root", true).await;
    let acme = fixture.org(&alice, "acme").await;
    fixture.services.orgs.add_member(&alice, &acme, "bob", OrgRole::Member).await.unwrap();
    fixture.repo_in(&alice, "acme", "secret", Visibility::Private).await.unwrap();
    fixture.repo_in(&alice, "acme", "open", Visibility::Public).await.unwrap();

    assert_eq!(fixture.access(Some(&alice), "acme", "secret").await, RepoAccess::Admin);
    assert_eq!(fixture.access(Some(&bob), "acme", "secret").await, RepoAccess::Write);
    assert_eq!(fixture.access(Some(&root), "acme", "secret").await, RepoAccess::Admin);
    assert_eq!(fixture.access(Some(&eve), "acme", "secret").await, RepoAccess::None);
    assert_eq!(fixture.access(None, "acme", "secret").await, RepoAccess::None);
    assert_eq!(fixture.access(Some(&eve), "acme", "open").await, RepoAccess::Read);

    let names = |repos: Vec<rbit_core::OwnedRepository>| {
        let mut names: Vec<_> = repos.into_iter().map(|r| r.full_name()).collect();
        names.sort();
        names
    };
    let repos = &fixture.services.repositories;
    assert_eq!(names(repos.list_for_owner(Some(&bob), &acme.account).await.unwrap()), ["acme/open", "acme/secret"]);
    assert_eq!(names(repos.list_for_owner(Some(&eve), &acme.account).await.unwrap()), ["acme/open"]);
    assert_eq!(names(repos.list_for_owner(None, &acme.account).await.unwrap()), ["acme/open"]);
    assert_eq!(names(repos.list_for_dashboard(&bob).await.unwrap()), ["acme/open", "acme/secret"]);
    assert!(repos.list_for_dashboard(&eve).await.unwrap().is_empty());

    // Leaving revokes access immediately.
    fixture.services.orgs.leave(&bob, &acme).await.unwrap();
    assert_eq!(fixture.access(Some(&bob), "acme", "secret").await, RepoAccess::None);
}

#[tokio::test]
async fn explore_searches_public_repositories_by_page() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    for n in 0..35 {
        fixture.repo_in(&alice, "", &format!("repo-{n:02}"), Visibility::Public).await.unwrap();
    }
    fixture.repo_in(&alice, "", "hidden", Visibility::Private).await.unwrap();
    let repos = &fixture.services.repositories;

    let first = repos.explore(&RepoSearch { text: String::new(), sort: RepoSort::Name, page: 0, ..RepoSearch::default() }).await.unwrap();
    assert_eq!((first.total, first.page, first.page_count(), first.repos.len()), (35, 1, 2, 30));
    assert_eq!(first.repos[0].repo.name.as_str(), "repo-00");
    assert!(first.has_next() && !first.has_previous());

    let past_end =
        repos.explore(&RepoSearch { text: String::new(), sort: RepoSort::Name, page: 99, ..RepoSearch::default() }).await.unwrap();
    assert_eq!((past_end.page, past_end.repos.len()), (2, 5));

    let found =
        repos.explore(&RepoSearch { text: " REPO-3 ".into(), sort: RepoSort::Newest, page: 1, ..RepoSearch::default() }).await.unwrap();
    let names: Vec<_> = found.repos.iter().map(|r| r.repo.name.to_string()).collect();
    assert_eq!(names, ["repo-34", "repo-33", "repo-32", "repo-31", "repo-30"]);

    let none = repos.explore(&RepoSearch { text: "hidden".into(), ..RepoSearch::default() }).await.unwrap();
    assert_eq!((none.total, none.page, none.repos.len()), (0, 1, 0));
}
