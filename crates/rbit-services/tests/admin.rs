//! Administration, instance settings and suspension against real, temporary `SQLite`,
//! redb and on-disk repository stores.

use std::sync::Arc;

use rbit_core::{Config, InstanceSettings, OrgCreationPolicy, RegistrationMode, TokenScope, User, Visibility};
use rbit_services::{
    CreateAccessToken, CreateOrganization, CreateRepository, ServiceError, Services, SignUp,
    admin::{ListQuery, PAGE_SIZE},
    instance::PublicCounts,
};
use rbit_storage::{Database, KvStore, NewUser};

const PASSWORD: &str = "correct horse battery";

#[tokio::test]
async fn branding_is_authorized_validated_and_persists_across_service_reload() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let user = fixture.user("member", false).await;
    let mut settings = fixture.services.instance.current();
    settings.site_name = "Acme Code".into();
    settings.branding.logo_url = "https://example.com/logo.png".into();
    settings.branding.show_powered_by = false;
    settings.branding.show_version = false;
    assert!(forbidden(fixture.services.admin.update_settings(&user, settings.clone()).await));
    fixture.services.admin.update_settings(&root, settings.clone()).await.unwrap();
    let reloaded = Services::new(fixture.config.clone(), &fixture.database, &fixture.kv).await.unwrap();
    assert_eq!(reloaded.instance.current(), settings);
    settings.branding.logo_url = "javascript:alert(1)".into();
    assert!(invalid(fixture.services.admin.update_settings(&root, settings).await));
    assert_eq!(fixture.services.instance.current().branding.logo_url, "https://example.com/logo.png");
}

#[tokio::test]
async fn configuration_stages_validated_settings_masks_secrets_and_can_reset() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let user = fixture.user("member", false).await;
    let admin = &fixture.services.admin;
    assert!(forbidden(admin.configuration(&user).await));
    assert!(forbidden(admin.stage_configuration(&user, "{}").await));
    assert!(forbidden(admin.reset_configuration(&user).await));
    assert!(invalid(admin.stage_configuration(&root, "{invalid").await));
    assert!(invalid(admin.stage_configuration(&root, "{}").await));

    let mut value: serde_json::Value = serde_json::from_str(&admin.configuration(&root).await.unwrap()).unwrap();
    value["mail"]["smtp_password"] = "smtp-private-value".into();
    value["operations"]["metrics_token"] = "metrics-private-value".into();
    value["repository"]["default_branch"] = "develop".into();
    admin.stage_configuration(&root, &value.to_string()).await.unwrap();
    assert_eq!(fixture.config.repository.default_branch, "main", "deployment changes require restart");
    let masked = admin.configuration(&root).await.unwrap();
    assert!(!masked.contains("smtp-private-value") && !masked.contains("metrics-private-value"));
    assert!(masked.contains("<unchanged>"));
    admin.stage_configuration(&root, &masked).await.unwrap();
    let stored =
        fixture.database.instance_settings().load().await.unwrap().into_iter().find(|(key, _)| key == "deployment_config").unwrap().1;
    assert!(stored.contains("smtp-private-value") && stored.contains("metrics-private-value"));

    let mut invalid_value: serde_json::Value = serde_json::from_str(&masked).unwrap();
    invalid_value["storage"]["data_dir"] = "different-data".into();
    assert!(invalid(admin.stage_configuration(&root, &invalid_value.to_string()).await));
    invalid_value = serde_json::from_str(&masked).unwrap();
    invalid_value["server"]["publc_url"] = "typo".into();
    assert!(invalid(admin.stage_configuration(&root, &invalid_value.to_string()).await));
    invalid_value = serde_json::from_str(&masked).unwrap();
    invalid_value["server"]["public_url"] = "javascript:alert(1)".into();
    assert!(invalid(admin.stage_configuration(&root, &invalid_value.to_string()).await));
    assert_eq!(admin.configuration(&root).await.unwrap(), masked, "failed updates must be atomic");
    admin.reset_configuration(&root).await.unwrap();
    assert!(!admin.configuration(&root).await.unwrap().contains("develop"));
}

struct Fixture {
    dir: tempfile::TempDir,
    database: Database,
    kv: KvStore,
    config: Arc<Config>,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let config = Arc::new(config);
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        Self { dir, database, kv, config, services }
    }

    /// Builds a second set of services over the same stores, as a restart would.
    async fn restart(&self) -> Services {
        Services::new(Arc::clone(&self.config), &self.database, &self.kv).await.unwrap()
    }

    /// Inserts a user directly (fast: no password hashing).
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

    /// Creates a user with a real password, so sign-in can be exercised.
    async fn user_with_password(&self, name: &str) -> User {
        let input = SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        self.services.accounts.create_user(input, Some(false)).await.unwrap()
    }
}

#[allow(clippy::needless_pass_by_value)] // Reads best at call sites: `invalid(service.call().await)`.
fn invalid<T: std::fmt::Debug>(result: Result<T, ServiceError>) -> bool {
    matches!(result, Err(ServiceError::Invalid(_)))
}

#[allow(clippy::needless_pass_by_value)]
fn forbidden<T: std::fmt::Debug>(result: Result<T, ServiceError>) -> bool {
    matches!(result, Err(ServiceError::Forbidden))
}

fn settings(site_name: &str, registration: RegistrationMode) -> InstanceSettings {
    InstanceSettings {
        site_name: site_name.to_owned(),
        branding: rbit_core::config::BrandingConfig::default(),
        registration,
        org_creation: OrgCreationPolicy::AdminsOnly,
        default_visibility: Visibility::Private,
        digest_hour: 21,
    }
}

// ---------------------------------------------------------------------- instance settings

#[tokio::test]
async fn instance_settings_persist_and_apply_immediately() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    assert_eq!(fixture.services.instance.current(), InstanceSettings::defaults(&fixture.config));

    assert!(forbidden(fixture.services.instance.update(&alice, settings("Mine", RegistrationMode::Open)).await));
    assert!(invalid(fixture.services.instance.update(&admin, settings("   ", RegistrationMode::Open)).await));
    assert!(invalid(fixture.services.instance.update(&admin, settings(&"x".repeat(51), RegistrationMode::Open)).await));
    let late = InstanceSettings { digest_hour: 24, ..settings("Forge", RegistrationMode::Open) };
    assert!(invalid(fixture.services.instance.update(&admin, late).await), "digest hours are 0-23");

    fixture.services.instance.update(&admin, settings("  Forge  ", RegistrationMode::Closed)).await.unwrap();
    let expected = settings("Forge", RegistrationMode::Closed);
    assert_eq!(fixture.services.instance.current(), expected);
    assert_eq!(fixture.services.admin.dashboard(&admin).await.unwrap().settings, expected, "shared in-memory copy");
    assert_eq!(fixture.restart().await.instance.current(), expected, "survives a restart");
    assert_eq!(fixture.services.notification_email.default_digest_hour(), 21, "digests follow the runtime default");
}

#[tokio::test]
async fn closed_registration_still_allows_the_first_account_and_admin_creation() {
    let fixture = Fixture::new().await;
    let mut config = (*fixture.config).clone();
    config.server.open_registration = false;
    let services = Services::new(Arc::new(config), &fixture.database, &fixture.kv).await.unwrap();
    let signup = |name: &str| SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };

    assert!(services.accounts.registration_open().await.unwrap(), "bootstrap rule");
    let first = services.accounts.sign_up(signup("root")).await.unwrap();
    assert!(first.is_admin, "the first user administers the instance");

    assert!(!services.accounts.registration_open().await.unwrap());
    assert!(matches!(services.accounts.sign_up(signup("bob")).await, Err(ServiceError::RegistrationClosed)));

    let bob = services.admin.create_user(&first, signup("bob"), false).await.unwrap();
    assert!(!bob.is_admin);
    assert!(forbidden(services.admin.create_user(&bob, signup("carol"), false).await));
    assert!(invalid(services.admin.create_user(&first, SignUp { password: "short".to_owned(), ..signup("dave") }, false).await));

    // Opening registration at runtime takes effect without a restart.
    services.instance.update(&first, settings("rbit", RegistrationMode::Open)).await.unwrap();
    assert!(services.accounts.registration_open().await.unwrap());
}

// ---------------------------------------------------------------------- suspension

#[tokio::test]
async fn concurrent_signups_respect_the_bootstrap_boundary() {
    for open in [false, true] {
        let fixture = Fixture::new().await;
        let mut config = (*fixture.config).clone();
        config.server.open_registration = open;
        let services = Services::new(Arc::new(config), &fixture.database, &fixture.kv).await.unwrap();
        let signup = |name: &str| SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        let (first, second, third) = tokio::join!(
            services.accounts.sign_up(signup("alice")),
            services.accounts.sign_up(signup("bob")),
            services.accounts.sign_up(signup("carol")),
        );
        let outcomes = [first, second, third];
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), if open { 3 } else { 1 });
        assert_eq!(outcomes.iter().filter_map(|result| result.as_ref().ok()).filter(|user| user.is_admin).count(), 1);
        for result in outcomes {
            assert!(result.is_ok() || matches!(result, Err(ServiceError::RegistrationClosed)), "{result:?}");
        }
    }
}

#[tokio::test]
async fn suspended_users_lose_every_kind_of_access() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user_with_password("alice").await;
    let accounts = &fixture.services.accounts;
    let tokens = &fixture.services.tokens;

    let session = accounts.start_session(&alice).await.unwrap();
    let (_, secret) =
        tokens.create(&alice, CreateAccessToken { name: "ci".to_owned(), scope: TokenScope::Write, expires_in_days: None }).await.unwrap();
    assert!(accounts.resume_session(&session).await.unwrap().is_some());
    assert!(tokens.authenticate(&secret).await.unwrap().is_some());

    fixture.services.admin.set_suspended(&admin, alice.id(), true).await.unwrap();

    assert!(accounts.is_suspended(alice.id()).await.unwrap());
    assert!(forbidden(accounts.authenticate("alice", PASSWORD.to_owned()).await), "right password, suspended");
    assert!(
        matches!(accounts.authenticate("alice", "wrong password".to_owned()).await, Err(ServiceError::InvalidCredentials)),
        "suspension is not revealed without the password"
    );
    assert!(accounts.resume_session(&session).await.unwrap().is_none());
    assert!(tokens.authenticate(&secret).await.unwrap().is_none());

    fixture.services.admin.set_suspended(&admin, alice.id(), false).await.unwrap();
    assert!(accounts.authenticate("alice", PASSWORD.to_owned()).await.is_ok());
    assert!(tokens.authenticate(&secret).await.unwrap().is_some());
    assert!(accounts.resume_session(&session).await.unwrap().is_none(), "the old session was discarded");
}

// ---------------------------------------------------------------------- guard rails

#[tokio::test]
async fn admin_actions_require_a_site_admin() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let admin = &fixture.services.admin;

    assert!(forbidden(admin.dashboard(&alice).await));
    assert!(forbidden(admin.users(&alice, &ListQuery::default()).await));
    assert!(forbidden(admin.organizations(&alice, &ListQuery::default()).await));
    assert!(forbidden(admin.repositories(&alice, &ListQuery::default()).await));
    assert!(forbidden(admin.set_admin(&alice, alice.id(), true).await));
    assert!(forbidden(admin.set_suspended(&alice, bob.id(), true).await));
    assert!(forbidden(admin.delete_user(&alice, bob.id(), "bob").await));
}

#[tokio::test]
async fn admins_cannot_lock_themselves_or_the_instance_out() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let second = fixture.user("second", true).await;
    let admin = &fixture.services.admin;

    assert!(invalid(admin.set_admin(&root, root.id(), false).await));
    assert!(invalid(admin.set_suspended(&root, root.id(), true).await));
    assert!(invalid(admin.delete_user(&root, root.id(), "root").await));

    // `second` demotes nobody yet; root demotes second, leaving root as the last admin.
    admin.set_admin(&root, second.id(), false).await.unwrap();
    // A stale session of `second` (still flagged admin in memory) can't remove the last admin.
    for result in [
        admin.set_admin(&second, root.id(), false).await.map(drop),
        admin.set_suspended(&second, root.id(), true).await.map(drop),
        admin.delete_user(&second, root.id(), "root").await.map(drop),
    ] {
        assert!(invalid(result));
    }
    let root_record = admin.user(&root, root.id()).await.unwrap();
    assert!(root_record.user.is_admin && !root_record.is_suspended());

    // A suspended admin doesn't count as active.
    admin.set_admin(&root, second.id(), true).await.unwrap();
    admin.set_suspended(&root, second.id(), true).await.unwrap();
    assert!(invalid(admin.set_admin(&second, root.id(), false).await));
    admin.set_suspended(&root, second.id(), false).await.unwrap();
    admin.set_admin(&second, root.id(), false).await.unwrap();
    assert!(!admin.user(&second, root.id()).await.unwrap().user.is_admin);

    assert!(matches!(admin.set_admin(&second, rbit_core::AccountId(9999), true).await, Err(ServiceError::NotFound)));
}

// ---------------------------------------------------------------------- deletion

#[tokio::test]
async fn deleting_a_user_removes_their_repositories_from_disk() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture
        .services
        .repositories
        .create(&alice, CreateRepository { name: "notes".to_owned(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let path = fixture.config.storage.repositories_dir().join(alice.id().0.to_string()).join(format!("{}.git", repo.repo.id.0));
    assert!(path.exists());

    let admin = &fixture.services.admin;
    assert!(invalid(admin.delete_user(&root, alice.id(), "Alice ").await), "confirmation must match exactly");
    admin.delete_user(&root, alice.id(), "alice").await.unwrap();

    assert!(!path.exists());
    assert!(matches!(admin.user(&root, alice.id()).await, Err(ServiceError::NotFound)));
    assert_eq!(admin.dashboard(&root).await.unwrap().counts.private_repositories, 0);
    assert!(fixture.dir.path().exists());
}

#[tokio::test]
async fn deleting_a_repository_requires_its_full_name() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let repo = fixture
        .services
        .repositories
        .create(&root, CreateRepository { name: "site".to_owned(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();
    let admin = &fixture.services.admin;

    assert!(invalid(admin.delete_repository(&root, repo.repo.id, "site").await));
    admin.delete_repository(&root, repo.repo.id, "root/site").await.unwrap();
    assert!(matches!(admin.repository(&root, repo.repo.id).await, Err(ServiceError::NotFound)));
    assert!(!fixture.config.storage.repositories_dir().join(root.id().0.to_string()).join(format!("{}.git", repo.repo.id.0)).exists());
}

// ---------------------------------------------------------------------- listings & dashboard

#[tokio::test]
async fn user_listing_searches_and_paginates() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    for n in 0..PAGE_SIZE + 5 {
        fixture.user(&format!("user{n:02}"), false).await;
    }
    fixture.user("under_score", false).await;
    let admin = &fixture.services.admin;

    let first = admin.users(&root, &ListQuery::default()).await.unwrap();
    assert_eq!(first.total, u64::from(PAGE_SIZE) + 7);
    assert_eq!(first.items.len(), PAGE_SIZE as usize);
    assert!(first.has_next() && !first.has_previous());

    let second = admin.users(&root, &ListQuery { search: String::new(), page: 2 }).await.unwrap();
    assert_eq!(second.items.len(), 7);
    assert!(!second.has_next());

    let found = admin.users(&root, &ListQuery { search: " USER1 ".to_owned(), page: 1 }).await.unwrap();
    assert_eq!(found.total, 10, "case-insensitive substring of name");
    let by_email = admin.users(&root, &ListQuery { search: "root@example".to_owned(), page: 1 }).await.unwrap();
    assert_eq!(by_email.items.len(), 1);
    let literal = admin.users(&root, &ListQuery { search: "_".to_owned(), page: 1 }).await.unwrap();
    assert_eq!(literal.total, 1, "wildcards are matched literally");
}

#[tokio::test]
async fn dashboard_reports_counts_and_versions() {
    let fixture = Fixture::new().await;
    let root = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    fixture.services.admin.set_suspended(&root, alice.id(), true).await.unwrap();
    fixture
        .services
        .repositories
        .create(&root, CreateRepository { name: "a".to_owned(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();

    let dashboard = fixture.services.admin.dashboard(&root).await.unwrap();
    let counts = dashboard.counts;
    assert_eq!((counts.users, counts.admins, counts.suspended), (2, 1, 1));
    assert_eq!((counts.public_repositories, counts.private_repositories, counts.organizations), (1, 0, 0));
    assert_eq!(dashboard.system.rbit_version, env!("CARGO_PKG_VERSION"));
    assert!(dashboard.system.sqlite_version.starts_with('3'));
    assert!(dashboard.system.data_dir_bytes.is_some_and(|bytes| bytes > 0));
    // `git` is required by the test suite elsewhere too.
    assert!(dashboard.system.git_version.is_some_and(|version| version.chars().next().is_some_and(|c| c.is_ascii_digit())));
}

#[tokio::test]
async fn public_counts_leave_out_private_repositories_and_suspended_users() {
    let fixture = Fixture::new().await;
    let empty = fixture.services.instance.public_counts().await.unwrap();
    assert_eq!(empty, PublicCounts::default(), "a fresh instance counts nothing");

    let root = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    fixture.user("bob", false).await;
    fixture.services.admin.set_suspended(&root, alice.id(), true).await.unwrap();
    let repositories = &fixture.services.repositories;
    for (name, visibility) in [("open", Visibility::Public), ("also-open", Visibility::Public), ("secret", Visibility::Private)] {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility };
        repositories.create(&root, input).await.unwrap();
    }
    let org = CreateOrganization { name: "acme".to_owned(), ..CreateOrganization::default() };
    fixture.services.orgs.create(&root, org).await.unwrap();

    let counts = fixture.services.instance.public_counts().await.unwrap();
    assert_eq!(counts, PublicCounts { public_repositories: 2, users: 2, organizations: 1 });
}
