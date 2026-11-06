//! Task comments retain repository/kind authority and source provenance, without per-run ownership.

use super::{Comment, CommentEditResult, CommentId, CommentSource, HumanCommentRepository};
use crate::threads::{IssueId, IssueText, IssueTextError};
use rbit_contracts::Timestamp;
use std::future::Future;

pub trait TaskCommentRepository: HumanCommentRepository {
    fn add_actions(
        &self,
        issue: IssueId,
        body: &IssueText,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<Comment>, Self::Error>> + Send;
}
#[derive(Debug)]
pub enum TaskCommentError<E> {
    Store(E),
    Text(IssueTextError),
    Forbidden,
    Archived,
    NotFound,
}
#[derive(Debug, Clone, Copy)]
pub struct TaskComments {
    issue: IssueId,
    authorized: bool,
    archived: bool,
}
impl TaskComments {
    /// `authorized` is the caller's unchanged same-repository and kind-specific grant check.
    pub fn new(issue: IssueId, authorized: bool, archived: bool) -> Self {
        Self { issue, authorized, archived }
    }
    fn authority<E>(&self) -> Result<(), TaskCommentError<E>> {
        if self.authorized { Ok(()) } else { Err(TaskCommentError::Forbidden) }
    }
    fn writable<E>(&self) -> Result<(), TaskCommentError<E>> {
        if self.archived { Err(TaskCommentError::Archived) } else { Ok(()) }
    }
    async fn owned<R: TaskCommentRepository>(
        &self,
        id: CommentId,
        factory: impl FnOnce() -> R,
    ) -> Result<(R, Comment), TaskCommentError<R::Error>> {
        self.authority()?;
        let repository = factory();
        let comment = repository.find(self.issue, id).await.map_err(TaskCommentError::Store)?.ok_or(TaskCommentError::NotFound)?;
        if comment.source != CommentSource::Actions {
            return Err(TaskCommentError::Forbidden);
        }
        self.writable()?;
        Ok((repository, comment))
    }
    pub async fn create<R: TaskCommentRepository>(
        &self,
        body: &str,
        factory: impl FnOnce() -> R,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Comment, TaskCommentError<R::Error>> {
        self.authority()?;
        self.writable()?;
        let body = IssueText::comment(body).map_err(TaskCommentError::Text)?;
        let repository = factory();
        repository.add_actions(self.issue, &body, now()).await.map_err(TaskCommentError::Store)?.ok_or(TaskCommentError::NotFound)
    }
    pub async fn edit<R: TaskCommentRepository>(
        &self,
        id: CommentId,
        body: &str,
        factory: impl FnOnce() -> R,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<CommentEditResult, TaskCommentError<R::Error>> {
        // A store owns only a shared pool handle: reuse it across separate operations,
        // without acquiring a connection or extending a database transaction.
        let (repository, comment) = self.owned(id, factory).await?;
        let body = IssueText::comment(body).map_err(TaskCommentError::Text)?;
        if body == comment.body {
            return Ok(CommentEditResult { comment, previous_body: None });
        }
        let edited = repository.edit(id, &body, now()).await.map_err(TaskCommentError::Store)?.ok_or(TaskCommentError::NotFound)?;
        Ok(CommentEditResult { comment: edited, previous_body: Some(comment.body) })
    }
    pub async fn delete<R: TaskCommentRepository>(
        &self,
        id: CommentId,
        factory: impl FnOnce() -> R,
    ) -> Result<Option<Comment>, TaskCommentError<R::Error>> {
        let (repository, _) = self.owned(id, factory).await?;
        repository.delete(id).await.map_err(TaskCommentError::Store)
    }
}
