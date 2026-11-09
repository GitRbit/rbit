//! What the issue event subscriber does with thread activity: which texts are indexed
//! for dashboard mentions and which produce cross-references. The subscriber only
//! routes events here.

use std::future::Future;

use rbit_contracts::AccountId;

use crate::{
    comments::CommentId,
    threads::{Issue, IssueId},
};

/// A review as the subscriber sees it: whether it is submitted, and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewText {
    pub submitted: bool,
    pub body: String,
}

/// Reads and effects of the subscriber. `Review` is the review id type of the caller.
pub trait ThreadActivityPort: Sync {
    type Error;
    type Review: Copy + Send + Sync;

    fn thread(&self, issue: IssueId) -> impl Future<Output = Result<Option<Issue>, Self::Error>> + Send;
    fn comment_body(&self, issue: IssueId, comment: CommentId) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    fn review(&self, pull: IssueId, review: Self::Review) -> impl Future<Output = Result<Option<ReviewText>, Self::Error>> + Send;
    /// The bodies of `review`'s line comments.
    fn review_comments(&self, pull: IssueId, review: Self::Review) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;
    /// Indexes the people `text` mentions on `issue` (`DashboardMentions::record`).
    fn record_mentions(&self, issue: IssueId, text: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Records `#N`/`owner/repo#N` references in `text` written on `source`.
    fn record_cross_references(
        &self,
        source: &Issue,
        actor: Option<AccountId>,
        text: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Thread activity decisions over a port.
#[derive(Debug)]
pub struct ThreadActivity<'a, P> {
    port: &'a P,
}

impl<'a, P: ThreadActivityPort> ThreadActivity<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }

    /// Text that is no longer stored: the previous body of an edit or a deleted
    /// comment. Its mentions stay indexed, since events can be handled after a later
    /// edit or deletion.
    pub async fn earlier_text(&self, issue: IssueId, text: &str) -> Result<(), P::Error> {
        self.port.record_mentions(issue, text).await
    }

    /// A thread opened or edited: its current body's mentions and references. A thread
    /// deleted since is skipped.
    pub async fn thread_written(&self, issue: IssueId, actor: Option<AccountId>) -> Result<(), P::Error> {
        let Some(source) = self.port.thread(issue).await? else { return Ok(()) };
        let body = source.body.to_string();
        self.port.record_mentions(issue, &body).await?;
        self.port.record_cross_references(&source, actor, &body).await
    }

    /// A comment created or edited: its current body's mentions, and references unless
    /// rbit Actions wrote it (`actor` is `None`): a cross-reference would name a ghost.
    pub async fn comment_written(&self, issue: IssueId, comment: CommentId, actor: Option<AccountId>) -> Result<(), P::Error> {
        let (Some(source), Some(body)) = (self.port.thread(issue).await?, self.port.comment_body(issue, comment).await?) else {
            return Ok(());
        };
        self.port.record_mentions(issue, &body).await?;
        match actor {
            Some(_) => self.port.record_cross_references(&source, actor, &body).await,
            None => Ok(()),
        }
    }

    /// A review event: once submitted (pending reviews stay private), the previous body
    /// of an edit, the body and its line comments are indexed for mentions.
    pub async fn review_changed(&self, pull: IssueId, review: P::Review, previous_body: Option<&str>) -> Result<(), P::Error> {
        let Some(text) = self.port.review(pull, review).await? else { return Ok(()) };
        if !text.submitted {
            return Ok(());
        }
        if let Some(previous) = previous_body {
            self.port.record_mentions(pull, previous).await?;
        }
        self.port.record_mentions(pull, &text.body).await?;
        for comment in self.port.review_comments(pull, review).await? {
            self.port.record_mentions(pull, &comment).await?;
        }
        Ok(())
    }
}
