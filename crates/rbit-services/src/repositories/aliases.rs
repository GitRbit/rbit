//! Request-local alias access without a cloned store or service locator.
use super::{OwnedRepository, RepoContext, RepositoryService, ServiceError, User, repo_access, viewer_grants};
use rbit_repositories::aliases::AliasPort;

#[derive(Debug)]
pub(super) struct AliasView<'a> {
    pub service: &'a RepositoryService,
    pub viewer: Option<&'a User>,
}

impl AliasPort for AliasView<'_> {
    type Error = ServiceError;
    type Opened = RepoContext;
    async fn find_current(&self, owner: &str, name: &str) -> Result<Option<OwnedRepository>, Self::Error> {
        Ok(self.service.store.find_by_path(owner, name).await?)
    }
    async fn find_old(&self, owner: &str, name: &str) -> Result<Option<OwnedRepository>, Self::Error> {
        Ok(self.service.store.find_by_old_path(owner, name).await?)
    }
    async fn open(&self, repository: OwnedRepository) -> Result<RepoContext, Self::Error> {
        self.service.open_found(self.viewer, repository).await
    }
    async fn readable(&self, repository: &OwnedRepository) -> Result<bool, Self::Error> {
        let grants = viewer_grants(&self.service.store, self.viewer, repository).await?;
        Ok(repo_access(self.viewer, repository, &grants).can_read())
    }
    fn not_found(&self) -> Self::Error {
        ServiceError::NotFound
    }
}
