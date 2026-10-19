//! Site administration: instance statistics and user, organisation and repository management.
//!
//! Every operation takes the acting user and fails with [`ServiceError::Forbidden`] unless
//! they are a site administrator. Guard rails keep the instance manageable: administrators
//! cannot demote, suspend or delete themselves, and the last active administrator can
//! never be demoted, suspended or deleted (enforced atomically by the store).

pub mod configuration;

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

use rbit_core::{AccountId, InstanceSettings, OwnedRepository, RepoId, User};
use rbit_storage::sqlite::admin::{AdminStore, ListFilter, Listing, Moderation};
pub use rbit_storage::sqlite::admin::{InstanceCounts, OrgRecord, UserRecord};

use crate::{
    DomainEvent, InstanceService, ServiceContext, ServiceError, SignUp, accounts,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// Rows per page in administration listings.
pub const PAGE_SIZE: u32 = 30;
/// Longest accepted search text; longer input is truncated.
const MAX_SEARCH_LEN: usize = 100;

/// When this process started serving (approximately: when the services were built).
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

/// Which page of a listing to show and what to filter by, as requested.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListQuery {
    pub search: String,
    /// 1-based; `0` is treated as `1`.
    pub page: u32,
}

impl ListQuery {
    /// The trimmed, length-capped search text.
    pub fn search(&self) -> &str {
        let search = self.search.trim();
        match search.char_indices().nth(MAX_SEARCH_LEN) {
            Some((end, _)) => &search[..end],
            None => search,
        }
    }

    pub fn page(&self) -> u32 {
        self.page.max(1)
    }

    pub(crate) fn filter(&self) -> ListFilter<'_> {
        ListFilter { search: self.search(), limit: i64::from(PAGE_SIZE), offset: i64::from(self.page() - 1) * i64::from(PAGE_SIZE) }
    }
}

/// One page of an administration listing.
#[derive(Debug, Clone)]
pub struct Paged<T> {
    pub items: Vec<T>,
    /// Matching rows across all pages.
    pub total: u64,
    /// 1-based.
    pub page: u32,
    pub per_page: u32,
}

impl<T> Paged<T> {
    pub(crate) fn new(listing: Listing<T>, query: &ListQuery) -> Self {
        Self { items: listing.items, total: listing.total.try_into().unwrap_or(0), page: query.page(), per_page: PAGE_SIZE }
    }

    pub fn has_previous(&self) -> bool {
        self.page > 1
    }

    pub fn has_next(&self) -> bool {
        u64::from(self.page) * u64::from(self.per_page) < self.total
    }

    /// 1-based index of the first item on this page (0 when the page is empty).
    pub fn first_index(&self) -> u64 {
        if self.items.is_empty() { 0 } else { u64::from(self.page - 1) * u64::from(self.per_page) + 1 }
    }

    /// 1-based index of the last item on this page (0 when the page is empty).
    pub fn last_index(&self) -> u64 {
        if self.items.is_empty() { 0 } else { self.first_index() + self.items.len() as u64 - 1 }
    }
}

/// The host the instance runs on, for the dashboard.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub rbit_version: &'static str,
    pub sqlite_version: String,
    /// `None` when the configured Git binary could not be run.
    pub git_version: Option<String>,
    pub uptime: Duration,
    pub data_dir: PathBuf,
    /// Total size of every file under the data directory; `None` if it could not be read.
    pub data_dir_bytes: Option<u64>,
}

/// Everything the administration dashboard shows.
#[derive(Debug, Clone)]
pub struct Dashboard {
    pub counts: InstanceCounts,
    pub system: SystemInfo,
    pub settings: InstanceSettings,
}

#[derive(Debug, Clone)]
pub struct AdminService {
    ctx: ServiceContext,
    instance: InstanceService,
    store: AdminStore,
    audit: AuditService,
}

impl AdminService {
    pub(crate) fn new(ctx: ServiceContext, instance: InstanceService) -> Self {
        STARTED_AT.get_or_init(Instant::now);
        let store = ctx.database.admin();
        let audit = AuditService::from_store(ctx.database.audit());
        Self { ctx, instance, store, audit }
    }

    // ------------------------------------------------------------------ dashboard

    pub async fn dashboard(&self, actor: &User) -> Result<Dashboard, ServiceError> {
        require_admin(actor)?;
        let counts = self.store.counts().await?;
        let sqlite_version = self.store.sqlite_version().await?;

        let git_binary = self.ctx.config.repository.git_binary.clone();
        let data_dir = self.ctx.config.storage.data_dir.clone();
        let walked_dir = data_dir.clone();
        let (git_version, data_dir_bytes) = tokio::task::spawn_blocking(move || (git_version(&git_binary), directory_size(&walked_dir)))
            .await
            .map_err(ServiceError::internal)?;

        Ok(Dashboard {
            counts,
            system: SystemInfo {
                rbit_version: env!("CARGO_PKG_VERSION"),
                sqlite_version,
                git_version,
                uptime: STARTED_AT.get().map_or(Duration::ZERO, Instant::elapsed),
                data_dir,
                data_dir_bytes,
            },
            settings: self.instance.current(),
        })
    }

    // ------------------------------------------------------------------ settings

    /// Saves the runtime instance settings (see [`InstanceService::update`]) and records
    /// what changed in the audit log. Use this rather than calling the instance service
    /// directly from the administration panel.
    pub async fn update_settings(&self, actor: &User, settings: InstanceSettings) -> Result<(), ServiceError> {
        let before = self.instance.current();
        self.instance.update(actor, settings).await?;
        let changes = describe_changes(&before, &self.instance.current());
        if !changes.is_empty() {
            self.audit.record(AuditEntry::new(AuditAction::InstanceSettingsChanged).by(actor).detail(changes)).await;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ users

    pub async fn users(&self, actor: &User, query: &ListQuery) -> Result<Paged<UserRecord>, ServiceError> {
        require_admin(actor)?;
        Ok(Paged::new(self.store.list_users(query.filter()).await?, query))
    }

    pub async fn user(&self, actor: &User, id: AccountId) -> Result<UserRecord, ServiceError> {
        require_admin(actor)?;
        self.store.find_user(id).await?.ok_or(ServiceError::NotFound)
    }

    /// Creates an account. Works even while registration is closed.
    pub async fn create_user(&self, actor: &User, input: SignUp, is_admin: bool) -> Result<User, ServiceError> {
        require_admin(actor)?;
        let new = accounts::prepare_user(&self.ctx.config, input, is_admin).await?;
        let user = self.ctx.database.accounts().create_user_with_initial_password(new).await?;
        tracing::info!(admin = %actor.name(), user = %user.name(), is_admin, "administrator created account");
        // Subscribers (e.g. email verification) treat every new account the same way.
        self.ctx.events.publish(DomainEvent::UserSignedUp { user: user.id() });
        let detail = if is_admin { "as an administrator" } else { "" };
        self.audit.record(AuditEntry::new(AuditAction::UserCreated).by(actor).on(user.name().as_str()).detail(detail)).await;
        Ok(user)
    }

    /// Grants or revokes site administrator rights.
    pub async fn set_admin(&self, actor: &User, id: AccountId, is_admin: bool) -> Result<UserRecord, ServiceError> {
        let target = self.user(actor, id).await?;
        if !is_admin {
            refuse_self(actor, &target, "remove your own administrator rights")?;
        }
        check_moderation(self.store.set_admin(id, is_admin).await?, &target, "demoted")?;
        tracing::info!(admin = %actor.name(), user = %target.user.name(), is_admin, "administrator rights changed");
        let action = if is_admin { AuditAction::AdminGranted } else { AuditAction::AdminRevoked };
        self.audit.record(AuditEntry::new(action).by(actor).on(target.user.name().as_str())).await;
        Ok(target)
    }

    /// Suspends a user (they can no longer sign in, use sessions, tokens or Git) or lifts it.
    pub async fn set_suspended(&self, actor: &User, id: AccountId, suspended: bool) -> Result<UserRecord, ServiceError> {
        let target = self.user(actor, id).await?;
        if suspended {
            refuse_self(actor, &target, "suspend yourself")?;
        }
        check_moderation(self.store.set_suspended(id, suspended).await?, &target, "suspended")?;
        tracing::info!(admin = %actor.name(), user = %target.user.name(), suspended, "user suspension changed");
        let action = if suspended { AuditAction::UserSuspended } else { AuditAction::UserUnsuspended };
        self.audit.record(AuditEntry::new(action).by(actor).on(target.user.name().as_str())).await;
        Ok(target)
    }

    /// Deletes a user, everything they own in the database and their repositories on disk.
    /// `confirmation` must repeat the username exactly.
    pub async fn delete_user(&self, actor: &User, id: AccountId, confirmation: &str) -> Result<UserRecord, ServiceError> {
        let target = self.user(actor, id).await?;
        if confirmation.trim() != target.user.name().as_str() {
            return Err(ServiceError::Invalid("type the username exactly to confirm".to_owned()));
        }
        refuse_self(actor, &target, "delete your own account here")?;
        let orphaned = self.store.organizations_solely_owned_by(id).await?;
        if !orphaned.is_empty() {
            return Err(ServiceError::Invalid(format!(
                "{} is the only owner of {}; add another owner or delete them first",
                target.user.name(),
                orphaned.join(", ")
            )));
        }

        let repositories = self.store.repositories_owned_by(id).await?;
        check_moderation(self.store.delete_user(id).await?, &target, "deleted")?;
        for (repo, name) in repositories {
            self.remove_from_disk(id, repo).await;
            let full_name = format!("{}/{name}", target.user.name());
            self.ctx.events.publish(DomainEvent::RepositoryDeleted { repo, owner: id, full_name, actor: actor.id() });
        }
        tracing::info!(admin = %actor.name(), user = %target.user.name(), "user deleted");
        self.audit.record(AuditEntry::new(AuditAction::UserDeleted).by(actor).on(target.user.name().as_str())).await;
        Ok(target)
    }

    // ------------------------------------------------------------------ organisations

    pub async fn organizations(&self, actor: &User, query: &ListQuery) -> Result<Paged<OrgRecord>, ServiceError> {
        require_admin(actor)?;
        Ok(Paged::new(self.store.list_organizations(query.filter()).await?, query))
    }

    // ------------------------------------------------------------------ repositories

    pub async fn repositories(&self, actor: &User, query: &ListQuery) -> Result<Paged<OwnedRepository>, ServiceError> {
        require_admin(actor)?;
        Ok(Paged::new(self.store.list_repositories(query.filter()).await?, query))
    }

    pub async fn repository(&self, actor: &User, id: RepoId) -> Result<OwnedRepository, ServiceError> {
        require_admin(actor)?;
        self.store.find_repository(id).await?.ok_or(ServiceError::NotFound)
    }

    /// Deletes a repository's metadata and its bare repository on disk.
    /// `confirmation` must repeat `owner/name` exactly.
    pub async fn delete_repository(&self, actor: &User, id: RepoId, confirmation: &str) -> Result<OwnedRepository, ServiceError> {
        let repo = self.repository(actor, id).await?;
        if confirmation.trim() != repo.full_name() {
            return Err(ServiceError::Invalid("type the repository's full name exactly to confirm".to_owned()));
        }
        if !self.store.delete_repository(id).await? {
            return Err(ServiceError::NotFound);
        }
        self.remove_from_disk(repo.owner.id, id).await;
        tracing::info!(admin = %actor.name(), repository = %repo.full_name(), "repository deleted");
        self.ctx.events.publish(DomainEvent::RepositoryDeleted {
            repo: id,
            owner: repo.owner.id,
            full_name: repo.full_name(),
            actor: actor.id(),
        });
        self.audit.record(AuditEntry::new(AuditAction::RepositoryDeleted).by(actor).on(repo.full_name())).await;
        Ok(repo)
    }

    /// The database row is already gone, so a failure here leaves only an unreachable
    /// directory behind: log it loudly rather than failing the request.
    async fn remove_from_disk(&self, owner: AccountId, repo: RepoId) {
        if let Err(error) = self.ctx.repositories.delete(owner, repo).await {
            tracing::error!(%error, owner = owner.0, repo = repo.0, "failed to delete repository from disk");
        }
    }
}

/// e.g. `registration: open → closed; site name: rbit → Forge`.
fn describe_changes(before: &InstanceSettings, after: &InstanceSettings) -> String {
    let mut description = if before.branding == after.branding { String::new() } else { "branding changed; ".to_owned() };
    let fields = [
        ("site name", before.site_name.as_str(), after.site_name.as_str()),
        ("registration", before.registration.as_str(), after.registration.as_str()),
        ("organization creation", before.org_creation.as_str(), after.org_creation.as_str()),
        ("default visibility", before.default_visibility.as_str(), after.default_visibility.as_str()),
    ];
    description.push_str(
        &fields
            .iter()
            .filter(|(_, old, new)| old != new)
            .map(|(name, old, new)| format!("{name}: {old} → {new}"))
            .collect::<Vec<_>>()
            .join("; "),
    );
    description
}

pub(crate) fn require_admin(actor: &User) -> Result<(), ServiceError> {
    if actor.is_admin { Ok(()) } else { Err(ServiceError::Forbidden) }
}

fn refuse_self(actor: &User, target: &UserRecord, action: &str) -> Result<(), ServiceError> {
    if actor.id() == target.user.id() { Err(ServiceError::Invalid(format!("you can't {action}"))) } else { Ok(()) }
}

fn check_moderation(outcome: Moderation, target: &UserRecord, verb: &str) -> Result<(), ServiceError> {
    match outcome {
        Moderation::Applied => Ok(()),
        Moderation::NotFound => Err(ServiceError::NotFound),
        Moderation::LastAdmin => Err(ServiceError::Invalid(format!(
            "{} is the last active administrator and can't be {verb}; promote someone else first",
            target.user.name()
        ))),
    }
}

/// `git --version` of the configured binary, e.g. `2.51.0`. Blocking.
fn git_version(binary: &Path) -> Option<String> {
    let output = std::process::Command::new(binary).arg("--version").env("GIT_CONFIG_NOSYSTEM", "1").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    Some(text.strip_prefix("git version ").unwrap_or(text).to_owned())
}

/// Sum of file sizes below `root`, without following symbolic links. Blocking.
fn directory_size(root: &Path) -> Option<u64> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            // The root itself must be readable; unreadable subdirectories are skipped.
            Err(_) if dir != root => continue,
            Err(_) => return None,
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else { continue };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_normalises_input() {
        let query = ListQuery { search: format!("  {}  ", "é".repeat(150)), page: 0 };
        assert_eq!(query.search().chars().count(), MAX_SEARCH_LEN);
        assert_eq!(query.page(), 1);
        assert_eq!(query.filter().offset, 0);
        assert_eq!(ListQuery { search: String::new(), page: 3 }.filter().offset, 60);
    }

    #[test]
    fn paging_bounds() {
        let query = ListQuery { search: String::new(), page: 2 };
        let paged = Paged::new(Listing { items: vec![1; 5], total: 35 }, &query);
        assert!(paged.has_previous());
        assert!(!paged.has_next());
        assert_eq!((paged.first_index(), paged.last_index()), (31, 35));

        let first = Paged::new(Listing { items: vec![1; 30], total: 35 }, &ListQuery::default());
        assert!(!first.has_previous());
        assert!(first.has_next());

        let empty = Paged::<u8>::new(Listing { items: Vec::new(), total: 0 }, &ListQuery::default());
        assert_eq!((empty.first_index(), empty.last_index()), (0, 0));
    }

    #[test]
    fn measures_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), [0_u8; 10]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("b"), [0_u8; 32]).unwrap();
        assert_eq!(directory_size(dir.path()), Some(42));
        assert_eq!(directory_size(&dir.path().join("missing")), None);
    }
}
