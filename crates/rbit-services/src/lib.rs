//! # rbit-services
//!
//! The application layer: one service per area, each enforcing business rules and
//! permissions. Services are HTTP-agnostic so the web UI, the REST API, CLI admin
//! commands and background jobs all share exactly the same behaviour.

pub mod accounts;
pub mod admin;
pub mod archives;
pub mod audit;
pub mod blame;
pub mod collaborators;
pub mod contributions;
mod credentials;
pub mod error;
pub mod events;
// Milestone 3: external sign-in (O).
pub mod external_auth;
pub mod forks;
pub mod insights;
mod insights_recent;
pub mod instance;
pub mod issues;
pub mod jobs;
pub mod lfs;
pub mod limits;
pub mod mail;
pub mod mirrors;
pub mod notification_email;
pub mod notifications;
pub mod orgs;
pub mod preferences;
pub mod profiles;
pub mod protection;
pub mod pulls;
pub mod pushes;
pub mod recovery;
pub mod references;
pub mod releases;
pub mod repo_settings;
pub mod repositories;
pub mod ssh_keys;
pub mod stars;
// Milestone 3: commit statuses (S).
pub mod statuses;
pub mod teams;
pub mod throttle;
pub mod timeline;
pub mod tokens;
pub mod transfers;
pub mod two_factor;
pub mod updates;
mod uploads;
pub mod webhooks;
// Milestone 3: editing files in the browser (E).
pub mod edits;
// Milestone 3: importing and push mirrors (M).
pub mod imports;
pub mod push_mirrors;

use std::sync::Arc;

use rbit_core::Config;
use rbit_git::RepositoryStorage;
use rbit_storage::{Database, KvStore};

pub use accounts::{AccountService, SignUp};
pub use admin::AdminService;
pub use archives::ArchiveService;
pub use audit::AuditService;
pub use blame::BlameService;
pub use collaborators::CollaboratorService;
pub use error::ServiceError;
pub use events::{DomainEvent, EventBus};
pub use external_auth::ExternalAuthService;
pub use forks::ForkService;
pub use insights::InsightsService;
pub use instance::InstanceService;
pub use issues::IssueService;
pub use jobs::JobService;
pub use lfs::LfsService;
pub use limits::{ConcurrencyLimiter, ConcurrencySlot};
pub use mail::MailService;
pub use mirrors::MirrorService;
pub use notification_email::NotificationEmailService;
pub use notifications::NotificationService;
pub use orgs::{CreateOrganization, OrgContext, OrgService, UpdateOrganization};
pub use preferences::PreferenceService;
pub use profiles::ProfileService;
pub use protection::ProtectionService;
pub use pulls::PullService;
pub use pushes::{PendingPush, PushService};
pub use recovery::RecoveryService;
pub use references::ReferenceService;
pub use releases::ReleaseService;
pub use repo_settings::RepoSettingsService;
pub use repositories::{CreateRepository, RepoContext, RepoSearch, RepoSearchResults, RepositoryService};
pub use ssh_keys::SshKeyService;
pub use stars::{StarService, StarSummary};
pub use statuses::StatusService;
pub use teams::TeamService;
pub use throttle::{ThrottleScope, ThrottleService};
pub use timeline::{IssueContext, TimelineService};
pub use tokens::{AccessTokenService, CreateAccessToken};
pub use transfers::TransferService;
pub use two_factor::TwoFactorService;
pub use webhooks::WebhookService;
// Milestone 3: editing files in the browser (E).
pub use edits::EditService;
// Milestone 3: importing and push mirrors (M).
pub use imports::ImportService;
pub use push_mirrors::PushMirrorService;

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
    pub updates: updates::UpdateService,
    pub contributions: contributions::ContributionService,
    pub accounts: AccountService,
    pub admin: AdminService,
    pub audit: AuditService,
    pub blame: BlameService,
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
    // Milestone 2 wave A: access (A).
    pub collaborators: CollaboratorService,
    pub teams: TeamService,
    pub transfers: TransferService,
    // API & webhooks (W).
    pub webhooks: WebhookService,
    // Repository essentials (R).
    pub archives: ArchiveService,
    pub forks: ForkService,
    pub mirrors: MirrorService,
    pub releases: ReleaseService,
    /// Git transport processes, shared by smart HTTP and SSH (`[repository]
    /// max_git_processes`; per address: `max_git_requests_per_ip`, applied to HTTP).
    pub git_processes: ConcurrencyLimiter,
    // Frontend wave: themes (T).
    pub preferences: PreferenceService,
    // Milestone 2 wave B: the shared issue timeline (T0).
    pub timeline: TimelineService,
    // Issues (I).
    pub issues: IssueService,
    pub references: ReferenceService,
    pub notifications: NotificationService,
    // Pull requests (P).
    pub pulls: PullService,
    // Branch protection (B).
    pub protection: ProtectionService,
    // Milestone 3: commit statuses and required checks (S).
    pub statuses: StatusService,
    // Milestone 3: email notifications (N).
    pub notification_email: NotificationEmailService,
    // Milestone 3: editing files in the browser (E).
    pub edits: EditService,
    // Milestone 3: Git LFS (L).
    pub lfs: LfsService,
    // Milestone 3: external sign-in (O).
    pub external_auth: ExternalAuthService,
    // Milestone 3: importing and push mirrors (M).
    pub imports: ImportService,
    pub push_mirrors: PushMirrorService,
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
        // Before anything is published: new hook ids (and event stamps) start above these.
        ctx.events.note_existing_webhooks(database.webhooks().max_id().await?);
        let instance = InstanceService::load(&ctx).await?;
        // Shared so recovery emails use the same delivery queue and the same rate limits.
        let mail = MailService::new(ctx.clone()).with_instance(instance.clone(), &config.mail);
        let throttle = ThrottleService::new(ctx.clone());
        // Shared so the issue and pull request services spend the same content budget and
        // record through the same timeline.
        let timeline = TimelineService::new(ctx.clone(), throttle.clone());
        let references = ReferenceService::new(ctx.clone(), timeline.clone());
        let repositories =
            RepositoryService::new(Arc::clone(&config), database.repositories(), ctx.repositories.clone(), ctx.events.clone());
        // Milestone 3 (N): notification emails share the mail queue and the rate limits.
        let notification_email = NotificationEmailService::new(ctx.clone(), mail.clone(), throttle.clone(), instance.clone());
        // Milestone 3 (O): fails on an invalid `[auth]` section, so the server won't start.
        let external_auth = ExternalAuthService::new(&ctx, throttle.clone())?;
        // Milestone 3 (M): the importer stores release files and LFS objects through the
        // same services (LFS placement shares a lock with the orphan sweep).
        let releases = ReleaseService::new(ctx.clone());
        let lfs = LfsService::new(ctx.clone());
        let setup_token = accounts::bootstrap_setup_token(&config, &database.accounts()).await?;
        let accounts =
            AccountService::new(Arc::clone(&config), database.accounts(), kv.sessions(), instance.clone(), ctx.events.clone(), setup_token);

        let issues = IssueService::new(ctx.clone(), timeline.clone(), references.clone(), throttle.clone());
        issues.index_existing_mentions().await?;

        Ok(Self {
            updates: updates::UpdateService::new(&config.updates),
            accounts: accounts.clone(),
            contributions: contributions::ContributionService::new(ctx.clone()),
            audit: AuditService::new(ctx.clone()),
            blame: BlameService::new(),
            jobs: JobService::new(ctx.clone()),
            mail: mail.clone(),
            recovery: RecoveryService::builder(ctx.clone())
                .mail(mail)
                .throttle(throttle.clone())
                .instance(instance.clone())
                .accounts(accounts.clone())
                .build(),
            ssh_keys: SshKeyService::new(ctx.clone()),
            throttle: throttle.clone(),
            two_factor: TwoFactorService::new(ctx.clone(), instance.clone()),
            admin: AdminService::new(ctx.clone(), instance.clone()),
            insights: InsightsService::new(ctx.clone()),
            orgs: OrgService::new(ctx.clone(), instance.clone()),
            profiles: ProfileService::new(ctx.clone(), accounts, throttle.clone()),
            pushes: PushService::new(ctx.clone()),
            repo_settings: RepoSettingsService::new(ctx.clone()),
            collaborators: CollaboratorService::new(ctx.clone()),
            teams: TeamService::new(ctx.clone()),
            transfers: TransferService::new(ctx.clone()),
            webhooks: WebhookService::new(ctx.clone()),
            archives: ArchiveService::new(ctx.clone()),
            forks: ForkService::new(ctx.clone(), repositories.clone()),
            mirrors: MirrorService::new(ctx.clone()),
            releases: releases.clone(),
            git_processes: ConcurrencyLimiter::new(config.repository.max_git_processes(), config.repository.max_git_requests_per_ip()),
            preferences: PreferenceService::new(ctx.clone()),
            issues,
            references,
            pulls: PullService::with_shared(ctx.clone(), timeline.clone(), throttle.clone()),
            timeline,
            notifications: NotificationService::new(ctx.clone()).with_email(notification_email.clone()),
            protection: ProtectionService::new(ctx.clone()),
            statuses: StatusService::new(ctx.clone()),
            // Milestone 3: email notifications (N).
            notification_email,
            // Milestone 3: editing files in the browser (E).
            edits: EditService::new(ctx.clone(), ProtectionService::new(ctx.clone()), PushService::new(ctx.clone())),
            lfs: lfs.clone(),
            // Milestone 3: external sign-in (O).
            external_auth,
            // Milestone 3: importing and push mirrors (M).
            imports: ImportService::new(ctx.clone(), repositories.clone(), lfs, releases),
            push_mirrors: PushMirrorService::new(ctx.clone()),
            repositories,
            stars: StarService::new(ctx.clone()),
            tokens: AccessTokenService::new(database.access_tokens(), database.accounts()),
            events: ctx.events.clone(),
            instance,
        })
    }

    /// Starts background subscribers and periodic work. Call once, after start-up.
    pub fn spawn_background(&self) {
        self.updates.spawn_background();
        self.jobs.spawn_background();
        self.recovery.spawn_background();
        self.webhooks.spawn_background();
        self.releases.spawn_background();
        self.mirrors.spawn_background();
        self.issues.spawn_background();
        self.notifications.spawn_background();
        self.pulls.spawn_background();
        self.lfs.spawn_background();
        // Milestone 3: importing and push mirrors (M).
        self.imports.spawn_background();
        self.push_mirrors.spawn_background();
    }

    /// Stops background work (jobs, webhook deliveries, mirror fetches, release clean-up,
    /// commit references, notifications, merges) and waits, bounded, for it to finish.
    /// Call during graceful shutdown, before closing the stores.
    pub async fn shutdown(&self) {
        tokio::join!(
            self.updates.shutdown(),
            self.jobs.shutdown(),
            self.webhooks.shutdown(),
            self.releases.shutdown(),
            self.mirrors.shutdown(),
            self.issues.shutdown(),
            self.notifications.shutdown(),
            self.pulls.shutdown(),
            self.lfs.shutdown(),
            // Milestone 3: importing and push mirrors (M).
            self.imports.shutdown(),
            self.push_mirrors.shutdown(),
        );
    }
}
