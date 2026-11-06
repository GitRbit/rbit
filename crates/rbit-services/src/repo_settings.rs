//! Repository settings use feature-owned authority and explicit narrow infrastructure.
//! Settings remain available while archived; content writes retain their separate guard.
use crate::{AuditService, EventBus, RepoContext, ServiceError};
use rbit_core::{RepoName, Timestamp, User};
use rbit_git::RepositoryStorage;
pub use rbit_repositories::settings::UpdateRepository;
use rbit_repositories::settings::{RepositorySettings, RepositorySettingsError};
use rbit_storage::Database;
mod effects;
use effects::SettingsEffects;
#[derive(Debug, Clone)]
pub struct RepoSettingsService {
    database: Database,
    storage: RepositoryStorage,
    events: EventBus,
    audit: AuditService,
}
fn error<S: Into<ServiceError>, E: Into<ServiceError>>(error: RepositorySettingsError<S, E>) -> ServiceError {
    match error {
        RepositorySettingsError::Forbidden => ServiceError::Forbidden,
        RepositorySettingsError::Invalid(message) => ServiceError::Invalid(message),
        RepositorySettingsError::Name(error) => error.into(),
        RepositorySettingsError::Store(error) => error.into(),
        RepositorySettingsError::Effect(error) => error.into(),
    }
}
impl RepoSettingsService {
    pub(crate) fn new(database: Database, storage: RepositoryStorage, events: EventBus) -> Self {
        Self { audit: AuditService::from_store(database.audit()), database, storage, events }
    }
    fn effects<'a>(&'a self, repo: &'a RepoContext) -> SettingsEffects<'a> {
        SettingsEffects { database: &self.database, storage: &self.storage, events: &self.events, audit: &self.audit, repo }
    }
    /// Updates description/visibility/default branch. Changed HEAD is restored on SQL failure.
    pub async fn update(&self, repo: &RepoContext, input: UpdateRepository) -> Result<(), ServiceError> {
        RepositorySettings::new(&repo.repo, repo.access.can_admin()).update(&self.effects(repo), input).await.map_err(error)
    }
    /// A same-name rename performs no SQL; paths remain keyed by stable repository ID.
    pub async fn rename(&self, repo: &RepoContext, new_name: &str) -> Result<RepoName, ServiceError> {
        RepositorySettings::new(&repo.repo, repo.access.can_admin()).rename(&self.effects(repo), new_name).await.map_err(error)
    }
    /// Only the conditional SQL winner is audited and published; settings work when archived.
    pub async fn set_archived(&self, actor: &User, repo: &RepoContext, archived: bool) -> Result<bool, ServiceError> {
        RepositorySettings::new(&repo.repo, repo.access.can_admin())
            .set_archived(actor, &self.effects(repo), archived, Timestamp::now)
            .await
            .map_err(error)
    }
    /// Confirmation precedes closing the reader; disk deletion precedes metadata, as before.
    pub async fn delete(&self, actor: &User, repo: &RepoContext, confirmation: &str) -> Result<(), ServiceError> {
        RepositorySettings::new(&repo.repo, repo.access.can_admin()).delete(actor, &self.effects(repo), confirmation).await.map_err(error)
    }
}
