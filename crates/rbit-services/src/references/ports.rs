//! Concrete reference adapters preserve batched SQL and the shared grant calculation.
use crate::{ServiceError, ThreadServices, repositories::viewer_grants};
use rbit_collaboration::{
    references::{CrossReferencePort, ReferenceReadPort, RemoteRef, RemoteReferencePort},
    threads::{Issue, IssueId, IssueNumber},
    timeline::IssueRef,
};
use rbit_core::access::repo_access;
use rbit_core::{Account, AccountId, RepoId, TimelineEvent, Timestamp, User, repository::OwnedRepository};
use rbit_storage::{Database, RepositoryStore};
#[derive(Debug, Clone, Copy)]
pub(super) struct ReferenceView<'a> {
    pub database: &'a Database,
    pub timeline: &'a ThreadServices,
}
pub(super) struct RemoteView {
    store: RepositoryStore,
}
impl ReferenceReadPort for ReferenceView<'_> {
    type Error = ServiceError;
    async fn resolve_batch(
        &self,
        repo: RepoId,
        numbers: &[IssueNumber],
        names: &[String],
    ) -> Result<(Vec<IssueRef>, Vec<Account>), Self::Error> {
        let issue_store = self.database.issues();
        let meta_store = self.database.issue_meta();
        Ok(tokio::try_join!(issue_store.find_numbers(repo, numbers), meta_store.accounts_named(names))?)
    }
}
impl RemoteReferencePort for RemoteView {
    type Error = ServiceError;
    async fn find(&self, remote: &RemoteRef) -> Result<Option<OwnedRepository>, Self::Error> {
        Ok(self.store.find_by_path(&remote.owner, &remote.repo).await?)
    }
    async fn can_read(&self, user: &User, repo: &OwnedRepository) -> Result<bool, Self::Error> {
        let grants = viewer_grants(&self.store, Some(user), repo).await?;
        Ok(repo_access(Some(user), repo, &grants).can_read())
    }
}
impl CrossReferencePort for ReferenceView<'_> {
    type Error = ServiceError;
    type Remote = RemoteView;
    async fn numbers(&self, repo: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        Ok(self.database.issues().find_numbers(repo, numbers).await?)
    }
    async fn issue(&self, id: IssueId) -> Result<Option<Issue>, Self::Error> {
        Ok(self.database.issues().find(id).await?)
    }
    async fn active_user(&self, id: AccountId) -> Result<Option<User>, Self::Error> {
        Ok(self.database.accounts().find_active_user(id).await?)
    }
    fn repositories(&self) -> RemoteView {
        RemoteView { store: self.database.repositories() }
    }
    async fn by_number(&self, repo: RepoId, number: IssueNumber) -> Result<Option<Issue>, Self::Error> {
        Ok(self.database.issues().find_by_number(repo, number).await?)
    }
    async fn link(&self, target: IssueId, source: IssueId, at: Timestamp) -> Result<bool, Self::Error> {
        Ok(self.database.issue_meta().record_cross_ref(target, source, at).await?)
    }
    async fn record(&self, target: IssueId, source: IssueId, actor: Option<AccountId>) -> Result<(), Self::Error> {
        self.timeline.writer().record(target, actor, TimelineEvent::CrossReferenced { source }).await?;
        Ok(())
    }
}

impl rbit_collaboration::references::CommitReferencePort for ReferenceView<'_> {
    type Error = ServiceError;
    async fn targets(&self, repo: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        Ok(self.database.issues().find_numbers(repo, numbers).await?)
    }
    async fn commit_link(&self, target: IssueId, commit: &str, at: Timestamp) -> Result<bool, Self::Error> {
        Ok(self.database.issue_meta().record_commit_ref(target, commit, at).await?)
    }
    async fn record_commit(&self, target: IssueId, commit: &str, summary: &str, actor: Option<AccountId>) -> Result<(), Self::Error> {
        self.timeline
            .writer()
            .record(target, actor, TimelineEvent::CommitReferenced { commit: commit.to_owned(), summary: summary.to_owned() })
            .await?;
        Ok(())
    }
    async fn close_commit(&self, target: IssueId, commit: &str, actor: Option<AccountId>) -> Result<(), Self::Error> {
        let event = TimelineEvent::Closed { reason: rbit_core::StateReason::Completed, commit: Some(commit.to_owned()), pull: None };
        self.timeline.writer().close_as_system(target, actor, rbit_core::StateReason::Completed, event).await?;
        Ok(())
    }
}
impl rbit_collaboration::references::MergeReferencePort for ReferenceView<'_> {
    type Error = ServiceError;
    async fn merged_into_default(&self, pull: IssueId) -> Result<bool, Self::Error> {
        crate::pulls::lifecycle::merged_into_default_branch_with(self.database, pull).await
    }
    async fn pull_issue(&self, pull: IssueId) -> Result<Option<Issue>, Self::Error> {
        Ok(self.database.issues().find(pull).await?)
    }
    async fn closing_targets(&self, repo: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        Ok(self.database.issues().find_numbers(repo, numbers).await?)
    }
    async fn close_merge(&self, target: IssueId, pull: IssueNumber, actor: Option<AccountId>) -> Result<(), Self::Error> {
        let event = TimelineEvent::Closed { reason: rbit_core::StateReason::Completed, commit: None, pull: Some(pull) };
        self.timeline.writer().close_as_system(target, actor, rbit_core::StateReason::Completed, event).await?;
        Ok(())
    }
}
