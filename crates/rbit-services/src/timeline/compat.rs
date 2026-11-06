//! Transitional Rust entry points; feature callers can select explicit borrowed reader/writer roles.
use super::{
    Account, Comment, CommentId, Issue, IssueContext, IssueNumber, OpenCounts, RepoContext, RepoId, ServiceError, StateChange, TaskGrant,
    ThreadServices, Timeline, TimelineCursor, User,
};
impl ThreadServices {
    pub async fn open(&self, viewer: Option<&User>, repo: RepoContext, number: IssueNumber) -> Result<IssueContext, ServiceError> {
        self.reader().open(viewer, repo, number).await
    }
    pub async fn open_counts(&self, repo: RepoId) -> Result<OpenCounts, ServiceError> {
        self.reader().open_counts(repo).await
    }
    pub async fn find_comment(&self, ctx: &IssueContext, id: CommentId) -> Result<(Comment, Option<Account>), ServiceError> {
        self.reader().find_comment(ctx, id).await
    }
    pub async fn timeline(
        &self,
        viewer: Option<&User>,
        ctx: &IssueContext,
        cursor: Option<TimelineCursor>,
        limit: u32,
    ) -> Result<Timeline, ServiceError> {
        self.reader().timeline(viewer, ctx, cursor, limit).await
    }
    pub async fn edit(&self, actor: &User, ctx: &IssueContext, title: Option<&str>, body: Option<&str>) -> Result<Issue, ServiceError> {
        self.writer().edit(actor, ctx, title, body).await
    }
    pub async fn set_state(&self, actor: &User, ctx: &IssueContext, change: StateChange) -> Result<Issue, ServiceError> {
        self.writer().set_state(actor, ctx, change).await
    }
    pub async fn set_locked(&self, actor: &User, ctx: &IssueContext, locked: bool) -> Result<(), ServiceError> {
        self.writer().set_locked(actor, ctx, locked).await
    }
    pub async fn comment(&self, actor: &User, ctx: &IssueContext, body: &str) -> Result<Comment, ServiceError> {
        self.writer().comment(actor, ctx, body).await
    }
    pub async fn edit_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId, body: &str) -> Result<Comment, ServiceError> {
        self.writer().edit_comment(actor, ctx, id, body).await
    }
    pub async fn delete_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId) -> Result<(), ServiceError> {
        self.writer().delete_comment(actor, ctx, id).await
    }
    pub async fn comment_for_task(&self, grant: &TaskGrant, ctx: &IssueContext, body: &str) -> Result<Comment, ServiceError> {
        self.writer().comment_for_task(grant, ctx, body).await
    }
    pub async fn edit_comment_for_task(
        &self,
        grant: &TaskGrant,
        ctx: &IssueContext,
        id: CommentId,
        body: &str,
    ) -> Result<Comment, ServiceError> {
        self.writer().edit_comment_for_task(grant, ctx, id, body).await
    }
    pub async fn delete_comment_for_task(&self, grant: &TaskGrant, ctx: &IssueContext, id: CommentId) -> Result<(), ServiceError> {
        self.writer().delete_comment_for_task(grant, ctx, id).await
    }
}
