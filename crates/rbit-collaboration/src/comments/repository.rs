//! Concrete comment persistence keeps the original SQL and per-operation store handle.

use super::{Comment, CommentId};
use crate::threads::{IssueId, IssueText};
use rbit_contracts::{AccountId, Timestamp};
use std::future::Future;

pub trait HumanCommentRepository: Sync {
    type Error;
    fn find(&self, issue: IssueId, id: CommentId) -> impl Future<Output = Result<Option<Comment>, Self::Error>> + Send;
    fn add(
        &self,
        issue: IssueId,
        actor: AccountId,
        body: &IssueText,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<Comment>, Self::Error>> + Send;
    fn edit(&self, id: CommentId, body: &IssueText, at: Timestamp) -> impl Future<Output = Result<Option<Comment>, Self::Error>> + Send;
    fn delete(&self, id: CommentId) -> impl Future<Output = Result<Option<Comment>, Self::Error>> + Send;
}
