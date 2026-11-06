//! Static human-comment adapter; original SQL, transactions and row checks stay authoritative.

use crate::{StorageError, sqlite::issues::IssueStore};
use rbit_collaboration::{
    comments::{Comment, CommentId, HumanCommentRepository},
    threads::{IssueId, IssueText},
};
use rbit_core::{AccountId, Timestamp};

impl HumanCommentRepository for IssueStore {
    type Error = StorageError;
    async fn find(&self, issue: IssueId, id: CommentId) -> Result<Option<Comment>, Self::Error> {
        self.find_comment(issue, id).await
    }
    async fn add(&self, issue: IssueId, actor: AccountId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, Self::Error> {
        self.add_comment(issue, actor, body, at).await
    }
    async fn edit(&self, id: CommentId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, Self::Error> {
        self.edit_comment(id, body, at).await
    }
    async fn delete(&self, id: CommentId) -> Result<Option<Comment>, Self::Error> {
        self.delete_comment(id).await
    }
}
