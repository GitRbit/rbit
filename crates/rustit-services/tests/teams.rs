//! Team rules: who may see and manage teams, membership, repository grants.

use std::sync::Arc;

use rustit_core::{
    Config, OrgRole, Organization, RepoAccess, User, Visibility,
    membership::{BasePermission, Permission},
};
use rustit_services::{
    CreateRepository, ServiceError, Services,
    orgs::CreateOrganization,
    teams::{TeamContext, TeamInput},
};
use rustit_storage::{Database, KvStore, NewUser};

struct Fixture {
    _dir: tempfile::TempDir,

    services: Services,
    olga: User,
    bob: User,
    eve: User,
    root: User,
    acme: Organization,
}

impl Fixture {
    /// `olga` owns `acme` (base permission `none`), `bob` is a member, `eve` is not,
    /// `root` is a site administrator.
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rustit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rustit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let user = |name: &'static str, is_admin: bool| {
            let database = database.clone();
            async move {
                database
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
        };
        let (olga, bob, eve, root) =
            (user("olga", false).await, user("bob", false).await, user("eve", false).await, user("root", true).await);
        let acme = services.orgs.create(&olga, CreateOrganization { name: "acme".into(), ..CreateOrganization::default() }).await.unwrap();
        services.orgs.add_member(&olga, &acme, "bob", OrgRole::Member).await.unwrap();
        services.orgs.set_base_permission(&olga, &acme, BasePermission::None).await.unwrap();
        for (name, visibility) in [("rocket", Visibility::Private), ("brochure", Visibility::Public)] {
            let input = CreateRepository { name: name.into(), description: String::new(), visibility };
            services.repositories.create_in(&olga, "acme", input).await.unwrap();
        }
        Self { _dir: dir, services, olga, bob, eve, root, acme }
    }

    async fn team(&self, name: &str) -> TeamContext {
        let input = TeamInput { name: name.to_owned(), description: String::new() };
        self.services.teams.create(&self.olga, &self.acme, input).await.unwrap();
        self.services.teams.open(Some(&self.olga), &self.acme, name).await.unwrap()
    }

    async fn access(&self, viewer: &User, name: &str) -> RepoAccess {
        match self.services.repositories.open(Some(viewer), "acme", name).await {
            Ok(ctx) => ctx.access,
            Err(ServiceError::NotFound) => RepoAccess::None,
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}

fn invalid<T: std::fmt::Debug>(result: Result<T, ServiceError>, needle: &str) {
    match result {
        Err(ServiceError::Invalid(message)) => assert!(message.contains(needle), "{message:?} should mention {needle:?}"),
        other => panic!("expected a validation error mentioning {needle:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn owners_create_rename_and_delete_teams() {
    let fixture = Fixture::new().await;
    let teams = &fixture.services.teams;
    let input = |name: &str, description: &str| TeamInput { name: name.to_owned(), description: description.to_owned() };

    let core = teams.create(&fixture.olga, &fixture.acme, input(" core ", " The core team ")).await.unwrap();
    assert_eq!(core.name.as_str(), "core");
    assert_eq!(core.description.as_deref(), Some("The core team"));
    invalid(teams.create(&fixture.olga, &fixture.acme, input("Core", "")).await, "already exists");
    invalid(teams.create(&fixture.olga, &fixture.acme, input("new", "")).await, "reserved");
    invalid(teams.create(&fixture.olga, &fixture.acme, input("bad name", "")).await, "team name");
    invalid(teams.create(&fixture.olga, &fixture.acme, input("long", &"x".repeat(351))).await, "description");
    assert!(matches!(teams.create(&fixture.bob, &fixture.acme, input("mine", "")).await, Err(ServiceError::Forbidden)));
    assert!(matches!(teams.create(&fixture.eve, &fixture.acme, input("mine", "")).await, Err(ServiceError::NotFound)));
    teams.create(&fixture.root, &fixture.acme, input("ops", "")).await.expect("site admins manage every organisation");

    let ctx = teams.open(Some(&fixture.olga), &fixture.acme, "CORE").await.unwrap();
    assert!(ctx.can_manage && !ctx.is_member);
    assert_eq!(ctx.full_name(), "acme/core");
    let renamed = teams.update(&fixture.olga, &ctx, input("platform", "")).await.unwrap();
    assert_eq!(renamed.as_str(), "platform");
    let ctx = teams.open(Some(&fixture.olga), &fixture.acme, "platform").await.unwrap();
    assert_eq!(ctx.team.description, None);
    invalid(teams.update(&fixture.olga, &ctx, input("ops", "")).await, "already exists");
    assert!(matches!(teams.update(&fixture.bob, &ctx, input("x", "")).await, Err(ServiceError::Forbidden)));

    let names = |list: Vec<rustit_core::membership::TeamSummary>| list.into_iter().map(|t| t.team.name.to_string()).collect::<Vec<_>>();
    assert_eq!(names(teams.list(Some(&fixture.bob), &fixture.acme).await.unwrap()), ["ops", "platform"]);
    assert!(matches!(teams.delete(&fixture.bob, &ctx).await, Err(ServiceError::Forbidden)));
    teams.delete(&fixture.olga, &ctx).await.unwrap();
    assert_eq!(names(teams.list(Some(&fixture.olga), &fixture.acme).await.unwrap()), ["ops"]);
    assert!(matches!(teams.open(Some(&fixture.olga), &fixture.acme, "platform").await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn teams_are_visible_to_members_only() {
    let fixture = Fixture::new().await;
    fixture.team("core").await;
    let teams = &fixture.services.teams;

    assert!(teams.can_view(Some(&fixture.bob), &fixture.acme).await.unwrap());
    assert!(teams.can_view(Some(&fixture.root), &fixture.acme).await.unwrap());
    assert!(!teams.can_view(Some(&fixture.eve), &fixture.acme).await.unwrap());
    assert!(!teams.can_view(None, &fixture.acme).await.unwrap());
    for viewer in [None, Some(&fixture.eve)] {
        assert!(matches!(teams.list(viewer, &fixture.acme).await, Err(ServiceError::NotFound)));
        assert!(matches!(teams.open(viewer, &fixture.acme, "core").await, Err(ServiceError::NotFound)));
    }
    let as_bob = teams.open(Some(&fixture.bob), &fixture.acme, "core").await.unwrap();
    assert!(!as_bob.can_manage);
}

#[tokio::test]
async fn members_must_belong_to_the_organisation_and_may_leave() {
    let fixture = Fixture::new().await;
    let core = fixture.team("core").await;
    let teams = &fixture.services.teams;

    teams.add_member(&fixture.olga, &core, "bob").await.unwrap();
    invalid(teams.add_member(&fixture.olga, &core, "bob").await, "already on this team");
    invalid(teams.add_member(&fixture.olga, &core, "eve").await, "not a member of acme");
    invalid(teams.add_member(&fixture.olga, &core, "ghost").await, "no user called ghost");
    invalid(teams.add_member(&fixture.olga, &core, "acme").await, "no user called acme");
    assert!(matches!(teams.add_member(&fixture.bob, &core, "olga").await, Err(ServiceError::Forbidden)));
    let names: Vec<_> = teams.members(&core).await.unwrap().iter().map(|a| a.name.to_string()).collect();
    assert_eq!(names, ["bob"]);
    assert!(teams.open(Some(&fixture.bob), &fixture.acme, "core").await.unwrap().is_member);

    // Members cannot remove others, but can leave.
    teams.add_member(&fixture.olga, &core, "olga").await.unwrap();
    assert!(matches!(teams.remove_member(&fixture.bob, &core, "olga").await, Err(ServiceError::Forbidden)));
    teams.remove_member(&fixture.bob, &core, "bob").await.unwrap();
    invalid(teams.remove_member(&fixture.olga, &core, "bob").await, "not on this team");

    // Leaving the organisation leaves its teams.
    teams.add_member(&fixture.olga, &core, "bob").await.unwrap();
    fixture.services.orgs.leave(&fixture.bob, &fixture.acme).await.unwrap();
    let names: Vec<_> = teams.members(&core).await.unwrap().iter().map(|a| a.name.to_string()).collect();
    assert_eq!(names, ["olga"]);
}

#[tokio::test]
async fn team_grants_give_their_members_access() {
    let fixture = Fixture::new().await;
    let core = fixture.team("core").await;
    let teams = &fixture.services.teams;
    teams.add_member(&fixture.olga, &core, "bob").await.unwrap();
    assert_eq!(fixture.access(&fixture.bob, "rocket").await, RepoAccess::None, "base permission none");

    teams.set_repository(&fixture.olga, &core, "rocket", Permission::Write).await.unwrap();
    assert_eq!(fixture.access(&fixture.bob, "rocket").await, RepoAccess::Write);
    teams.set_repository(&fixture.olga, &core, "acme/rocket", Permission::Admin).await.expect("owner/name is accepted");
    assert_eq!(fixture.access(&fixture.bob, "rocket").await, RepoAccess::Admin);
    teams.set_repository(&fixture.olga, &core, "brochure", Permission::Read).await.unwrap();
    invalid(teams.set_repository(&fixture.olga, &core, "missing", Permission::Read).await, "no repository called missing");
    invalid(teams.set_repository(&fixture.olga, &core, "", Permission::Read).await, "enter a repository name");
    let personal = CreateRepository { name: "dotfiles".into(), description: String::new(), visibility: Visibility::Public };
    fixture.services.repositories.create(&fixture.olga, personal).await.unwrap();
    invalid(teams.set_repository(&fixture.olga, &core, "olga/dotfiles", Permission::Read).await, "no repository called dotfiles");
    assert!(matches!(teams.set_repository(&fixture.bob, &core, "rocket", Permission::Read).await, Err(ServiceError::Forbidden)));

    // The team page shows readable repositories only: Eve could not open it anyway, and an
    // anonymous visitor sees the public one.
    let describe = |list: Vec<(rustit_core::OwnedRepository, Permission)>| {
        list.into_iter().map(|(r, p)| format!("{}:{p}", r.repo.name)).collect::<Vec<_>>()
    };
    assert_eq!(describe(teams.repositories(Some(&fixture.bob), &core).await.unwrap()), ["brochure:read", "rocket:admin"]);
    assert_eq!(describe(teams.repositories(None, &core).await.unwrap()), ["brochure:read"]);

    teams.remove_repository(&fixture.olga, &core, "rocket").await.unwrap();
    invalid(teams.remove_repository(&fixture.olga, &core, "rocket").await, "no access to acme/rocket");
    assert_eq!(fixture.access(&fixture.bob, "rocket").await, RepoAccess::None);

    // Audited under "Repository access".
    let log = fixture.services.audit.list(&fixture.root, None, &rustit_services::admin::ListQuery::default()).await.unwrap();
    let actions: Vec<_> = log.items.iter().map(|event| event.action.as_str()).collect();
    assert!(actions.contains(&"team_created") && actions.contains(&"team_changed"), "{actions:?}");
}

#[tokio::test]
async fn owners_change_the_base_permission() {
    let fixture = Fixture::new().await;
    let orgs = &fixture.services.orgs;
    assert_eq!(orgs.base_permission(&fixture.acme).await.unwrap(), BasePermission::None);
    assert_eq!(fixture.access(&fixture.bob, "rocket").await, RepoAccess::None);
    assert_eq!(fixture.access(&fixture.bob, "brochure").await, RepoAccess::Read, "public stays readable");

    assert!(matches!(orgs.set_base_permission(&fixture.bob, &fixture.acme, BasePermission::Write).await, Err(ServiceError::Forbidden)));
    assert!(matches!(orgs.set_base_permission(&fixture.eve, &fixture.acme, BasePermission::Write).await, Err(ServiceError::NotFound)));
    orgs.set_base_permission(&fixture.olga, &fixture.acme, BasePermission::Read).await.unwrap();
    assert_eq!(fixture.access(&fixture.bob, "rocket").await, RepoAccess::Read);
    orgs.set_base_permission(&fixture.root, &fixture.acme, BasePermission::Write).await.unwrap();
    assert_eq!(fixture.access(&fixture.bob, "rocket").await, RepoAccess::Write);
    assert_eq!(fixture.access(&fixture.olga, "rocket").await, RepoAccess::Admin, "owners always administer");
}
