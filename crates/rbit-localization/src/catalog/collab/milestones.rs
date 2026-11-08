//! Milestone list, page and form.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneMessage<'a> {
    NewMilestone,
    EditMilestone,
    EmptyTitle,
    EmptyText,
    /// `when` is a relative time.
    ClosedWhen {
        when: &'a str,
    },
    /// `due` is a formatted date.
    DueBy {
        due: &'a str,
    },
    PastDueBy {
        due: &'a str,
    },
    NoDueDate,
    Close,
    Reopen,
    DeleteConfirm {
        title: &'a str,
    },
    SearchIssues,
    NothingHereTitle,
    NothingHereText,
    FormIntro,
    DueDate,
    MarkdownOptional,
    CreateMilestone,
    SaveMilestone,
    /// The progress bar's accessible name.
    Complete {
        percent: u64,
    },
    OpenIssues {
        count: u64,
    },
    ClosedIssues {
        count: u64,
    },
}

impl MessageFamily for MilestoneMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::NewMilestone => "milestones.new",
            Self::EditMilestone => "milestones.edit",
            Self::EmptyTitle => "milestones.empty-title",
            Self::EmptyText => "milestones.empty-text",
            Self::ClosedWhen { .. } => "milestones.closed-when",
            Self::DueBy { .. } => "milestones.due-by",
            Self::PastDueBy { .. } => "milestones.past-due-by",
            Self::NoDueDate => "milestones.no-due-date",
            Self::Close => "milestones.close",
            Self::Reopen => "milestones.reopen",
            Self::DeleteConfirm { .. } => "milestones.delete-confirm",
            Self::SearchIssues => "milestones.search-issues",
            Self::NothingHereTitle => "milestones.page.empty-title",
            Self::NothingHereText => "milestones.page.empty-text",
            Self::FormIntro => "milestones.form.intro",
            Self::DueDate => "milestones.form.due-date",
            Self::MarkdownOptional => "milestones.form.markdown-optional",
            Self::CreateMilestone => "milestones.form.create",
            Self::SaveMilestone => "milestones.form.save",
            Self::Complete { .. } => "milestones.progress.complete",
            Self::OpenIssues { .. } => "milestones.progress.open",
            Self::ClosedIssues { .. } => "milestones.progress.closed",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::ClosedWhen { when } => {
                text.literal("Closed ");
                text.argument(when);
            }
            Self::DueBy { due } => {
                text.literal("Due by ");
                text.argument(due);
            }
            Self::PastDueBy { due } => {
                text.literal("Past due by ");
                text.argument(due);
            }
            Self::DeleteConfirm { title } => {
                text.literal("Delete the milestone “");
                text.argument(title);
                text.literal("”? Its issues are kept.");
            }
            Self::Complete { percent } => {
                text.number(percent);
                text.literal("% complete");
            }
            Self::OpenIssues { count } => {
                text.number(count);
                text.literal(" open");
            }
            Self::ClosedIssues { count } => {
                text.number(count);
                text.literal(" closed");
            }
            Self::NewMilestone => text.literal("New milestone"),
            Self::EditMilestone => text.literal("Edit milestone"),
            Self::EmptyTitle => text.literal("No milestones here"),
            Self::EmptyText => text.literal("Milestones group issues towards a release or a date, and show how far along they are."),
            Self::NoDueDate => text.literal("No due date"),
            Self::Close => text.literal("Close"),
            Self::Reopen => text.literal("Reopen"),
            Self::SearchIssues => text.literal("Search these issues"),
            Self::NothingHereTitle => text.literal("Nothing here"),
            Self::NothingHereText => text.literal("Issues and pull requests added to this milestone appear here."),
            Self::FormIntro => text.literal("Group issues towards a release or a date."),
            Self::DueDate => text.literal("Due date"),
            Self::MarkdownOptional => text.literal("Markdown, optional"),
            Self::CreateMilestone => text.literal("Create milestone"),
            Self::SaveMilestone => text.literal("Save milestone"),
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
