//! Production timeline read/create entry points through shared thread policy.

use rbit_collaboration::threads::{ThreadAccess, ThreadApplication, ThreadDraft, ThreadError};
use rbit_storage::StorageError;

use super::{
    Issue, IssueContext, IssueKind, IssueNumber, OpenCounts, RepoContext, RepoId, ServiceError, TimelineService, Timestamp, User,
    thread_permissions,
};

fn access(repo: &RepoContext) -> ThreadAccess {
    ThreadAccess { repo: repo.repo.repo.id, can_read: repo.access.can_read(), archived: repo.repo.repo.is_archived() }
}

fn error<P: Into<ServiceError>>(error: ThreadError<StorageError, P>) -> ServiceError {
    match error {
        ThreadError::Store(error) => error.into(),
        ThreadError::Throttle(error) => error.into(),
        ThreadError::Text(error) => error.into(),
        ThreadError::NotFound => ServiceError::NotFound,
        ThreadError::Archived => ServiceError::Archived,
    }
}

impl TimelineService {
    /// A thread and author in one statement; permission shaping retains the legacy table.
    pub async fn open(&self, viewer: Option<&User>, repo: RepoContext, number: IssueNumber) -> Result<IssueContext, ServiceError> {
        let (issue, author) = ThreadApplication::new(&self.ctx.database.issues(), access(&repo)).open(number).await.map_err(error)?;
        let permissions = thread_permissions(viewer, &repo, &issue);
        Ok(IssueContext { repo, issue, author, permissions })
    }

    /// Open issues and pull requests of `repo`, in one indexed query.
    pub async fn open_counts(&self, repo: RepoId) -> Result<OpenCounts, ServiceError> {
        let scope = ThreadAccess { repo, can_read: true, archived: false };
        ThreadApplication::new(&self.ctx.database.issues(), scope).counts().await.map_err(error)
    }

    /// Validates, spends content throttle, then inserts the original row transaction.
    /// The caller attaches feature-specific records before publishing `Opened`.
    pub(crate) async fn create(
        &self,
        actor: &User,
        repo: &RepoContext,
        kind: IssueKind,
        title: &str,
        body: &str,
    ) -> Result<Issue, ServiceError> {
        let draft = ThreadDraft { author: actor.id(), kind, title, body };
        ThreadApplication::new(&self.ctx.database.issues(), access(repo))
            .create(draft, || self.spend(actor), Timestamp::now)
            .await
            .map_err(error)
    }
}
