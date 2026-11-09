//! Concrete stores, the system repository loader, progress and the fetch itself behind
//! the feature's mirror sync port. No mirror policy lives here.
use super::{MirrorService, lock};
use crate::{RepoContext, ServiceError};
use rbit_core::{RepoId, Timestamp, mirror::Mirror};
use rbit_repositories::mirrors::{MirrorPhase, MirrorSync};

/// Borrows the service; holds no policy.
pub(super) struct SyncView<'a>(pub(super) &'a MirrorService);

impl MirrorSync for SyncView<'_> {
    type Error = ServiceError;
    type Mirror = Mirror;
    type Repository = RepoContext;

    async fn mirror(&self, repo: RepoId) -> Result<Option<Mirror>, ServiceError> {
        Ok(self.0.store().find(repo).await?)
    }

    async fn repository(&self, repo: RepoId) -> Result<Option<RepoContext>, ServiceError> {
        RepoContext::load_for_system(&self.0.inner.ctx, repo).await
    }

    fn is_archived(&self, repository: &RepoContext) -> bool {
        repository.repo.repo.is_archived()
    }

    fn set_phase(&self, repo: RepoId, phase: Option<MirrorPhase>) {
        let mut phases = lock(&self.0.inner.phases);
        match phase {
            Some(phase) => phases.insert(repo, phase),
            None => phases.remove(&repo),
        };
    }

    async fn fetch(&self, repository: &RepoContext, mirror: &Mirror) -> Result<(), String> {
        self.0.fetch(repository, mirror).await
    }

    fn observe(&self, repository: &RepoContext, error: Option<&str>) {
        match error {
            None => tracing::info!(repo = %repository.repo.full_name(), "mirror fetched"),
            Some(error) => tracing::warn!(repo = %repository.repo.full_name(), %error, "mirror fetch failed"),
        }
        metrics::counter!("rbit_mirror_fetches_total", "outcome" => if error.is_none() { "ok" } else { "error" }).increment(1);
    }

    async fn record(&self, repo: RepoId, at: Timestamp, error: Option<&str>) -> Result<(), ServiceError> {
        Ok(self.0.store().record_sync(repo, at, error).await?)
    }
}
