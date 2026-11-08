//! Reviews: the Files changed tab, line comments and their threads, the review form, a
//! submitted review and the reviewers in the sidebar.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewMessage<'a> {
    /// The accessible name of the Files changed section.
    Changes,
    ToggleFileTree,
    CollapseAll,
    ExpandAll,
    OutdatedComments {
        count: u64,
    },
    FinishReview,
    Truncated,
    FileToggleHint,
    ViewFile,
    CommentOnLine {
        line: u64,
    },
    StartReview,
    AddSingleComment,
    BinaryFile,
    TooLarge,
    SubmoduleCommit,
    NoContentChanges,
    NoChangesTitle,
    NoChangesText,
    /// A changed file's status.
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    /// Line comments.
    Outdated,
    Pending,
    /// `path` and `line` (`R12`) name the commented line.
    ResolvedThread {
        path: &'a str,
        line: &'a str,
    },
    ReplyPlaceholder,
    AddToReview,
    ReopenConversation,
    ResolveConversation,
    /// The review form.
    ReviewChanges,
    PendingCount {
        count: u64,
    },
    ReviewPlaceholder,
    RequestChanges,
    Approve,
    DiscardReview,
    /// A submitted review.
    DismissReview,
    DismissReason,
    Dismiss,
    DismissedBy {
        by: &'a str,
        message: &'a str,
    },
    /// The reviewers in the sidebar.
    Reviewers,
    NoReviews,
    RemoveRequestFor {
        name: &'a str,
    },
    RemoveRequest,
    RequestReviewFrom,
    Username,
    RequestReview,
    Approved,
    ChangesRequested,
    Dismissed,
    Commented,
    AwaitingReview,
}

impl MessageFamily for ReviewMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Changes => "reviews.files.label",
            Self::ToggleFileTree => "reviews.files.toggle-tree",
            Self::CollapseAll => "reviews.files.collapse-all",
            Self::ExpandAll => "reviews.files.expand-all",
            Self::OutdatedComments { .. } => "reviews.files.outdated",
            Self::FinishReview => "reviews.files.finish",
            Self::Truncated => "reviews.files.truncated",
            Self::FileToggleHint => "reviews.files.toggle-hint",
            Self::ViewFile => "reviews.files.view-file",
            Self::CommentOnLine { .. } => "reviews.files.comment-on-line",
            Self::StartReview => "reviews.files.start-review",
            Self::AddSingleComment => "reviews.files.single-comment",
            Self::BinaryFile => "reviews.files.binary",
            Self::TooLarge => "reviews.files.too-large",
            Self::SubmoduleCommit => "reviews.files.submodule",
            Self::NoContentChanges => "reviews.files.no-content-changes",
            Self::NoChangesTitle => "reviews.files.empty-title",
            Self::NoChangesText => "reviews.files.empty-text",
            Self::Added => "reviews.status.added",
            Self::Deleted => "reviews.status.deleted",
            Self::Modified => "reviews.status.modified",
            Self::Renamed => "reviews.status.renamed",
            Self::Copied => "reviews.status.copied",
            Self::Outdated => "reviews.comment.outdated",
            Self::Pending => "reviews.comment.pending",
            Self::ResolvedThread { .. } => "reviews.thread.resolved",
            Self::ReplyPlaceholder => "reviews.thread.reply-placeholder",
            Self::AddToReview => "reviews.thread.add-to-review",
            Self::ReopenConversation => "reviews.thread.reopen",
            Self::ResolveConversation => "reviews.thread.resolve",
            Self::ReviewChanges => "reviews.form.title",
            Self::PendingCount { .. } => "reviews.form.pending",
            Self::ReviewPlaceholder => "reviews.form.placeholder",
            Self::RequestChanges => "reviews.form.request-changes",
            Self::Approve => "reviews.form.approve",
            Self::DiscardReview => "reviews.form.discard",
            Self::DismissReview => "reviews.review.dismiss",
            Self::DismissReason => "reviews.review.dismiss-reason",
            Self::Dismiss => "reviews.review.dismiss-submit",
            Self::DismissedBy { .. } => "reviews.review.dismissed-by",
            Self::Reviewers => "reviews.reviewers.title",
            Self::NoReviews => "reviews.reviewers.none",
            Self::RemoveRequestFor { .. } => "reviews.reviewers.remove-for",
            Self::RemoveRequest => "reviews.reviewers.remove",
            Self::RequestReviewFrom => "reviews.reviewers.request-from",
            Self::Username => "reviews.reviewers.username",
            Self::RequestReview => "reviews.reviewers.request",
            Self::Approved => "reviews.state.approved",
            Self::ChangesRequested => "reviews.state.changes-requested",
            Self::Dismissed => "reviews.state.dismissed",
            Self::Commented => "reviews.state.commented",
            Self::AwaitingReview => "reviews.state.awaiting",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::OutdatedComments { count } => {
                text.number(count);
                text.literal(if count == 1 {
                    " outdated comment is in the conversation"
                } else {
                    " outdated comments are in the conversation"
                });
            }
            Self::CommentOnLine { line } => {
                text.literal("Comment on line ");
                text.number(line);
            }
            Self::ResolvedThread { path, line } => {
                text.literal("Resolved conversation · ");
                text.argument(path);
                text.literal(" ");
                text.argument(line);
            }
            Self::PendingCount { count } => {
                text.number(count);
                text.literal(" pending");
            }
            Self::DismissedBy { by, message } => {
                text.literal("Dismissed by ");
                text.argument(by);
                text.literal(": ");
                text.argument(message);
            }
            Self::RemoveRequestFor { name } => {
                text.literal("Remove the review request for ");
                text.argument(name);
            }
            literal => text.literal(literal_text(literal)),
        }
    }
}

/// The English text of the messages without arguments.
const fn literal_text(message: ReviewMessage<'_>) -> &'static str {
    match message {
        ReviewMessage::Changes => "Changes",
        ReviewMessage::ToggleFileTree => "Toggle file tree",
        ReviewMessage::CollapseAll => "Collapse all",
        ReviewMessage::ExpandAll => "Expand all",
        ReviewMessage::FinishReview => "Finish your review",
        ReviewMessage::Truncated => "This change is very large, so some diffs are not shown. Clone the repository to see the full change.",
        ReviewMessage::FileToggleHint => "Click to collapse or expand | Alt-click for every file",
        ReviewMessage::ViewFile => "View file",
        ReviewMessage::StartReview => "Start a review",
        ReviewMessage::AddSingleComment => "Add single comment",
        ReviewMessage::BinaryFile => "Binary file not shown.",
        ReviewMessage::TooLarge => "Diff too large to display.",
        ReviewMessage::SubmoduleCommit => "Submodule commit",
        ReviewMessage::NoContentChanges => "No content changes.",
        ReviewMessage::NoChangesTitle => "No changes",
        ReviewMessage::NoChangesText => "The head adds nothing to the base.",
        ReviewMessage::Added => "Added",
        ReviewMessage::Deleted => "Deleted",
        ReviewMessage::Modified => "Modified",
        ReviewMessage::Renamed => "Renamed",
        ReviewMessage::Copied => "Copied",
        ReviewMessage::Outdated => "Outdated",
        ReviewMessage::Pending => "Pending",
        ReviewMessage::ReplyPlaceholder => "Reply…",
        ReviewMessage::AddToReview => "Add to review",
        ReviewMessage::ReopenConversation => "Reopen conversation",
        ReviewMessage::ResolveConversation => "Resolve conversation",
        ReviewMessage::ReviewChanges => "Review changes",
        ReviewMessage::ReviewPlaceholder => "Leave a comment (Markdown is supported)",
        ReviewMessage::RequestChanges => "Request changes",
        ReviewMessage::Approve => "Approve",
        ReviewMessage::DiscardReview => "Discard pending review",
        ReviewMessage::DismissReview => "Dismiss review",
        ReviewMessage::DismissReason => "Why is this review dismissed?",
        ReviewMessage::Dismiss => "Dismiss",
        ReviewMessage::Reviewers => "Reviewers",
        ReviewMessage::NoReviews => "No reviews yet",
        ReviewMessage::RemoveRequest => "Remove request",
        ReviewMessage::RequestReviewFrom => "Request a review from",
        ReviewMessage::Username => "username",
        ReviewMessage::RequestReview => "Request review",
        ReviewMessage::Approved => "Approved",
        ReviewMessage::ChangesRequested => "Changes requested",
        ReviewMessage::Dismissed => "Dismissed",
        ReviewMessage::Commented => "Commented",
        ReviewMessage::AwaitingReview => "Awaiting review",
        ReviewMessage::OutdatedComments { .. }
        | ReviewMessage::CommentOnLine { .. }
        | ReviewMessage::ResolvedThread { .. }
        | ReviewMessage::PendingCount { .. }
        | ReviewMessage::DismissedBy { .. }
        | ReviewMessage::RemoveRequestFor { .. } => "",
    }
}
