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

use rbit_core::{
    Account, AccountId, IssueText, ReviewId, TimelineEvent, Timestamp, User,
    issue::{IssueAction, ReviewAction, ReviewState, issue_permissions},
    pull::{MAX_REQUESTED_REVIEWERS, Review, ReviewComment, ReviewCommentId, ReviewSide},
};
use rbit_git::{
    DiffLimits, FileDiffBody,
    diff::{Hunk, LineKind},
};
use rbit_storage::sqlite::reviews::{NewReview, NewReviewComment};

use super::{PullContext, PullService};
use crate::{DomainEvent, ServiceError};

/// Diff lines kept with a comment (ending at the commented line).
pub const DIFF_HUNK_LINES: usize = 4;

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

/// How a review is submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewVerdict {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "comment" | "COMMENT" => Some(Self::Comment),
            "approve" | "APPROVE" => Some(Self::Approve),
            "request_changes" | "REQUEST_CHANGES" => Some(Self::RequestChanges),
            _ => None,
        }
    }

    const fn state(self) -> ReviewState {
        match self {
            Self::Comment => ReviewState::Commented,
            Self::Approve => ReviewState::Approved,
            Self::RequestChanges => ReviewState::ChangesRequested,
        }
    }
}

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
        if !ctx.repo().access.can_write() && root.author != Some(actor.id()) {
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
                let visible = match store.find(ctx.issue().id, parent.review_id).await? {
                    Some(review) => review.state != ReviewState::Pending || review.reviewer == Some(actor.id()),
                    None => false,
                };
                if !visible {
                    return Err(ServiceError::NotFound);
                }
                NewReviewComment {
                    path: parent.path,
                    side: parent.side,
                    line: parent.line,
                    commit_sha: parent.commit_sha,
                    diff_hunk: parent.diff_hunk,
                    in_reply_to: Some(parent.in_reply_to.unwrap_or(parent.id)),
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
        if input.start_review || store.pending(ctx.issue().id, actor.id()).await?.is_some() {
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
        if verdict != ReviewVerdict::Comment && ctx.issue().is_author(actor) {
            return Err(ServiceError::Invalid("you cannot approve or request changes on your own pull request".to_owned()));
        }
        if !ctx.issue().is_open() && verdict != ReviewVerdict::Comment {
            return Err(ServiceError::Invalid("the pull request is closed".to_owned()));
        }
        let body = IssueText::body(body)?;
        let store = self.ctx().database.reviews();
        let head = ctx.pull.head_sha.clone();
        let pending = store.pending(ctx.issue().id, actor.id()).await?;
        let has_comments = match &pending {
            Some(review) => store.comments(ctx.issue().id).await?.iter().any(|comment| comment.review_id == review.id),
            None => false,
        };
        if body.is_empty() && !has_comments && verdict != ReviewVerdict::Approve {
            return Err(ServiceError::Invalid("write a comment or leave line comments first".to_owned()));
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
        Ok(self.ctx().database.reviews().discard_pending(ctx.issue().id, actor.id()).await?)
    }

    /// Edits the body of the actor's own submitted review.
    pub async fn edit_review(&self, actor: &User, ctx: &PullContext, id: ReviewId, body: &str) -> Result<Review, ServiceError> {
        let store = self.ctx().database.reviews();
        let review = store.find(ctx.issue().id, id).await?.ok_or(ServiceError::NotFound)?;
        if review.state == ReviewState::Pending || review.reviewer != Some(actor.id()) {
            return Err(if review.state == ReviewState::Pending { ServiceError::NotFound } else { ServiceError::Forbidden });
        }
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
        if review.state == ReviewState::Pending {
            return Err(ServiceError::NotFound);
        }
        if !ctx.repo().access.can_write() {
            return Err(ServiceError::Forbidden);
        }
        let message = message.trim();
        if message.is_empty() {
            return Err(ServiceError::Invalid("say why the review is dismissed".to_owned()));
        }
        let message = IssueText::comment(message)?;
        let dismissed = store
            .dismiss(ctx.issue().id, id, actor.id(), message.as_str(), Timestamp::now())
            .await?
            .ok_or_else(|| ServiceError::Invalid("only approvals and change requests can be dismissed".to_owned()))?;
        self.inner
            .timeline
            .record(ctx.issue().id, Some(actor.id()), TimelineEvent::ReviewDismissed { review: id, reviewer: review.reviewer })
            .await?;
        self.publish_review(ctx, id, actor.id(), ReviewAction::Dismissed);
        Ok(dismissed)
    }

    /// Asks `names` to review (writers and the author). Unknown names, the author, and
    /// users who cannot read the repository are refused as a whole (`Invalid`).
    pub async fn request_reviewers(&self, actor: &User, ctx: &PullContext, names: &[String]) -> Result<Vec<Account>, ServiceError> {
        Self::require_triage(actor, ctx)?;
        let store = self.ctx().database.reviews();
        let mut accounts = Vec::new();
        for name in names.iter().map(|name| name.trim()).filter(|name| !name.is_empty()) {
            let account = self.ctx().database.accounts().find_account_by_name(name).await?;
            let account = match account {
                Some(account)
                    if account.kind == rbit_core::AccountKind::User && self.access_of(account.id, &ctx.repo().repo).await?.can_read() =>
                {
                    account
                }
                // The same answer whether the account does not exist or cannot read the repository.
                _ => return Err(ServiceError::Invalid(format!("{name} cannot be asked to review this pull request"))),
            };
            if Some(account.id) == ctx.issue().author {
                return Err(ServiceError::Invalid("the author cannot review their own pull request".to_owned()));
            }
            accounts.push(account);
        }
        let existing = store.requested(ctx.issue().id).await?;
        let new: Vec<&Account> = accounts.iter().filter(|account| !existing.contains(&account.id)).collect();
        if existing.len() + new.len() > MAX_REQUESTED_REVIEWERS {
            return Err(ServiceError::Invalid(format!("at most {MAX_REQUESTED_REVIEWERS} reviewers can be requested")));
        }
        let now = Timestamp::now();
        for account in new {
            if store.request(ctx.issue().id, account.id, actor.id(), now).await? {
                self.inner
                    .timeline
                    .record(ctx.issue().id, Some(actor.id()), TimelineEvent::ReviewRequested { reviewer: account.id })
                    .await?;
                self.publish(ctx.issue(), Some(actor.id()), IssueAction::ReviewRequested { reviewer: account.id });
            }
        }
        Ok(accounts)
    }

    /// Withdraws review requests (writers and the author).
    pub async fn remove_reviewers(&self, actor: &User, ctx: &PullContext, names: &[String]) -> Result<(), ServiceError> {
        Self::require_triage(actor, ctx)?;
        let store = self.ctx().database.reviews();
        for name in names.iter().map(|name| name.trim()).filter(|name| !name.is_empty()) {
            let Some(account) = self.ctx().database.accounts().find_account_by_name(name).await? else { continue };
            if store.remove_request(ctx.issue().id, account.id).await? {
                self.inner
                    .timeline
                    .record(ctx.issue().id, Some(actor.id()), TimelineEvent::ReviewRequestRemoved { reviewer: account.id })
                    .await?;
                self.publish(ctx.issue(), Some(actor.id()), IssueAction::ReviewRequestRemoved { reviewer: account.id });
            }
        }
        Ok(())
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
        if issue_permissions(Some(actor), ctx.repo().access, ctx.issue()).can_comment { Ok(()) } else { Err(ServiceError::Forbidden) }
    }

    fn require_triage(actor: &User, ctx: &PullContext) -> Result<(), ServiceError> {
        let permissions = issue_permissions(Some(actor), ctx.repo().access, ctx.issue());
        if permissions.can_triage || (ctx.issue().is_author(actor) && permissions.can_comment) {
            Ok(())
        } else {
            Err(ServiceError::Forbidden)
        }
    }

    fn publish_review(&self, ctx: &PullContext, review: ReviewId, actor: AccountId, action: ReviewAction) {
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

/// The hunk header and up to [`DIFF_HUNK_LINES`] lines ending at `line` on `side`, in
/// unified diff form; `None` when no such line is shown.
pub fn hunk_excerpt(hunks: &[Hunk], side: ReviewSide, line: u32) -> Option<String> {
    let line = usize::try_from(line).ok()?;
    for hunk in hunks {
        let position = hunk.lines.iter().position(|diff_line| match side {
            ReviewSide::Left => diff_line.kind != LineKind::Added && diff_line.old_number == Some(line),
            ReviewSide::Right => diff_line.kind != LineKind::Removed && diff_line.new_number == Some(line),
        });
        if let Some(end) = position {
            let start = (end + 1).saturating_sub(DIFF_HUNK_LINES);
            let mut excerpt = hunk.header.clone();
            for diff_line in &hunk.lines[start..=end] {
                let sign = match diff_line.kind {
                    LineKind::Added => '+',
                    LineKind::Removed => '-',
                    LineKind::Context => ' ',
                };
                excerpt.push('\n');
                excerpt.push(sign);
                excerpt.push_str(&diff_line.text);
            }
            return Some(excerpt);
        }
    }
    None
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
