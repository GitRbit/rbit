//! Site administration adapter: instance statistics, the dashboard and the
//! restart-required configuration editor, plus user, organisation and repository
//! management driven by [`rbit_access::administration`].
//!
//! The rules (site administrators only, no demoting, suspending or deleting oneself, the
//! last active administrator, deletion confirmations, listing normalisation, settings
//! change descriptions) live in `rbit-access`; the store keeps the last-administrator rule
//! atomic. This adapter removes deleted repositories from disk, publishes events and
//! records the audit log.

mod accounts;
pub mod configuration;
mod repository_deletion;

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

pub use rbit_access::administration::{ListQuery, PAGE_SIZE, Paged};
use rbit_access::{
    administration::{Administration, describe_changes as describe_fields, require_site_admin},
    organizations::{AccessError, Actor},
};
use rbit_core::{AccountId, InstanceSettings, OwnedRepository, RepoId, User};
use rbit_identity::PasswordWork;
pub use rbit_storage::sqlite::admin::{InstanceCounts, OrgRecord, UserRecord};
use rbit_storage::{StorageError, sqlite::admin::AdminStore};

use crate::{
    DomainEvent, InstanceService, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// When this process started serving (approximately: when the services were built).
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

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
    passwords: PasswordWork,
    ctx: ServiceContext,
    instance: InstanceService,
    store: AdminStore,
    audit: AuditService,
}

impl AdminService {
    pub(crate) fn new(ctx: ServiceContext, instance: InstanceService, passwords: PasswordWork) -> Self {
        STARTED_AT.get_or_init(Instant::now);
        let store = ctx.database.admin();
        let audit = AuditService::from_store(ctx.database.audit());
        Self { passwords, ctx, instance, store, audit }
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

    fn rules(&self) -> Administration<'_, AdminStore> {
        Administration::new(&self.store)
    }

    pub async fn users(&self, actor: &User, query: &ListQuery) -> Result<Paged<UserRecord>, ServiceError> {
        self.rules().users(&self::actor(actor), query).await.map_err(service_error)
    }

    pub async fn user(&self, actor: &User, id: AccountId) -> Result<UserRecord, ServiceError> {
        self.rules().user(&self::actor(actor), id).await.map_err(service_error)
    }

    /// Grants or revokes site administrator rights.
    pub async fn set_admin(&self, actor: &User, id: AccountId, is_admin: bool) -> Result<UserRecord, ServiceError> {
        let target = self.rules().set_admin(&self::actor(actor), id, is_admin).await.map_err(service_error)?;
        tracing::info!(admin = %actor.name(), user = %target.user.name(), is_admin, "administrator rights changed");
        let action = if is_admin { AuditAction::AdminGranted } else { AuditAction::AdminRevoked };
        self.audit.record(AuditEntry::new(action).by(actor).on(target.user.name().as_str())).await;
        Ok(target)
    }

    /// Suspends a user (they can no longer sign in, use sessions, tokens or Git) or lifts it.
    pub async fn set_suspended(&self, actor: &User, id: AccountId, suspended: bool) -> Result<UserRecord, ServiceError> {
        let target = self.rules().set_suspended(&self::actor(actor), id, suspended).await.map_err(service_error)?;
        tracing::info!(admin = %actor.name(), user = %target.user.name(), suspended, "user suspension changed");
        let action = if suspended { AuditAction::UserSuspended } else { AuditAction::UserUnsuspended };
        self.audit.record(AuditEntry::new(action).by(actor).on(target.user.name().as_str())).await;
        Ok(target)
    }

    /// Deletes a user, everything they own in the database and their repositories on disk.
    /// `confirmation` must repeat the username exactly.
    pub async fn delete_user(&self, actor: &User, id: AccountId, confirmation: &str) -> Result<UserRecord, ServiceError> {
        let deleted = self.rules().delete_user(&self::actor(actor), id, confirmation).await.map_err(service_error)?;
        let target = deleted.user;
        for (repo, name) in deleted.repositories {
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
        self.rules().organizations(&self::actor(actor), query).await.map_err(service_error)
    }

    // ------------------------------------------------------------------ repositories

    pub async fn repositories(&self, actor: &User, query: &ListQuery) -> Result<Paged<OwnedRepository>, ServiceError> {
        self.rules().repositories(&self::actor(actor), query).await.map_err(service_error)
    }

    pub async fn repository(&self, actor: &User, id: RepoId) -> Result<OwnedRepository, ServiceError> {
        self.rules().repository(&self::actor(actor), id).await.map_err(service_error)
    }

    /// The database row is already gone, so a failure here leaves only an unreachable
    /// directory behind: log it loudly rather than failing the request.
    async fn remove_from_disk(&self, owner: AccountId, repo: RepoId) {
        if let Err(error) = self.ctx.repositories.delete(owner, repo).await {
            tracing::error!(%error, owner = owner.0, repo = repo.0, "failed to delete repository from disk");
        }
    }
}

/// The settings an administrator can change, as the audit description compares them.
fn describe_changes(before: &InstanceSettings, after: &InstanceSettings) -> String {
    let (hour_before, hour_after) = (format!("{:02}:00 UTC", before.digest_hour), format!("{:02}:00 UTC", after.digest_hour));
    let fields = [
        ("site name", before.site_name.as_str(), after.site_name.as_str()),
        ("registration", before.registration.as_str(), after.registration.as_str()),
        ("organization creation", before.org_creation.as_str(), after.org_creation.as_str()),
        ("default visibility", before.default_visibility.as_str(), after.default_visibility.as_str()),
        ("email digest hour", hour_before.as_str(), hour_after.as_str()),
    ];
    describe_fields(before.branding != after.branding, &fields)
}

/// The identity layer's facts about the acting user, for the administration rules.
fn actor(user: &User) -> Actor<AccountId> {
    Actor { id: user.id(), site_admin: user.is_admin }
}

pub(crate) fn require_admin(actor: &User) -> Result<(), ServiceError> {
    require_site_admin::<StorageError, _>(&self::actor(actor)).map_err(service_error)
}

fn service_error(error: AccessError<StorageError>) -> ServiceError {
    crate::orgs::service_error(error)
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
    fn measures_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), [0_u8; 10]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("b"), [0_u8; 32]).unwrap();
        assert_eq!(directory_size(dir.path()), Some(42));
        assert_eq!(directory_size(&dir.path().join("missing")), None);
    }
}
