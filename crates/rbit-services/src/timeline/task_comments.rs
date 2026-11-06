//! Task grant composition and legacy post-commit logs/events stay in the service adapter.

use super::{Comment, CommentAction, CommentId, IssueContext, ServiceError, TaskGrant, Timestamp};
use rbit_collaboration::comments::{TaskCommentError, TaskComments};
use rbit_storage::StorageError;
fn application(grant: &TaskGrant, ctx: &IssueContext) -> TaskComments {
    TaskComments::new(
        ctx.issue.id,
        ctx.repo.repo.repo.id == grant.repo && grant.can_comment_on(ctx.issue.kind),
        ctx.repo.repo.repo.is_archived(),
    )
}
fn error(error: TaskCommentError<StorageError>) -> ServiceError {
    match error {
        TaskCommentError::Store(error) => error.into(),
        TaskCommentError::Text(error) => error.into(),
        TaskCommentError::Forbidden => ServiceError::Forbidden,
        TaskCommentError::Archived => ServiceError::Archived,
        TaskCommentError::NotFound => ServiceError::NotFound,
    }
}
impl crate::timeline::writer::ThreadWriter<'_> {
    /// Adds an Actions comment using the task's repository/kind authority and API budget.
    pub async fn comment_for_task(&self, grant: &TaskGrant, ctx: &IssueContext, body: &str) -> Result<Comment, ServiceError> {
        let comment = application(grant, ctx).create(body, || self.database.issues(), Timestamp::now).await.map_err(error)?;
        tracing::info!(repo = grant.repo.0, issue = ctx.issue.number.get(), task = grant.task.0, "comment written by a workflow task");
        self.publish_comment(ctx, comment.id, None, CommentAction::Created);
        Ok(comment)
    }
    /// Edits any Actions-authored comment on the authorized thread, regardless of originating run.
    pub async fn edit_comment_for_task(
        &self,
        grant: &TaskGrant,
        ctx: &IssueContext,
        id: CommentId,
        body: &str,
    ) -> Result<Comment, ServiceError> {
        let result = application(grant, ctx).edit(id, body, || self.database.issues(), Timestamp::now).await.map_err(error)?;
        if let Some(previous_body) = result.previous_body {
            tracing::info!(repo = grant.repo.0, issue = ctx.issue.number.get(), task = grant.task.0, "comment edited by a workflow task");
            self.publish_comment(ctx, id, None, CommentAction::Edited { previous_body: previous_body.into() });
        }
        Ok(result.comment)
    }
    /// Deletes an Actions-authored comment with the existing source and scoped lookup checks.
    pub async fn delete_comment_for_task(&self, grant: &TaskGrant, ctx: &IssueContext, id: CommentId) -> Result<(), ServiceError> {
        if let Some(snapshot) = application(grant, ctx).delete(id, || self.database.issues()).await.map_err(error)? {
            tracing::info!(repo = grant.repo.0, issue = ctx.issue.number.get(), task = grant.task.0, "comment deleted by a workflow task");
            self.publish_comment(ctx, id, None, CommentAction::Deleted { snapshot });
        }
        Ok(())
    }
}
