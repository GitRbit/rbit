//! Request effects of release commands: tags reported like pushes, events and tracing.

use rbit_core::{AccountId, release::ReleaseId};
use rbit_git::GitError;
use rbit_repositories::releases::{ReleasePort, TagError};
use rbit_storage::sqlite::releases::ReleaseStore;

use super::{ReleaseActor, ReleaseService};
use crate::{DomainEvent, RepoContext, ServiceError};

/// One create or edit command on an opened repository.
pub(super) struct ReleaseEffects<'a> {
    pub service: &'a ReleaseService,
    pub repo: &'a RepoContext,
    pub actor: ReleaseActor<'a>,
    pub store: ReleaseStore,
}

impl ReleasePort for ReleaseEffects<'_> {
    type Store = ReleaseStore;
    type Error = ServiceError;

    fn store(&self) -> &ReleaseStore {
        &self.store
    }

    async fn tag_exists(&self, tag: &str) -> Result<bool, ServiceError> {
        Ok(self.repo.reader.ref_snapshot().await?.get(&format!("refs/tags/{tag}")).is_some())
    }

    /// Reported like a push of the tag ([`DomainEvent::RepositoryPushed`]) so caches and
    /// webhooks see it; a task's push starts no runs.
    async fn create_tag(&self, tag: &str, target: &str) -> Result<(), TagError<ServiceError>> {
        let pending = self.service.inner.pushes.begin(self.repo, self.actor.push_source()).await;
        match self.repo.reader.create_tag(tag.to_owned(), target.to_owned()).await {
            Ok(_) => {
                self.service.inner.pushes.complete(pending).await;
                tracing::info!(repo = %self.repo.repo.full_name(), tag, target, "tag created for a release");
                Ok(())
            }
            Err(GitError::InvalidBranchName(_)) => Err(TagError::InvalidName),
            Err(GitError::AlreadyExists) => Err(TagError::Exists),
            Err(GitError::NotFound(_)) => Err(TagError::MissingTarget),
            Err(other) => Err(TagError::Failure(other.into())),
        }
    }

    fn published(&self, release: ReleaseId, actor: Option<AccountId>) {
        self.service.inner.ctx.events.publish(DomainEvent::ReleasePublished { repo: self.repo.repo.repo.id, release, actor });
    }

    fn created(&self, tag: &str, draft: bool) {
        tracing::info!(repo = %self.repo.repo.full_name(), tag, draft, "release created");
    }
}
