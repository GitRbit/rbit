//! Human lifecycle adapters retain legacy permission tables and transient event publication.

use super::{Comment, CommentAction, CommentId, IssueContext, ServiceError, Timestamp, User, comment_permissions, permissions};
use rbit_collaboration::comments::{HumanCommentError, HumanComments};
use rbit_storage::StorageError;

fn error<T: Into<ServiceError>>(error: HumanCommentError<StorageError, T>) -> ServiceError {
    match error {
        HumanCommentError::Store(error) => error.into(),
        HumanCommentError::Throttle(error) => error.into(),
        HumanCommentError::Text(error) => error.into(),
        HumanCommentError::Forbidden => ServiceError::Forbidden,
        HumanCommentError::Archived => ServiceError::Archived,
        HumanCommentError::NotFound => ServiceError::NotFound,
    }
}
fn application(actor: &User, ctx: &IssueContext) -> HumanComments {
    HumanComments::new(ctx.issue.id, actor.id(), ctx.repo.repo.repo.is_archived())
}
impl crate::timeline::writer::ThreadWriter<'_> {
    /// Adds a human comment after the original permission, validation and throttle gates.
    pub async fn comment(&self, actor: &User, ctx: &IssueContext, body: &str) -> Result<Comment, ServiceError> {
        let comment = application(actor, ctx)
            .create(body, permissions(actor, ctx).can_comment, || self.database.issues(), || self.spend(actor), Timestamp::now)
            .await
            .map_err(error)?;
        self.publish_comment(ctx, comment.id, Some(actor), CommentAction::Created);
        Ok(comment)
    }
    /// Replaces the actor's comment text; unchanged normalized bodies avoid throttle/write.
    pub async fn edit_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId, body: &str) -> Result<Comment, ServiceError> {
        let result = application(actor, ctx)
            .edit(
                id,
                body,
                || self.database.issues(),
                |comment| comment_permissions(Some(actor), permissions(actor, ctx), comment).can_edit,
                || self.spend(actor),
                Timestamp::now,
            )
            .await
            .map_err(error)?;
        if let Some(previous_body) = result.previous_body {
            self.publish_comment(ctx, id, Some(actor), CommentAction::Edited { previous_body: previous_body.into() });
        }
        Ok(result.comment)
    }
    /// Deletes an authorized comment, retaining success if another deletion wins the race.
    pub async fn delete_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId) -> Result<(), ServiceError> {
        if let Some(snapshot) = application(actor, ctx)
            .delete(id, || self.database.issues(), |comment| comment_permissions(Some(actor), permissions(actor, ctx), comment).can_delete)
            .await
            .map_err(error)?
        {
            self.publish_comment(ctx, id, Some(actor), CommentAction::Deleted { snapshot });
        }
        Ok(())
    }
}
