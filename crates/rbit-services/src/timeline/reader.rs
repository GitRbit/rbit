//! Narrow borrowed read composition; runtime concurrency remains in the storage adapter.
use super::{
    Account, Comment, CommentId, IssueContext, IssueId, ServiceError, Timeline, TimelineCursor, TimelineEvent, User, comment_permissions,
    thread_permissions,
};
use rbit_collaboration::timeline::{CommentAccess, IssueSource, TimelinePage, TimelineReadPort, TimelineReader};
use rbit_core::{AccountId, IssuePermissions};
use rbit_storage::{Database, StorageError, sqlite::issues::IssueStore};
#[derive(Debug, Clone, Copy)]
pub struct ThreadReader<'a> {
    pub(super) database: &'a Database,
}
struct ReadView<'a> {
    database: &'a Database,
    store: IssueStore,
    viewer: Option<&'a User>,
    context: &'a IssueContext,
}
impl TimelineReadPort for ReadView<'_> {
    type Error = StorageError;
    type Event = TimelineEvent;
    type Permissions = IssuePermissions;
    async fn page(&self, issue: IssueId, cursor: Option<TimelineCursor>, limit: u32) -> Result<TimelinePage<Self::Event>, Self::Error> {
        self.store.timeline(issue, cursor, limit).await
    }
    async fn subjects(&self, accounts: &[AccountId], sources: &[IssueId]) -> Result<(Vec<Account>, Vec<IssueSource>), Self::Error> {
        let account_store = self.database.accounts();
        tokio::try_join!(
            account_store.find_accounts(accounts),
            self.store.readable_sources(sources, self.viewer.map(User::id), self.viewer.is_some_and(|viewer| viewer.is_admin))
        )
    }
    async fn comment(&self, issue: IssueId, id: CommentId) -> Result<Option<Comment>, Self::Error> {
        self.store.find_comment(issue, id).await
    }
    async fn accounts(&self, ids: &[AccountId]) -> Result<Vec<Account>, Self::Error> {
        self.database.accounts().find_accounts(ids).await
    }
    fn subject_account(event: &TimelineEvent) -> Option<AccountId> {
        event.subject_account()
    }
    fn cross_reference(event: &TimelineEvent) -> Option<IssueId> {
        if let TimelineEvent::CrossReferenced { source } = event { Some(*source) } else { None }
    }
    fn permissions(&self) -> IssuePermissions {
        thread_permissions(self.viewer, &self.context.repo, &self.context.issue)
    }
    fn comment_access(&self, permissions: IssuePermissions, comment: &Comment) -> CommentAccess {
        let access = comment_permissions(self.viewer, permissions, comment);
        CommentAccess { can_edit: access.can_edit, can_delete: access.can_delete }
    }
}
impl ThreadReader<'_> {
    pub async fn find_comment(&self, ctx: &IssueContext, id: CommentId) -> Result<(Comment, Option<Account>), ServiceError> {
        let view = ReadView { database: self.database, store: self.database.issues(), viewer: None, context: ctx };
        TimelineReader::new(&view).find_comment(ctx.issue.id, id).await?.ok_or(ServiceError::NotFound)
    }
    pub async fn timeline(
        &self,
        viewer: Option<&User>,
        ctx: &IssueContext,
        cursor: Option<TimelineCursor>,
        limit: u32,
    ) -> Result<Timeline, ServiceError> {
        let view = ReadView { database: self.database, store: self.database.issues(), viewer, context: ctx };
        Ok(TimelineReader::new(&view).read(ctx.issue.id, cursor, limit).await?)
    }
}
