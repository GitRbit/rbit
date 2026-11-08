//! The issue list, the new-issue form and the issue page around its conversation.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IssueMessage<'a> {
    Issues,
    Labels,
    Milestones,
    NewIssue,
    SearchIssues,
    /// The search box placeholder of the issue and pull request lists.
    SearchPlaceholder,
    Search,
    SearchHelp,
    ClearSearch,
    /// The accessible name of the Open / Closed tabs.
    StateNav,
    NoIssuesTitle,
    NoIssuesText,
    DescriptionHint,
    NoLabelsYet,
    Assignees,
    Milestone,
    NoMilestone,
    OpenIssue,
    Details,
    /// A list row's lock icon, for screen readers.
    Locked,
    /// `names` is the list of assignees.
    AssignedTo {
        names: &'a str,
    },
    Comments {
        count: u64,
    },
}

impl MessageFamily for IssueMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Issues => "issues.nav.issues",
            Self::Labels => "issues.nav.labels",
            Self::Milestones => "issues.nav.milestones",
            Self::NewIssue => "issues.new",
            Self::SearchIssues => "issues.search.label",
            Self::SearchPlaceholder => "issues.search.placeholder",
            Self::Search => "issues.search.submit",
            Self::SearchHelp => "issues.search.help",
            Self::ClearSearch => "issues.search.clear",
            Self::StateNav => "issues.list.state",
            Self::NoIssuesTitle => "issues.list.empty-title",
            Self::NoIssuesText => "issues.list.empty-text",
            Self::DescriptionHint => "issues.form.description-hint",
            Self::NoLabelsYet => "issues.form.no-labels",
            Self::Assignees => "issues.form.assignees",
            Self::Milestone => "issues.form.milestone",
            Self::NoMilestone => "issues.form.no-milestone",
            Self::OpenIssue => "issues.form.submit",
            Self::Details => "issues.page.details",
            Self::Locked => "issues.row.locked",
            Self::AssignedTo { .. } => "issues.row.assigned-to",
            Self::Comments { .. } => "issues.row.comments",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::AssignedTo { names } => {
                text.literal("Assigned to ");
                text.argument(names);
            }
            Self::Comments { count } => {
                text.number(count);
                text.literal(if count == 1 { " comment" } else { " comments" });
            }
            literal => text.literal(match literal {
                Self::Issues => "Issues",
                Self::Labels => "Labels",
                Self::Milestones => "Milestones",
                Self::NewIssue => "New issue",
                Self::SearchIssues => "Search issues",
                Self::SearchPlaceholder => "Search titles and descriptions…",
                Self::Search => "Search",
                Self::SearchHelp => "Search help",
                Self::ClearSearch => "Clear the search, filters and sort",
                Self::StateNav => "State",
                Self::NoIssuesTitle => "No issues match",
                Self::NoIssuesText => "Try another search, or open the first issue.",
                Self::DescriptionHint => "Markdown; #12 and @name link",
                Self::NoLabelsYet => "No labels yet.",
                Self::Assignees => "Assignees",
                Self::Milestone => "Milestone",
                Self::NoMilestone => "No milestone",
                Self::OpenIssue => "Open issue",
                Self::Details => "Details",
                Self::Locked => "locked",
                Self::AssignedTo { .. } | Self::Comments { .. } => "",
            }),
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
