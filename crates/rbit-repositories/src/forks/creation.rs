//! Original fork workflow and compensation precedence; no native implementation in policy.
use super::{ForkCreationPort, ForkCreationRepository, ForkInput};
use crate::{OwnedRepository, RepoName, creation::NewRepository, names::NameError};
use rbit_identity_models::User;
#[derive(Debug)]
pub enum ForkCreationError<S, E> {
    NotFound,
    Name(NameError),
    Owner(E),
    Store(S),
    Copy(E),
}
pub struct ForkCreation<'a, P> {
    port: &'a P,
}
impl<P> std::fmt::Debug for ForkCreation<'_, P> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ForkCreation").finish_non_exhaustive()
    }
}
impl<'a, P: ForkCreationPort> ForkCreation<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    pub async fn fork(
        &self,
        actor: &User,
        parent: &OwnedRepository,
        readable: bool,
        input: ForkInput,
    ) -> Result<OwnedRepository, ForkCreationError<<P::Store as ForkCreationRepository>::Error, P::Error>> {
        if !readable {
            return Err(ForkCreationError::NotFound);
        }
        let owner = self.port.owner(actor, input.owner.trim()).await.map_err(ForkCreationError::Owner)?;
        let name: RepoName = match input.name.trim() {
            "" => parent.repo.name.clone(),
            name => name.parse().map_err(ForkCreationError::Name)?,
        };
        let source = &parent.repo;
        let store = self.port.store();
        let repo = store
            .create(NewRepository {
                owner_id: owner.id,
                name,
                description: source.description.clone(),
                visibility: source.visibility,
                default_branch: source.default_branch.clone(),
            })
            .await
            .map_err(ForkCreationError::Store)?;
        let copied = async {
            self.port.copy_disk(source.id, owner.id, repo.id).await?;
            self.port.record(repo.id, source.id).await?;
            Ok::<_, P::Error>(())
        };
        if let Err(error) = copied.await {
            self.port.copy_failed(&error, parent);
            if let Err(error) = self.port.delete_disk(owner.id, repo.id).await {
                self.port.cleanup_failed(&error);
            }
            store.delete(repo.id).await.map_err(ForkCreationError::Store)?;
            return Err(ForkCreationError::Copy(error));
        }
        if source.pushed_at.is_some()
            && let Err(error) = store.mark_pushed(repo.id).await
        {
            self.port.pushed_failed(&error);
        }
        self.port.created(actor, parent, &owner, &repo);
        Ok(OwnedRepository { owner, repo })
    }
}
