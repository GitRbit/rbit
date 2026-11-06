//! Metadata → disk → creator grant → publication, preserving original compensation/error order.
use super::{
    CreateRepository, NewRepository, RepositoryCreationPort, RepositoryCreationRepository, RepositoryMembership, RepositoryOwnerCatalog,
    RepositoryOwnerError, RepositoryOwnership,
};
use crate::{OwnedRepository, RepoName, names::NameError};
use rbit_identity_models::User;
const MAX_DESCRIPTION_LEN: usize = 350;
#[derive(Debug)]
pub enum RepositoryCreationError<S, E> {
    Owner(RepositoryOwnerError<S>),
    Name(NameError),
    Invalid(String),
    Store(S),
    Disk(E),
}
pub struct RepositoryCreation<'a, P> {
    port: &'a P,
}
impl<P> std::fmt::Debug for RepositoryCreation<'_, P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Infrastructure is neither a diagnostic field nor a required Debug bound.
        formatter.debug_struct("RepositoryCreation").finish_non_exhaustive()
    }
}
impl<'a, P: RepositoryCreationPort> RepositoryCreation<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    pub async fn create_in(
        &self,
        actor: &User,
        owner: &str,
        input: CreateRepository,
    ) -> Result<OwnedRepository, RepositoryCreationError<<P::Store as RepositoryOwnerCatalog>::Error, P::Error>> {
        let owner =
            RepositoryOwnership::resolve(self.port.repository(), actor, owner.trim()).await.map_err(RepositoryCreationError::Owner)?;
        let name: RepoName = input.name.trim().parse().map_err(RepositoryCreationError::Name)?;
        let description = input.description.trim();
        if description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(RepositoryCreationError::Invalid(format!("description must be at most {MAX_DESCRIPTION_LEN} characters")));
        }

        let default_branch = self.port.default_branch().to_owned();
        let repo = self
            .port
            .repository()
            .create(NewRepository {
                owner_id: owner.id,
                name,
                description: (!description.is_empty()).then(|| description.to_owned()),
                visibility: input.visibility,
                default_branch: default_branch.clone(),
            })
            .await
            .map_err(RepositoryCreationError::Store)?;

        if let Err(error) = self.port.create_disk(owner.id, repo.id, &default_branch).await {
            self.port.disk_failed(&error, &repo);
            self.port.repository().delete(repo.id).await.map_err(RepositoryCreationError::Store)?;
            return Err(RepositoryCreationError::Disk(error));
        }

        // A member who is not an owner would otherwise depend on the organisation's base
        // permission to reach the repository they just created (with `none`, not at all),
        // so the creator administers it as a collaborator, as on other forges.
        if owner.is_organization()
            && !actor.is_admin
            && self.port.repository().membership(owner.id, actor.id()).await.map_err(RepositoryCreationError::Store)?
                == Some(RepositoryMembership::Member)
        {
            self.port.repository().grant_creator_admin(repo.id, actor.id()).await.map_err(RepositoryCreationError::Store)?;
        }

        self.port.created(actor, &owner, &repo);
        Ok(OwnedRepository { owner, repo })
    }
}
