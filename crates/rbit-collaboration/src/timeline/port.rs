//! Static selected-page reads. Implementations retain their existing concurrent subject queries.
use super::{IssueSource, TimelineCursor, TimelinePage};
use crate::{
    comments::{Comment, CommentId},
    threads::IssueId,
};
use rbit_contracts::AccountId;
use rbit_identity_models::Account;
use std::future::Future;
#[derive(Debug, Clone, Copy)]
pub struct CommentAccess {
    pub can_edit: bool,
    pub can_delete: bool,
}
pub trait TimelineReadPort: Sync {
    type Error;
    type Event: Send;
    type Permissions: Copy;
    fn page(
        &self,
        issue: IssueId,
        cursor: Option<TimelineCursor>,
        limit: u32,
    ) -> impl Future<Output = Result<TimelinePage<Self::Event>, Self::Error>> + Send;
    fn subjects(
        &self,
        accounts: &[AccountId],
        sources: &[IssueId],
    ) -> impl Future<Output = Result<(Vec<Account>, Vec<IssueSource>), Self::Error>> + Send;
    fn comment(&self, issue: IssueId, id: CommentId) -> impl Future<Output = Result<Option<Comment>, Self::Error>> + Send;
    fn accounts(&self, ids: &[AccountId]) -> impl Future<Output = Result<Vec<Account>, Self::Error>> + Send;
    fn subject_account(event: &Self::Event) -> Option<AccountId>;
    fn cross_reference(event: &Self::Event) -> Option<IssueId>;
    fn permissions(&self) -> Self::Permissions;
    fn comment_access(&self, permissions: Self::Permissions, comment: &Comment) -> CommentAccess;
}
