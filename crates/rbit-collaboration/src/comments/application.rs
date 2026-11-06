//! Human comment policy with exact lookup, authorization and unchanged-body fast paths.

use super::{Comment, CommentId, HumanCommentRepository};
use crate::threads::{IssueId, IssueText, IssueTextError};
use rbit_contracts::{AccountId, Timestamp};

#[derive(Debug)]
pub enum HumanCommentError<E, T = E> {
    Store(E),
    Throttle(T),
    Text(IssueTextError),
    Forbidden,
    Archived,
    NotFound,
}
#[derive(Debug)]
pub struct CommentEditResult {
    pub comment: Comment,
    /// Moved from the previous row only on actual edits; no string copy is added.
    pub previous_body: Option<IssueText>,
}
#[derive(Debug, Clone, Copy)]
pub struct HumanComments {
    issue: IssueId,
    actor: AccountId,
    archived: bool,
}
impl HumanComments {
    pub fn new(issue: IssueId, actor: AccountId, archived: bool) -> Self {
        Self { issue, actor, archived }
    }
    fn require<E, T>(&self, allowed: bool) -> Result<(), HumanCommentError<E, T>> {
        if !allowed {
            return Err(HumanCommentError::Forbidden);
        }
        if self.archived {
            return Err(HumanCommentError::Archived);
        }
        Ok(())
    }
    pub async fn create<R: HumanCommentRepository, T>(
        &self,
        body: &str,
        allowed: bool,
        factory: impl FnOnce() -> R,
        spend: impl FnOnce() -> Result<(), T>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Comment, HumanCommentError<R::Error, T>> {
        self.require(allowed)?;
        let body = IssueText::comment(body).map_err(HumanCommentError::Text)?;
        spend().map_err(HumanCommentError::Throttle)?;
        let repository = factory();
        repository.add(self.issue, self.actor, &body, now()).await.map_err(HumanCommentError::Store)?.ok_or(HumanCommentError::NotFound)
    }
    pub async fn edit<R: HumanCommentRepository, T>(
        &self,
        id: CommentId,
        body: &str,
        factory: impl FnOnce() -> R,
        allowed: impl FnOnce(&Comment) -> bool,
        spend: impl FnOnce() -> Result<(), T>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<CommentEditResult, HumanCommentError<R::Error, T>> {
        let repository = factory();
        let comment = repository.find(self.issue, id).await.map_err(HumanCommentError::Store)?.ok_or(HumanCommentError::NotFound)?;
        self.require(allowed(&comment))?;
        let body = IssueText::comment(body).map_err(HumanCommentError::Text)?;
        if body == comment.body {
            return Ok(CommentEditResult { comment, previous_body: None });
        }
        spend().map_err(HumanCommentError::Throttle)?;
        let edited = repository.edit(id, &body, now()).await.map_err(HumanCommentError::Store)?.ok_or(HumanCommentError::NotFound)?;
        Ok(CommentEditResult { comment: edited, previous_body: Some(comment.body) })
    }
    pub async fn delete<R: HumanCommentRepository>(
        &self,
        id: CommentId,
        factory: impl FnOnce() -> R,
        allowed: impl FnOnce(&Comment) -> bool,
    ) -> Result<Option<Comment>, HumanCommentError<R::Error>> {
        let repository = factory();
        let comment = repository.find(self.issue, id).await.map_err(HumanCommentError::Store)?.ok_or(HumanCommentError::NotFound)?;
        self.require(allowed(&comment))?;
        // A concurrent deletion may win after lookup. Preserve its empty-success result.
        repository.delete(id).await.map_err(HumanCommentError::Store)
    }
}
