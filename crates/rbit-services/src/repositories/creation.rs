//! Original public creation API composes static feature authority with concrete Git/stores/events.
use std::time::Duration;

use super::{CreateRepository, RepoContext, RepositoryService};
use crate::{DomainEvent, LfsService, ServiceError};
use rbit_core::{
    Account, AccountId, RepoId, Repository, User, Visibility,
    pull::{CommitIdentity, noreply_domain},
};
use rbit_git::{remote::lfs_pointers, template::copy_snapshot};
use rbit_repositories::{
    OwnedRepository,
    creation::{RepositoryCreation, RepositoryCreationError, RepositoryCreationPort, RepositoryOwnerError},
};
use rbit_storage::{RepositoryStore, StorageError};

const TEMPLATE_TIMEOUT: Duration = Duration::from_mins(10);

struct CreationEffects<'a> {
    service: &'a RepositoryService,
    template: Option<TemplateSource<'a>>,
}

struct TemplateSource<'a> {
    repo: &'a RepoContext,
    commit: &'a str,
    actor: &'a User,
    lfs: &'a LfsService,
}
impl RepositoryCreationPort for CreationEffects<'_> {
    type Error = ServiceError;
    type Store = RepositoryStore;
    fn repository(&self) -> &Self::Store {
        &self.service.store
    }
    fn default_branch(&self) -> &str {
        &self.service.config.repository.default_branch
    }
    async fn create_disk(&self, owner: AccountId, repo: RepoId, branch: &str) -> Result<(), Self::Error> {
        let target = self.service.storage.create(owner, repo, branch).await?;
        let Some(template) = &self.template else {
            return Ok(());
        };
        let result = self.seed_from_template(template, repo, (&target, branch)).await;
        if result.is_err()
            && let Err(error) = self.service.storage.delete(owner, repo).await
        {
            tracing::error!(%error, "could not remove an incomplete template repository");
        }
        result
    }
    fn disk_failed(&self, error: &Self::Error, repo: &Repository) {
        tracing::error!(%error,repo=%repo.name,"failed to initialise repository on disk; rolling back");
    }
    fn created(&self, actor: &User, owner: &Account, repo: &Repository) {
        tracing::info!(actor=%actor.name(),owner=%owner.name,repo=%repo.name,"repository created");
        self.service.events.publish(DomainEvent::RepositoryCreated { repo: repo.id, actor: actor.id() });
    }
}

impl CreationEffects<'_> {
    async fn seed_from_template(
        &self,
        template: &TemplateSource<'_>,
        repo: RepoId,
        target: (&std::path::Path, &str),
    ) -> Result<(), ServiceError> {
        let config = &self.service.config;
        let source = template.repo;
        let domain = noreply_domain(&config.pulls.noreply_domain, &config.server.public_url);
        let author = CommitIdentity::noreply(template.actor.name().as_str(), template.actor.account.display_name.as_deref(), &domain);
        let timeout = config.repository.git_max_duration().min(TEMPLATE_TIMEOUT);
        let git = &config.repository.git_binary;
        copy_snapshot(git, &config.storage.tmp_dir(), (&source.disk_path, template.commit), target, &author, timeout)
            .await
            .map_err(ServiceError::internal)?;
        if template.lfs.enabled() {
            // The destination only has the new snapshot. Looking here never links an
            // object that appeared exclusively in a different source revision.
            let pointers = tokio::time::timeout(timeout, lfs_pointers(git, target.0)).await.map_err(ServiceError::internal)??;
            template.lfs.copy_template_objects(source, repo, pointers).await?;
        }
        self.service.store.mark_pushed(repo).await?;
        Ok(())
    }
}
pub(super) fn owner_error(error: RepositoryOwnerError<StorageError>) -> ServiceError {
    match error {
        RepositoryOwnerError::Port(error) => error.into(),
        RepositoryOwnerError::Invalid(message) => ServiceError::Invalid(message),
    }
}
impl RepositoryService {
    /// Personally owned creation retains the original owner-name input and fast path.
    pub async fn create(&self, actor: &User, input: CreateRepository) -> Result<OwnedRepository, ServiceError> {
        self.create_in(actor, actor.name().as_str(), input).await
    }
    /// Metadata, bare Git initialization, optional creator grant and publication retain their original order.
    pub async fn create_in(&self, actor: &User, owner: &str, input: CreateRepository) -> Result<OwnedRepository, ServiceError> {
        self.create_repository(actor, owner, input, None).await
    }

    /// Creates an independent repository from the default branch of a readable source.
    pub async fn create_from_template(
        &self,
        actor: &User,
        owner: &str,
        input: CreateRepository,
        source: &str,
        lfs: &LfsService,
    ) -> Result<OwnedRepository, ServiceError> {
        let Some((source_owner, source_name)) = source.trim().split_once('/') else {
            return Err(ServiceError::Invalid("Enter the template repository as owner/name".into()));
        };
        let (source_owner, source_name) = (source_owner.trim(), source_name.trim());
        if source_owner.is_empty() || source_name.is_empty() || source_name.contains('/') {
            return Err(ServiceError::Invalid("Enter the template repository as owner/name".into()));
        }
        let source = self.open_template(actor, (source_owner, source_name), input.visibility).await?;
        // Finish the database and Git work even if the submitting request disconnects.
        let service = self.clone();
        let actor = actor.clone();
        let owner = owner.to_owned();
        let lfs = lfs.clone();
        tokio::spawn(async move {
            let _source_lease = service.storage.lease(source.repo.repo.id).await;
            let commit = source.reader.branch_commit_id(source.repo.repo.default_branch.clone()).await?;
            let current =
                service.open_template(&actor, (source.repo.owner.name.as_str(), source.repo.repo.name.as_str()), input.visibility).await?;
            if current.repo.repo.id != source.repo.repo.id || current.repo.repo.default_branch != source.repo.repo.default_branch {
                return Err(ServiceError::Invalid("The template repository changed. Try again.".into()));
            }
            let commit = commit.ok_or_else(|| ServiceError::Invalid("The template repository's default branch has no commits".into()))?;
            let template = TemplateSource { repo: &current, commit: &commit, actor: &actor, lfs: &lfs };
            service.create_repository(&actor, &owner, input, Some(template)).await
        })
        .await
        .map_err(ServiceError::internal)?
    }

    async fn open_template(
        &self,
        actor: &User,
        (source_owner, source_name): (&str, &str),
        visibility: Visibility,
    ) -> Result<RepoContext, ServiceError> {
        let source = match self.open(Some(actor), source_owner, source_name).await {
            Ok(source) => source,
            Err(ServiceError::NotFound) => return Err(ServiceError::Invalid("The template repository could not be found".into())),
            Err(error) => return Err(error),
        };
        if !source.repo.repo.visibility.is_public() {
            if !source.access.can_admin() {
                return Err(ServiceError::Forbidden);
            }
            if visibility.is_public() {
                return Err(ServiceError::Invalid("A repository created from a private template must be private".into()));
            }
        }
        Ok(source)
    }

    async fn create_repository(
        &self,
        actor: &User,
        owner: &str,
        input: CreateRepository,
        template: Option<TemplateSource<'_>>,
    ) -> Result<OwnedRepository, ServiceError> {
        RepositoryCreation::new(&CreationEffects { service: self, template }).create_in(actor, owner, input).await.map_err(|error| {
            match error {
                RepositoryCreationError::Owner(error) => owner_error(error),
                RepositoryCreationError::Name(error) => error.into(),
                RepositoryCreationError::Invalid(message) => ServiceError::Invalid(message),
                RepositoryCreationError::Store(error) => error.into(),
                RepositoryCreationError::Disk(error) => error,
            }
        })
    }
}
