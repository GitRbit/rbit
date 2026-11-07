//! Borrow the existing repository opener only for private-parent visibility.
use crate::{RepositoryService, ServiceError};
use rbit_core::User;
use rbit_repositories::{
    OwnedRepository,
    forks::{ForkParentAccess, ForkViewer},
};
pub(super) struct ParentAccess<'a> {
    pub repositories: &'a RepositoryService,
    pub viewer: Option<&'a User>,
}
impl ForkParentAccess for ParentAccess<'_> {
    type Error = ServiceError;
    async fn can_open(&self, parent: &OwnedRepository) -> Result<bool, Self::Error> {
        match self.repositories.open(self.viewer, parent.owner.name.as_str(), parent.repo.name.as_str()).await {
            Ok(_) => Ok(true),
            Err(ServiceError::NotFound) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
pub(super) fn viewer(viewer: Option<&User>) -> ForkViewer {
    ForkViewer { account: viewer.map(User::id), is_admin: viewer.is_some_and(|user| user.is_admin) }
}
