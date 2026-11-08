//! Sentences of the issue and pull request pages that embed markup. Each variant says
//! what its slots hold; the template renders them.
use super::rich::{RichFamily, RichWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollabSentence<'a> {
    /// A state tab. Slot 0: the count.
    OpenCount {
        count: u64,
    },
    ClosedCount {
        count: u64,
    },
    /// The issue search help. Slots 0–3: example terms.
    IssueSearchHelp,
    /// The pull request search help. Slots 0–3: example terms.
    PullSearchHelp,
    /// Under an issue's title. Slot 0: the author; slot 1: when.
    OpenedIssue {
        comments: u64,
    },
    OpenedPull {
        comments: u64,
    },
    /// A list row. Slot 0: when.
    RowOpened {
        number: &'a str,
        author: &'a str,
    },
    RowClosed {
        number: &'a str,
    },
    RowClosedBy {
        number: &'a str,
        author: &'a str,
    },
    RowMergedBy {
        number: &'a str,
        author: &'a str,
    },
    /// No assignees. Slot 0: the "assign yourself" button.
    NoOneAssignYourself,
    /// Slot 0: the percentage.
    PercentComplete,
    /// A pull request's header. Slot 0: the author; 1: the commits; 2: the base; 3: the head.
    WantsToMerge,
    MergedInto,
    ProposedToMerge,
    /// Slot 0: the "Sign in" link.
    SignInToOpenPull,
    SignInToCommentOnLines,
    SignInToJoin,
    /// Slot 0: the merge commit.
    PullMergedAs,
    /// Slot 0: the number of files.
    FilesChanged {
        count: u64,
    },
    /// A comment's header. Slot 0: when.
    CommentOpened,
    CommentCommented,
    /// A submitted review. Slot 0: the reviewer.
    ReviewApproved,
    ReviewChangesRequested,
    ReviewDismissed,
    Reviewed,
}

impl RichFamily for CollabSentence<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::OpenCount { .. } => "issues.tabs.open",
            Self::ClosedCount { .. } => "issues.tabs.closed",
            Self::IssueSearchHelp => "issues.search.help-text",
            Self::PullSearchHelp => "pulls.search.help-text",
            Self::OpenedIssue { .. } => "issues.page.opened",
            Self::OpenedPull { .. } => "pulls.page.opened",
            Self::RowOpened { .. } => "issues.row.opened",
            Self::RowClosed { .. } => "issues.row.closed",
            Self::RowClosedBy { .. } => "pulls.row.closed",
            Self::RowMergedBy { .. } => "pulls.row.merged",
            Self::NoOneAssignYourself => "issues.sidebar.no-one-assign-yourself",
            Self::PercentComplete => "milestones.progress.percent",
            Self::WantsToMerge => "pulls.header.wants-to-merge",
            Self::MergedInto => "pulls.header.merged",
            Self::ProposedToMerge => "pulls.header.proposed",
            Self::SignInToOpenPull => "pulls.new.sign-in",
            Self::SignInToCommentOnLines => "reviews.files.sign-in",
            Self::SignInToJoin => "timeline.comment-form.sign-in",
            Self::PullMergedAs => "pulls.merged-as",
            Self::FilesChanged { .. } => "reviews.files.changed",
            Self::CommentOpened => "timeline.comment.opened",
            Self::CommentCommented => "timeline.comment.commented",
            Self::ReviewApproved => "reviews.review.approved",
            Self::ReviewChangesRequested => "reviews.review.changes-requested",
            Self::ReviewDismissed => "reviews.review.dismissed",
            Self::Reviewed => "reviews.review.commented",
        }
    }

    fn english(self, text: &mut RichWriter) {
        match self {
            Self::OpenCount { .. } => slot_then(text, 0, " Open"),
            Self::ClosedCount { .. } => slot_then(text, 0, " Closed"),
            Self::IssueSearchHelp => {
                text.literal("Use the menus to filter results, or type terms such as ");
                examples(text);
                text.literal(". Text searches titles and descriptions. Each menu keeps your other filters.");
            }
            Self::PullSearchHelp => {
                text.literal("Choose filters below, or type terms such as ");
                examples(text);
                text.literal(".");
            }
            Self::OpenedIssue { comments } | Self::OpenedPull { comments } => {
                text.slot(0);
                text.literal(if matches!(self, Self::OpenedPull { .. }) { " opened this pull request " } else { " opened this issue " });
                text.slot(1);
                text.literal(" · ");
                text.number(comments);
                text.literal(if comments == 1 { " comment" } else { " comments" });
            }
            Self::RowOpened { number, author } | Self::RowClosedBy { number, author } | Self::RowMergedBy { number, author } => {
                text.argument(number);
                text.literal(match self {
                    Self::RowClosedBy { .. } => " closed ",
                    Self::RowMergedBy { .. } => " merged ",
                    _ => " opened ",
                });
                text.slot(0);
                text.literal(" by ");
                text.argument(author);
            }
            Self::RowClosed { number } => {
                text.argument(number);
                text.literal(" closed ");
                text.slot(0);
            }
            Self::NoOneAssignYourself => {
                text.literal("No one — ");
                text.slot(0);
            }
            Self::PercentComplete => slot_then(text, 0, " complete"),
            Self::WantsToMerge | Self::MergedInto | Self::ProposedToMerge => {
                text.slot(0);
                text.literal(match self {
                    Self::MergedInto => " merged ",
                    Self::ProposedToMerge => " proposed to merge ",
                    _ => " wants to merge ",
                });
                text.slot(1);
                text.literal(" into ");
                text.slot(2);
                text.literal(" from ");
                text.slot(3);
            }
            Self::SignInToOpenPull => slot_then(text, 0, " to open a pull request."),
            Self::SignInToCommentOnLines => slot_then(text, 0, " to comment on lines."),
            Self::SignInToJoin => slot_then(text, 0, " to join the conversation."),
            Self::PullMergedAs => {
                text.literal("Pull request merged as ");
                text.slot(0);
                text.literal(".");
            }
            Self::FilesChanged { count } => slot_then(text, 0, if count == 1 { " file changed" } else { " files changed" }),
            Self::CommentOpened => {
                text.literal("opened ");
                text.slot(0);
            }
            Self::CommentCommented => {
                text.literal("commented ");
                text.slot(0);
            }
            Self::ReviewApproved => slot_then(text, 0, " approved these changes"),
            Self::ReviewChangesRequested => slot_then(text, 0, " requested changes"),
            Self::ReviewDismissed => slot_then(text, 0, " reviewed (dismissed)"),
            Self::Reviewed => slot_then(text, 0, " reviewed"),
        }
    }
}

fn slot_then(text: &mut RichWriter, slot: usize, words: &str) {
    text.slot(slot);
    text.literal(words);
}

/// "a, b, c or d" over slots 0–3.
fn examples(text: &mut RichWriter) {
    text.slot(0);
    text.literal(", ");
    text.slot(1);
    text.literal(", ");
    text.slot(2);
    text.literal(" or ");
    text.slot(3);
}
