//! # rustit-services
//!
//! The application layer: one service per area, each enforcing business rules and
//! permissions. Services are HTTP-agnostic so the web UI, a future REST API, CLI admin
//! commands and background jobs all share exactly the same behaviour.

pub mod accounts;
pub mod admin;
pub mod error;
pub mod events;
pub mod insights;
pub mod instance;
pub mod orgs;
pub mod profiles;
pub mod repo_settings;
pub mod repositories;
pub mod stars;
pub mod tokens;

use std::sync::Arc;

use rustit_core::Config;
use rustit_git::RepositoryStorage;
use rustit_storage::{Database, KvStore};

pub use accounts::{AccountService, SignUp};
pub use admin::AdminService;
pub use error::ServiceError;
pub use events::{DomainEvent, EventBus};
pub use insights::InsightsService;
pub use instance::InstanceService;
pub use orgs::{CreateOrganization, OrgContext, OrgService, UpdateOrganization};
pub use profiles::ProfileService;
pub use repo_settings::RepoSettingsService;
pub use repositories::{CreateRepository, RepoContext, RepoSearch, RepoSearchResults, RepositoryService};
pub use stars::{StarService, StarSummary};
pub use tokens::{AccessTokenService, CreateAccessToken};

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
    pub events: EventBus,
    pub insights: InsightsService,
    pub instance: InstanceService,
    pub orgs: OrgService,
    pub profiles: ProfileService,
    pub repo_settings: RepoSettingsService,
    pub repositories: RepositoryService,
    pub stars: StarService,
    pub tokens: AccessTokenService,
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

        Ok(Self {
            accounts: AccountService::new(Arc::clone(&config), database.accounts(), kv.sessions(), instance.clone(), ctx.events.clone()),
            admin: AdminService::new(ctx.clone(), instance.clone()),
            insights: InsightsService::new(ctx.clone()),
            orgs: OrgService::new(ctx.clone(), instance.clone()),
            profiles: ProfileService::new(ctx.clone()),
            repo_settings: RepoSettingsService::new(ctx.clone()),
            repositories: RepositoryService::new(config, database.repositories(), ctx.repositories.clone()),
            stars: StarService::new(ctx.clone()),
            tokens: AccessTokenService::new(database.access_tokens(), database.accounts()),
            events: ctx.events.clone(),
            instance,
        })
    }
}
