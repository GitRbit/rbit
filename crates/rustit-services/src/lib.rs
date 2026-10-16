//! # rustit-services
//!
//! The application layer: one service per area, each enforcing business rules and
//! permissions. Services are HTTP-agnostic so the web UI, a future REST API, CLI admin
//! commands and background jobs all share exactly the same behaviour.

pub mod accounts;
pub mod admin;
pub mod audit;
pub mod error;
pub mod events;
pub mod insights;
pub mod instance;
pub mod jobs;
pub mod mail;
pub mod orgs;
pub mod profiles;
pub mod pushes;
pub mod recovery;
pub mod repo_settings;
pub mod repositories;
pub mod ssh_keys;
pub mod stars;
pub mod throttle;
pub mod tokens;
pub mod two_factor;

use std::sync::Arc;

use rustit_core::Config;
use rustit_git::RepositoryStorage;
use rustit_storage::{Database, KvStore};

pub use accounts::{AccountService, SignUp};
pub use admin::AdminService;
pub use audit::AuditService;
pub use error::ServiceError;
pub use events::{DomainEvent, EventBus};
pub use insights::InsightsService;
pub use instance::InstanceService;
pub use jobs::JobService;
pub use mail::MailService;
pub use orgs::{CreateOrganization, OrgContext, OrgService, UpdateOrganization};
pub use profiles::ProfileService;
pub use pushes::{PendingPush, PushService};
pub use recovery::RecoveryService;
pub use repo_settings::RepoSettingsService;
pub use repositories::{CreateRepository, RepoContext, RepoSearch, RepoSearchResults, RepositoryService};
pub use ssh_keys::SshKeyService;
pub use stars::{StarService, StarSummary};
pub use throttle::{ThrottleScope, ThrottleService};
pub use tokens::{AccessTokenService, CreateAccessToken};
pub use two_factor::TwoFactorService;

/// The infrastructure every service is built from. Cheap to clone (all handles are `Arc`s).
#[derive(Debug, Clone)]
pub struct ServiceContext {
    pub config: Arc<Config>,
    pub database: Database,
    pub kv: KvStore,
    pub repositories: RepositoryStorage,
    pub events: EventBus,
}

/// All services, constructed once at start-up and shared by reference.
#[derive(Debug, Clone)]
pub struct Services {
    pub accounts: AccountService,
    pub admin: AdminService,
    pub audit: AuditService,
    pub events: EventBus,
    pub insights: InsightsService,
    pub instance: InstanceService,
    pub jobs: JobService,
    pub mail: MailService,
    pub orgs: OrgService,
    pub profiles: ProfileService,
    pub pushes: PushService,
    pub recovery: RecoveryService,
    pub repo_settings: RepoSettingsService,
    pub repositories: RepositoryService,
    pub ssh_keys: SshKeyService,
    pub stars: StarService,
    pub throttle: ThrottleService,
    pub tokens: AccessTokenService,
    pub two_factor: TwoFactorService,
}

impl Services {
    pub async fn new(config: Arc<Config>, database: &Database, kv: &KvStore) -> Result<Self, ServiceError> {
        let ctx = ServiceContext {
            config: Arc::clone(&config),
            database: database.clone(),
            kv: kv.clone(),
            repositories: RepositoryStorage::new(config.storage.repositories_dir()),
            events: EventBus::new(),
        };
        let instance = InstanceService::load(&ctx).await?;
        // Shared so recovery emails use the same delivery queue and the same rate limits.
        let mail = MailService::new(ctx.clone());
        let throttle = ThrottleService::new(ctx.clone());

        Ok(Self {
            accounts: AccountService::new(Arc::clone(&config), database.accounts(), kv.sessions(), instance.clone(), ctx.events.clone()),
            audit: AuditService::new(ctx.clone()),
            jobs: JobService::new(ctx.clone()),
            mail: mail.clone(),
            recovery: RecoveryService::builder(ctx.clone()).mail(mail).throttle(throttle.clone()).instance(instance.clone()).build(),
            ssh_keys: SshKeyService::new(ctx.clone()),
            throttle,
            two_factor: TwoFactorService::new(ctx.clone()),
            admin: AdminService::new(ctx.clone(), instance.clone()),
            insights: InsightsService::new(ctx.clone()),
            orgs: OrgService::new(ctx.clone(), instance.clone()),
            profiles: ProfileService::new(ctx.clone()),
            pushes: PushService::new(ctx.clone()),
            repo_settings: RepoSettingsService::new(ctx.clone()),
            repositories: RepositoryService::new(config, database.repositories(), ctx.repositories.clone()),
            stars: StarService::new(ctx.clone()),
            tokens: AccessTokenService::new(database.access_tokens(), database.accounts()),
            events: ctx.events.clone(),
            instance,
        })
    }

    /// Starts background subscribers and periodic work. Call once, after start-up.
    pub fn spawn_background(&self) {
        self.jobs.spawn_background();
        self.recovery.spawn_background();
    }
}
