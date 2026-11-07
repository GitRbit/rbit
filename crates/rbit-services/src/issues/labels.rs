//! Legacy public entry points delegate label policy to the collaboration application.

use rbit_collaboration::labels::{LabelAccess, LabelApplication, LabelError};
use rbit_core::{LabelId, Timestamp, User};
use rbit_storage::{StorageError, sqlite::issue_meta::IssueMetaStore};

use super::{IssueService, Label, LabelInput, LabelUsage};
use crate::{RepoContext, ServiceError};

fn access(repo: &RepoContext) -> LabelAccess {
    LabelAccess { repo: repo.repo.repo.id, can_write: repo.access.can_write(), archived: repo.repo.repo.is_archived() }
}

fn error<P: Into<ServiceError>>(error: LabelError<StorageError, P>) -> ServiceError {
    match error {
        LabelError::Store(error) => error.into(),
        LabelError::Throttle(error) => error.into(),
        LabelError::Invalid(message) => ServiceError::Invalid(message),
        LabelError::Forbidden => ServiceError::Forbidden,
        LabelError::Archived => ServiceError::Archived,
        LabelError::NotFound => ServiceError::NotFound,
    }
}

impl IssueService {
    fn label_store(&self) -> IssueMetaStore {
        self.ctx.database.issue_meta()
    }

    /// The labels of `repo`, by name.
    pub async fn labels(&self, repo: &RepoContext) -> Result<Vec<Label>, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo)).list().await.map_err(error)
    }

    /// The labels of `repo` with their open issues.
    pub async fn label_usage(&self, repo: &RepoContext) -> Result<Vec<LabelUsage>, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo)).usage().await.map_err(error)
    }

    /// The label of `repo` called `name` (case-insensitively).
    pub async fn label_named(&self, repo: &RepoContext, name: &str) -> Result<Label, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo)).named(name).await.map_err(error)
    }

    /// The labels of `repo` among `names`; unknown names are invalid.
    pub async fn labels_named(&self, repo: &RepoContext, names: &[String]) -> Result<Vec<Label>, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo)).named_many(names).await.map_err(error)
    }

    /// Creates a label (writers; at most 200 per repository; names unique ignoring case).
    pub async fn create_label(&self, actor: &User, repo: &RepoContext, input: &LabelInput) -> Result<Label, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo)).create(input, || self.spend(actor), Timestamp::now).await.map_err(error)
    }

    /// Changes label `id` of `repo` (writers).
    pub async fn update_label(&self, actor: &User, repo: &RepoContext, id: LabelId, input: &LabelInput) -> Result<Label, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo))
            .update(id, input, || self.spend(actor), Timestamp::now)
            .await
            .map_err(error)
    }

    /// Deletes a label, removing thread associations without timeline events or throttle.
    pub async fn delete_label(&self, repo: &RepoContext, id: LabelId) -> Result<Label, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo)).delete(id).await.map_err(error)
    }

    /// Adds the default labels only to an empty catalog; returns the inserted count.
    pub async fn create_default_labels(&self, actor: &User, repo: &RepoContext) -> Result<u64, ServiceError> {
        LabelApplication::new(|| self.label_store(), access(repo)).seed_defaults(|| self.spend(actor), Timestamp::now).await.map_err(error)
    }
}
