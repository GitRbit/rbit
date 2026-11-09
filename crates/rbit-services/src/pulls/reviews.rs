//! Reviews: comments on diff lines, pending reviews, submitting, dismissing and review
//! requests.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Rules: anyone who may comment on the thread (`IssuePermissions::can_comment`) may
//! review; the pull request's author may only comment (never approve or request changes).
//! Pending reviews and their comments are visible to their author only. Writers dismiss
//! approvals and change requests with a message. Writers and the author request reviewers
//! among users who can read the repository (at most
//! [`MAX_REQUESTED_REVIEWERS`](rbit_core::pull::MAX_REQUESTED_REVIEWERS)); submitting a
//! review clears its author's request. Line comments are anchored to `(head commit, path,
//! side, line)` of the diff from the merge base to the head and keep the four diff lines
//! ending at the commented one.

use std::collections::HashMap;

use rbit_collaboration::pulls::{
    AnchorLine, LineMark, NOT_DISMISSIBLE, ReviewRefusal, ReviewRequestPlan, ReviewRequests, dismissal_message, empty_review_refusal,
    may_dismiss_review, may_edit_review, may_request_reviews, review_threads, verdict_refusal,
};
use rbit_core::{
    Account, AccountId, IssueText, ReviewId, TimelineEvent, Timestamp, User,
    issue::{IssueAction, ReviewAction, ReviewState, issue_permissions},
    pull::{Review, ReviewComment, ReviewCommentId, ReviewSide},
};
use rbit_git::{
    DiffLimits, FileDiffBody,
    diff::{Hunk, LineKind},
};
use rbit_storage::sqlite::reviews::{NewReview, NewReviewComment};

use super::{
    PullContext, PullService,
    review_requests::{RequestView, request_error},
};
use crate::{DomainEvent, ServiceError};

/// A comment on one line of the Files changed tab, as submitted.
#[derive(Debug, Clone)]
pub struct LineComment {
    pub path: String,
    pub side: ReviewSide,
    pub line: u32,
    pub body: String,
    /// Answer an existing comment (its anchor is reused; `path`/`side`/`line` are ignored).
    pub in_reply_to: Option<ReviewCommentId>,
    /// Keep the comment pending in the viewer's review instead of posting it at once.
    pub start_review: bool,
}

pub use rbit_collaboration::pulls::ReviewVerdict;

/// A review with the accounts and comments it shows.
#[derive(Debug, Clone)]
pub struct ReviewEntry {
    pub review: Review,
    pub reviewer: Option<Account>,
    pub dismissed_by: Option<Account>,
    pub comments: Vec<(ReviewComment, Option<Account>)>,
}

/// Everything about reviews one viewer sees on a pull request.
#[derive(Debug, Clone, Default)]
pub struct ReviewData {
    /// Submitted reviews, oldest first.
    pub reviews: Vec<ReviewEntry>,
    /// The viewer's own pending review.
    pub pending: Option<ReviewEntry>,
    pub requested: Vec<Account>,
    /// The latest decisive state of each reviewer who reviewed (for the sidebar).
    pub latest: Vec<(Account, ReviewState)>,
}

impl ReviewData {
    /// Every comment the viewer sees (submitted reviews plus their own pending one).
    pub fn visible_comments(&self) -> impl Iterator<Item = &(ReviewComment, Option<Account>)> {
        self.reviews.iter().chain(self.pending.iter()).flat_map(|entry| entry.comments.iter())
    }
}

impl PullService {
    /// Writers and the root comment's author may resolve or reopen submitted threads.
    /// Reply ids are normalized to their root; pending reviews remain private.
    pub async fn set_thread_resolved(
        &self,
        actor: &User,
        ctx: &PullContext,
        id: ReviewCommentId,
        resolved: bool,
    ) -> Result<ReviewComment, ServiceError> {
        let store = self.ctx().database.reviews();
        let comment = store.find_comment(ctx.issue().id, id).await?.ok_or(ServiceError::NotFound)?;
        let review = store.find(ctx.issue().id, comment.review_id).await?.ok_or(ServiceError::NotFound)?;
        if review.state == ReviewState::Pending {
            return Err(ServiceError::NotFound);
        }
        let root = match comment.in_reply_to {
            Some(root) => store.find_comment(ctx.issue().id, root).await?.ok_or(ServiceError::NotFound)?,
            None => comment,
        };
        let review = store.find(ctx.issue().id, root.review_id).await?.ok_or(ServiceError::NotFound)?;
        if review.state == ReviewState::Pending || root.in_reply_to.is_some() {
            return Err(ServiceError::NotFound);
        }
        Self::require_comment(actor, ctx)?;
        if !review_threads::may_resolve(ctx.repo().access.can_write(), actor.id(), root.author) {
            return Err(ServiceError::Forbidden);
        }
        if root.resolved_at.is_some() != resolved {
            self.spend(actor)?;
        }
        store.set_thread_resolved(ctx.issue().id, root.id, actor.id(), resolved, Timestamp::now()).await?;
        store.find_comment(ctx.issue().id, root.id).await?.ok_or(ServiceError::NotFound)
    }

    /// The reviews of `ctx` as `viewer` sees them.
    pub async fn reviews(&self, viewer: Option<&User>, ctx: &PullContext) -> Result<ReviewData, ServiceError> {
        let store = self.ctx().database.reviews();
        let (reviews, comments, requested) =
            tokio::try_join!(store.reviews(ctx.issue().id), store.comments(ctx.issue().id), store.requested(ctx.issue().id))?;
        let mut ids: Vec<AccountId> = reviews.iter().flat_map(|r| [r.reviewer, r.dismissed_by]).flatten().collect();
        ids.extend(comments.iter().filter_map(|c| c.author));
        ids.extend(requested.iter().copied());
        ids.sort();
        ids.dedup();
        let accounts: HashMap<AccountId, Account> =
            self.ctx().database.accounts().find_accounts(&ids).await?.into_iter().map(|account| (account.id, account)).collect();
        let account = |id: Option<AccountId>| id.and_then(|id| accounts.get(&id).cloned());

        let mut by_review: HashMap<ReviewId, Vec<(ReviewComment, Option<Account>)>> = HashMap::new();
        for comment in comments {
            let author = account(comment.author);
            by_review.entry(comment.review_id).or_default().push((comment, author));
        }
        let viewer_id = viewer.map(User::id);
        let mut data =
            ReviewData { requested: requested.iter().filter_map(|id| accounts.get(id).cloned()).collect(), ..ReviewData::default() };
        let mut latest: Vec<(AccountId, ReviewState)> = Vec::new();
        for review in reviews {
            let entry = ReviewEntry {
                reviewer: account(review.reviewer),
                dismissed_by: account(review.dismissed_by),
                comments: by_review.remove(&review.id).unwrap_or_default(),
                review,
            };
            if entry.review.state == ReviewState::Pending {
                if entry.review.reviewer.is_some() && entry.review.reviewer == viewer_id {
                    data.pending = Some(entry);
                }
                continue;
            }
            if let Some(reviewer) = entry.review.reviewer {
                let state = entry.review.state;
                match latest.iter_mut().find(|(id, _)| *id == reviewer) {
                    // A comment never replaces an earlier verdict.
                    Some(slot) if state == ReviewState::Commented && slot.1 != ReviewState::Commented => {}
                    Some(slot) => slot.1 = state,
                    None => latest.push((reviewer, state)),
                }
            }
            data.reviews.push(entry);
        }
        data.latest = latest.into_iter().filter_map(|(id, state)| accounts.get(&id).cloned().map(|account| (account, state))).collect();
        Ok(data)
    }

    /// Comments on a diff line: at once (a `commented` review) or into the viewer's
    /// pending review.
    pub async fn comment_on_line(&self, actor: &User, ctx: &PullContext, input: LineComment) -> Result<ReviewComment, ServiceError> {
        Self::require_comment(actor, ctx)?;
        let body = IssueText::comment(&input.body)?;
        let store = self.ctx().database.reviews();
        let head = ctx.pull.head_sha.clone();
        let new = if let Some(parent) = input.in_reply_to {
            {
                let parent = store.find_comment(ctx.issue().id, parent).await?.ok_or(ServiceError::NotFound)?;
                let review = store.find(ctx.issue().id, parent.review_id).await?;
                if !review_threads::reply_visible(review.map(|review| (review.state == ReviewState::Pending, review.reviewer)), actor.id())
                {
                    return Err(ServiceError::NotFound);
                }
                NewReviewComment {
                    path: parent.path,
                    side: parent.side,
                    line: parent.line,
                    commit_sha: parent.commit_sha,
                    diff_hunk: parent.diff_hunk,
                    in_reply_to: Some(review_threads::thread_root(parent.id, parent.in_reply_to)),
                    body: body.into(),
                }
            }
        } else {
            let diff_hunk = self.diff_hunk(ctx, &input.path, input.side, input.line).await?;
            NewReviewComment {
                path: input.path,
                side: input.side,
                line: input.line,
                commit_sha: head.clone(),
                diff_hunk,
                in_reply_to: None,
                body: body.into(),
            }
        };
        self.spend(actor)?;
        let now = Timestamp::now();
        let has_pending = async { Ok::<_, ServiceError>(store.pending(ctx.issue().id, actor.id()).await?.is_some()) };
        if review_threads::joins_pending_review(input.start_review, has_pending).await? {
            let (_, comment) = store.add_pending_comment(ctx.issue().id, actor.id(), &head, &new, now).await?;
            return Ok(comment);
        }
        let review =
            NewReview { pull: ctx.issue().id, reviewer: actor.id(), state: ReviewState::Commented, body: "", commit_sha: &head, at: now };
        let (review, mut comments) = store.create_submitted(&review, std::slice::from_ref(&new)).await?;
        self.publish_review(ctx, review.id, actor.id(), ReviewAction::Submitted);
        comments.pop().ok_or_else(|| ServiceError::Invalid("the comment was not stored".to_owned()))
    }

    /// Submits the viewer's review: the pending one with its comments, or a new one.
    /// Comments and change requests need a body when there are no line comments.
    pub async fn submit_review(&self, actor: &User, ctx: &PullContext, verdict: ReviewVerdict, body: &str) -> Result<Review, ServiceError> {
        Self::require_comment(actor, ctx)?;
        if let Some(refusal) = verdict_refusal(verdict, ctx.issue().is_author(actor), ctx.issue().is_open()) {
            return Err(ServiceError::Invalid(refusal.to_owned()));
        }
        let body = IssueText::body(body)?;
        let store = self.ctx().database.reviews();
        let head = ctx.pull.head_sha.clone();
        let pending = store.pending(ctx.issue().id, actor.id()).await?;
        let has_comments = match &pending {
            Some(review) => store.comments(ctx.issue().id).await?.iter().any(|comment| comment.review_id == review.id),
            None => false,
        };
        if let Some(refusal) = empty_review_refusal(verdict, body.is_empty(), has_comments) {
            return Err(ServiceError::Invalid(refusal.to_owned()));
        }
        self.spend(actor)?;
        let now = Timestamp::now();
        let review = if pending.is_some() {
            store
                .submit_pending(ctx.issue().id, actor.id(), verdict.state(), body.as_str(), &head, now)
                .await?
                .ok_or(ServiceError::NotFound)?
        } else {
            let new = NewReview {
                pull: ctx.issue().id,
                reviewer: actor.id(),
                state: verdict.state(),
                body: body.as_str(),
                commit_sha: &head,
                at: now,
            };
            store.create_submitted(&new, &[]).await?.0
        };
        self.publish_review(ctx, review.id, actor.id(), ReviewAction::Submitted);
        Ok(review)
    }

    /// Throws away the viewer's pending review and its comments.
    pub async fn discard_review(&self, actor: &User, ctx: &PullContext) -> Result<bool, ServiceError> {
        ctx.repo().ensure_writable()?;
        Ok(self.ctx().database.reviews().discard_pending(ctx.issue().id, actor.id()).await?)
    }

    /// Edits the body of the actor's own submitted review.
    pub async fn edit_review(&self, actor: &User, ctx: &PullContext, id: ReviewId, body: &str) -> Result<Review, ServiceError> {
        let store = self.ctx().database.reviews();
        let review = store.find(ctx.issue().id, id).await?.ok_or(ServiceError::NotFound)?;
        may_edit_review(review.state, Some(actor.id()) == review.reviewer).map_err(refusal)?;
        Self::require_comment(actor, ctx)?;
        let body = IssueText::body(body)?;
        if body.as_str() == review.body {
            return Ok(review);
        }
        self.spend(actor)?;
        let edited = store.edit_body(ctx.issue().id, id, body.as_str()).await?.ok_or(ServiceError::NotFound)?;
        self.publish_review(ctx, id, actor.id(), ReviewAction::Edited { previous_body: review.body });
        Ok(edited)
    }

    /// Dismisses an approval or a change request (writers), with a message.
    pub async fn dismiss_review(&self, actor: &User, ctx: &PullContext, id: ReviewId, message: &str) -> Result<Review, ServiceError> {
        let store = self.ctx().database.reviews();
        let review = store.find(ctx.issue().id, id).await?.ok_or(ServiceError::NotFound)?;
        may_dismiss_review(review.state, ctx.repo().access.can_write()).map_err(refusal)?;
        ctx.repo().ensure_writable()?;
        let message = dismissal_message(message).map_err(|refusal| ServiceError::Invalid(refusal.to_owned()))?;
        let message = IssueText::comment(message)?;
        let dismissed = store
            .dismiss(ctx.issue().id, id, actor.id(), message.as_str(), Timestamp::now())
            .await?
            .ok_or_else(|| ServiceError::Invalid(NOT_DISMISSIBLE.to_owned()))?;
        let timeline = self.inner.timeline.writer();
        timeline.record(ctx.issue().id, Some(actor.id()), TimelineEvent::ReviewDismissed { review: id, reviewer: review.reviewer }).await?;
        self.publish_review(ctx, id, actor.id(), ReviewAction::Dismissed);
        Ok(dismissed)
    }

    /// Asks `names` to review (writers and the author). Unknown names, the author, and
    /// users who cannot read the repository are refused as a whole (`Invalid`).
    pub async fn request_reviewers(&self, actor: &User, ctx: &PullContext, names: &[String]) -> Result<Vec<Account>, ServiceError> {
        Self::require_triage(actor, ctx)?;
        let store = self.ctx().database.reviews();
        let view = RequestView { service: self, ctx };
        let ReviewRequestPlan { accounts, new } =
            ReviewRequests::new(&view, ctx.issue().author).plan(names).await.map_err(request_error)?;
        let now = Timestamp::now();
        let timeline = self.inner.timeline.writer();
        for reviewer in new {
            if store.request(ctx.issue().id, reviewer, actor.id(), now).await? {
                timeline.record(ctx.issue().id, Some(actor.id()), TimelineEvent::ReviewRequested { reviewer }).await?;
                self.publish(ctx.issue(), Some(actor.id()), IssueAction::ReviewRequested { reviewer });
            }
        }
        Ok(accounts)
    }

    /// Withdraws review requests (writers and the author).
    pub async fn remove_reviewers(&self, actor: &User, ctx: &PullContext, names: &[String]) -> Result<(), ServiceError> {
        Self::require_triage(actor, ctx)?;
        let store = self.ctx().database.reviews();
        let timeline = self.inner.timeline.writer();
        let view = RequestView { service: self, ctx };
        ReviewRequests::new(&view, ctx.issue().author)
            .withdraw(names, |reviewer| {
                let (store, timeline) = (&store, &timeline);
                async move {
                    if store.remove_request(ctx.issue().id, reviewer).await? {
                        timeline.record(ctx.issue().id, Some(actor.id()), TimelineEvent::ReviewRequestRemoved { reviewer }).await?;
                        self.publish(ctx.issue(), Some(actor.id()), IssueAction::ReviewRequestRemoved { reviewer });
                    }
                    Ok(())
                }
            })
            .await
    }

    /// Up to [`DIFF_HUNK_LINES`] diff lines ending at `line` on `side` of `path`, with the
    /// hunk header. `Invalid` when that line is not part of the pull request's diff.
    async fn diff_hunk(&self, ctx: &PullContext, path: &str, side: ReviewSide, line: u32) -> Result<String, ServiceError> {
        let changes = self.changes(ctx, DiffLimits::default()).await?;
        let not_in_diff = || ServiceError::Invalid("that line is not part of this pull request's changes".to_owned());
        let file = changes.files.iter().find(|file| file.path == path).ok_or_else(not_in_diff)?;
        let FileDiffBody::Text(hunks) = &file.body else { return Err(not_in_diff()) };
        hunk_excerpt(hunks, side, line).ok_or_else(not_in_diff)
    }

    fn require_comment(actor: &User, ctx: &PullContext) -> Result<(), ServiceError> {
        if !issue_permissions(Some(actor), ctx.repo().access, ctx.issue()).can_comment {
            return Err(ServiceError::Forbidden);
        }
        ctx.repo().ensure_writable()
    }

    fn require_triage(actor: &User, ctx: &PullContext) -> Result<(), ServiceError> {
        let permissions = issue_permissions(Some(actor), ctx.repo().access, ctx.issue());
        if !may_request_reviews(permissions.can_triage, ctx.issue().is_author(actor), permissions.can_comment) {
            return Err(ServiceError::Forbidden);
        }
        ctx.repo().ensure_writable()
    }

    pub(super) fn publish_review(&self, ctx: &PullContext, review: ReviewId, actor: AccountId, action: ReviewAction) {
        let issue = ctx.issue();
        self.ctx().events.publish(DomainEvent::PullRequestReview {
            repo: issue.repo_id,
            pull: issue.id,
            number: issue.number,
            review,
            actor,
            action,
        });
    }
}

/// [`rbit_collaboration::pulls::hunk_excerpt`] over Git's diff hunks.
pub fn hunk_excerpt(hunks: &[Hunk], side: ReviewSide, line: u32) -> Option<String> {
    let hunks = hunks.iter().map(|hunk| (hunk.header.as_str(), hunk.lines.as_slice()));
    rbit_collaboration::pulls::hunk_excerpt(hunks, side, line, |line| AnchorLine {
        mark: match line.kind {
            LineKind::Context => LineMark::Context,
            LineKind::Added => LineMark::Added,
            LineKind::Removed => LineMark::Removed,
        },
        old_number: line.old_number,
        new_number: line.new_number,
        text: &line.text,
    })
}

/// The service error of a review the caller may not edit or dismiss.
fn refusal(refusal: ReviewRefusal) -> ServiceError {
    match refusal {
        ReviewRefusal::NotFound => ServiceError::NotFound,
        ReviewRefusal::Forbidden => ServiceError::Forbidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_end_at_the_commented_line() {
        let changes = rbit_git::diff::diff_bytes(b"a\nb\nc\nd\ne\nf\n", b"a\nb\nc\nd\nE\nf\ng\n");
        let right = hunk_excerpt(&changes.hunks, ReviewSide::Right, 5).unwrap();
        assert_eq!(right, "@@ -2,5 +2,6 @@\n c\n d\n-e\n+E");
        let left = hunk_excerpt(&changes.hunks, ReviewSide::Left, 5).unwrap();
        assert!(left.ends_with("\n-e"), "{left}");
        assert!(hunk_excerpt(&changes.hunks, ReviewSide::Right, 1).is_none(), "outside every hunk");
        assert!(hunk_excerpt(&changes.hunks, ReviewSide::Left, 7).is_none(), "an added line has no left side");
        assert_eq!(ReviewVerdict::parse("REQUEST_CHANGES"), Some(ReviewVerdict::RequestChanges));
        assert_eq!(ReviewVerdict::parse("nope"), None);
    }
}
