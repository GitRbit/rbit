//! Repository transfer: who may move a repository where, and what moves with it.

use std::sync::Arc;

use rustit_core::{BasePermission, Config, OrgRole, Permission, RepoAccess, User, Visibility};
use rustit_services::{CreateRepository, DomainEvent, RepoContext, ServiceError, Services, orgs::CreateOrganization};
use rustit_storage::{Database, KvStore, NewUser};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rustit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rustit.redb")).unwrap();
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

    async fn org(&self, owner: &User, name: &str) {
        self.services.orgs.create(owner, CreateOrganization { name: name.to_owned(), ..CreateOrganization::default() }).await.unwrap();
    }

    async fn repo(&self, actor: &User, owner: &str, name: &str) -> RepoContext {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility: Visibility::Private };
        self.services.repositories.create_in(actor, owner, input).await.unwrap();
        self.open(actor, owner, name).await
    }

    async fn open(&self, viewer: &User, owner: &str, name: &str) -> RepoContext {
        self.services.repositories.open(Some(viewer), owner, name).await.unwrap()
    }
}

fn invalid<T: std::fmt::Debug>(result: Result<T, ServiceError>, needle: &str) {
    match result {
        Err(ServiceError::Invalid(message)) => assert!(message.contains(needle), "{message:?} should mention {needle:?}"),
        other => panic!("expected a validation error mentioning {needle:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn admins_move_repositories_to_their_organisations() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let carol = fixture.user("carol", false).await;
    let fan = fixture.user("fan", false).await;
    fixture.org(&alice, "acme").await;
    let ctx = fixture.repo(&alice, "alice", "rocket").await;
    let old_path = ctx.disk_path.clone();
    std::fs::write(old_path.join("marker"), "moved with it").unwrap();
    fixture.services.collaborators.add(&alice, &ctx, "carol", Permission::Read).await.unwrap();
    fixture.database.stars().add(fan.id(), ctx.repo.repo.id).await.unwrap();
    let mut events = fixture.services.events.subscribe();

    let suggested: Vec<_> =
        fixture.services.transfers.suggested_owners(&alice, &ctx).await.unwrap().iter().map(|a| a.name.to_string()).collect();
    assert_eq!(suggested, ["acme"], "the current owner is not offered");

    let moved = fixture.services.transfers.transfer(&alice, &ctx, " acme ", "alice/rocket").await.unwrap();
    assert_eq!(moved.full_name(), "acme/rocket");
    assert_eq!(moved.repo.id, ctx.repo.repo.id);
    assert_eq!(
        events.recv().await.unwrap(),
        DomainEvent::RepositoryTransferred { repo: ctx.repo.repo.id, from: alice.id(), to: moved.owner.id, actor: alice.id() }
    );

    let reopened = fixture.open(&alice, "acme", "rocket").await;
    assert_eq!(reopened.access, RepoAccess::Admin, "alice owns acme");
    assert!(!old_path.exists());
    assert_eq!(std::fs::read_to_string(reopened.disk_path.join("marker")).unwrap(), "moved with it");
    assert!(matches!(fixture.services.repositories.open(Some(&alice), "alice", "rocket").await, Err(ServiceError::NotFound)));
    assert_eq!(
        fixture.services.repositories.open(Some(&carol), "acme", "rocket").await.unwrap().access,
        RepoAccess::Read,
        "collaborators stay"
    );
    assert_eq!(fixture.services.stars.summary(ctx.repo.repo.id, None).await.unwrap().count, 1, "stars stay");

    let root = fixture.user("root", true).await;
    let log = fixture.services.audit.list(&root, None, &rustit_services::admin::ListQuery::default()).await.unwrap();
    let entry = log.items.iter().find(|event| event.action.as_str() == "repository_transferred").expect("audited");
    assert_eq!(entry.target_name.as_deref(), Some("alice/rocket"));
    assert_eq!(entry.detail, "to acme/rocket");
}

#[tokio::test]
async fn transfers_are_refused_without_the_right_to_create_there() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let mallory = fixture.user("mallory", false).await;
    fixture.org(&bob, "elsewhere").await;
    let ctx = fixture.repo(&alice, "alice", "rocket").await;
    let transfers = &fixture.services.transfers;

    invalid(transfers.transfer(&alice, &ctx, "elsewhere", "alice/rocket").await, "cannot transfer repositories to elsewhere");
    invalid(transfers.transfer(&alice, &ctx, "bob", "alice/rocket").await, "cannot transfer repositories to bob");
    invalid(transfers.transfer(&alice, &ctx, "ghost", "alice/rocket").await, "cannot transfer repositories to ghost");
    invalid(transfers.transfer(&alice, &ctx, "", "alice/rocket").await, "choose the new owner");
    invalid(transfers.transfer(&alice, &ctx, "alice", "alice/rocket").await, "already belongs to alice");
    invalid(transfers.transfer(&alice, &ctx, "elsewhere", "rocket").await, "to confirm");

    // Writers are not administrators.
    fixture.services.collaborators.add(&alice, &ctx, "mallory", Permission::Write).await.unwrap();
    let as_mallory = fixture.open(&mallory, "alice", "rocket").await;
    assert!(matches!(transfers.transfer(&mallory, &as_mallory, "mallory", "alice/rocket").await, Err(ServiceError::Forbidden)));
    assert!(fixture.open(&alice, "alice", "rocket").await.disk_path.exists(), "nothing moved");
}

#[tokio::test]
async fn site_admins_move_repositories_anywhere_except_to_suspended_users() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let sam = fixture.user("sam", false).await;
    let root = fixture.user("root", true).await;
    fixture.repo(&alice, "alice", "rocket").await;
    let as_root = fixture.open(&root, "alice", "rocket").await;

    fixture.database.admin().set_suspended(sam.id(), true).await.unwrap();
    invalid(fixture.services.transfers.transfer(&root, &as_root, "sam", "alice/rocket").await, "sam is suspended");
    invalid(fixture.services.transfers.transfer(&root, &as_root, "ghost", "alice/rocket").await, "no user or organization called ghost");
    fixture.services.transfers.transfer(&root, &as_root, "bob", "alice/rocket").await.unwrap();
    assert_eq!(fixture.open(&bob, "bob", "rocket").await.access, RepoAccess::Admin);
    assert!(matches!(fixture.services.repositories.open(Some(&alice), "bob", "rocket").await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn a_name_clash_rolls_the_move_back() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    fixture.org(&alice, "acme").await;
    let ctx = fixture.repo(&alice, "alice", "Rocket").await;
    let existing = fixture.repo(&alice, "acme", "rocket").await;

    invalid(fixture.services.transfers.transfer(&alice, &ctx, "acme", "alice/Rocket").await, "acme/Rocket already exists");
    // The directory went back where it was and the database is unchanged.
    assert!(ctx.disk_path.exists(), "moved back after the database refused");
    let reopened = fixture.open(&alice, "alice", "Rocket").await;
    assert_eq!(reopened.repo.repo.id, ctx.repo.repo.id);
    assert_eq!(reopened.disk_path, ctx.disk_path);
    assert!(existing.disk_path.exists());
    assert_eq!(fixture.open(&alice, "acme", "rocket").await.repo.repo.id, existing.repo.repo.id);
}

#[tokio::test]
async fn transfers_drop_the_old_organisations_team_grants() {
    let fixture = Fixture::new().await;
    let olga = fixture.user("olga", false).await;
    let bob = fixture.user("bob", false).await;
    fixture.org(&olga, "acme").await;
    fixture.org(&olga, "beta").await;
    let acme = fixture.services.orgs.open(None, "acme").await.unwrap().org;
    fixture.services.orgs.add_member(&olga, &acme, "bob", OrgRole::Member).await.unwrap();
    fixture.services.orgs.set_base_permission(&olga, &acme, BasePermission::None).await.unwrap();
    let ctx = fixture.repo(&olga, "acme", "rocket").await;
    let input = rustit_services::teams::TeamInput { name: "core".into(), description: String::new() };
    fixture.services.teams.create(&olga, &acme, input).await.unwrap();
    let core = fixture.services.teams.open(Some(&olga), &acme, "core").await.unwrap();
    fixture.services.teams.add_member(&olga, &core, "bob").await.unwrap();
    fixture.services.teams.set_repository(&olga, &core, "rocket", Permission::Write).await.unwrap();
    assert_eq!(fixture.open(&bob, "acme", "rocket").await.access, RepoAccess::Write);

    fixture.services.transfers.transfer(&olga, &ctx, "beta", "acme/rocket").await.unwrap();
    assert!(fixture.services.teams.repositories(Some(&olga), &core).await.unwrap().is_empty());
    assert!(matches!(fixture.services.repositories.open(Some(&bob), "beta", "rocket").await, Err(ServiceError::NotFound)));
}
