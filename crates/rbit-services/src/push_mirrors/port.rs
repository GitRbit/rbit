//! Concrete stores, the system repository loader and the push itself behind the
//! feature's push mirror port. No push mirror policy lives here.
use super::PushMirrorService;
use crate::{RepoContext, ServiceError};
use rbit_core::{Timestamp, push_mirror::PushMirrorId};
use rbit_repositories::push_mirrors::MirrorPush;
use rbit_storage::sqlite::push_mirrors::PushTarget;

/// Borrows the service; holds no policy.
pub(super) struct PushView<'a>(pub(super) &'a PushMirrorService);

impl MirrorPush for PushView<'_> {
    type Error = ServiceError;
    type Id = PushMirrorId;
    type Target = PushTarget;
    type Repository = RepoContext;

    async fn target(&self, id: PushMirrorId) -> Result<Option<PushTarget>, ServiceError> {
        Ok(self.0.store().target(id).await?)
    }

    async fn claim(&self, id: PushMirrorId, next: Timestamp) -> Result<(), ServiceError> {
        Ok(self.0.store().claim(id, next).await?)
    }

    async fn repository(&self, target: &PushTarget) -> Result<Option<RepoContext>, ServiceError> {
        RepoContext::load_for_system(&self.0.inner.ctx, target.mirror.repo_id).await
    }

    fn is_archived(&self, repository: &RepoContext) -> bool {
        repository.repo.repo.is_archived()
    }

    async fn push(&self, repository: &RepoContext, target: &PushTarget) -> Result<(), String> {
        self.0.attempt(repository, target).await
    }

    fn observe(&self, repository: &RepoContext, id: PushMirrorId, error: Option<&str>) {
        match error {
            None => tracing::info!(repo = %repository.repo.full_name(), mirror = id.0, "push mirror updated"),
            Some(error) => tracing::warn!(repo = %repository.repo.full_name(), mirror = id.0, %error, "push mirror failed"),
        }
        metrics::counter!("rbit_push_mirror_pushes_total", "outcome" => if error.is_none() { "ok" } else { "error" }).increment(1);
    }

    async fn record(&self, id: PushMirrorId, at: Timestamp, error: Option<&str>) -> Result<(), ServiceError> {
        Ok(self.0.store().record_attempt(id, at, error).await?)
    }
}
