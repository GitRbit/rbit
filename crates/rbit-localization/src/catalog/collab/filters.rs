//! The filter menus above the issue and pull request lists.
use super::super::fallback::{MessageFamily, MessageWriter};

/// One filter menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IssueFilterMenu {
    State,
    Status,
    Author,
    Label,
    Milestone,
    Assignee,
    Sort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IssueFilterMessage {
    /// The menu's name.
    Menu(IssueFilterMenu),
    /// The accessible name of the menu's search box.
    Find(IssueFilterMenu),
    /// The menu's search box placeholder.
    FindPlaceholder(IssueFilterMenu),
    NothingToChoose,
    NoMatches,
    Anyone,
    Nobody,
    You,
    Any,
    NoMilestone,
    AnyLabel,
    NoLabels,
    Open,
    Closed,
    AllStates,
    AnyStatus,
    Draft,
    Merged,
    SortNewest,
    SortOldest,
    SortRecentlyUpdated,
    SortLeastRecentlyUpdated,
    SortMostCommented,
}

impl IssueFilterMenu {
    /// The ids of its name, search label and placeholder; its English name and noun.
    const fn entries(self) -> [&'static str; 5] {
        match self {
            Self::State => ["issues.filter.state", "issues.filter.state.find", "issues.filter.state.placeholder", "State", "state"],
            Self::Status => ["issues.filter.status", "issues.filter.status.find", "issues.filter.status.placeholder", "Status", "status"],
            Self::Author => ["issues.filter.author", "issues.filter.author.find", "issues.filter.author.placeholder", "Author", "author"],
            Self::Label => ["issues.filter.label", "issues.filter.label.find", "issues.filter.label.placeholder", "Label", "label"],
            Self::Milestone => {
                ["issues.filter.milestone", "issues.filter.milestone.find", "issues.filter.milestone.placeholder", "Milestone", "milestone"]
            }
            Self::Assignee => {
                ["issues.filter.assignee", "issues.filter.assignee.find", "issues.filter.assignee.placeholder", "Assignee", "assignee"]
            }
            Self::Sort => ["issues.filter.sort", "issues.filter.sort.find", "issues.filter.sort.placeholder", "Sort", "sort"],
        }
    }
}

impl MessageFamily for IssueFilterMessage {
    fn id(self) -> &'static str {
        match self {
            Self::Menu(menu) => menu.entries()[0],
            Self::Find(menu) => menu.entries()[1],
            Self::FindPlaceholder(menu) => menu.entries()[2],
            Self::NothingToChoose => "issues.filter.nothing",
            Self::NoMatches => "issues.filter.no-matches",
            Self::Anyone => "issues.filter.anyone",
            Self::Nobody => "issues.filter.nobody",
            Self::You => "issues.filter.you",
            Self::Any => "issues.filter.any",
            Self::NoMilestone => "issues.filter.no-milestone",
            Self::AnyLabel => "issues.filter.any-label",
            Self::NoLabels => "issues.filter.no-labels",
            Self::Open => "issues.filter.open",
            Self::Closed => "issues.filter.closed",
            Self::AllStates => "issues.filter.all-states",
            Self::AnyStatus => "issues.filter.any-status",
            Self::Draft => "issues.filter.draft",
            Self::Merged => "issues.filter.merged",
            Self::SortNewest => "issues.filter.sort-newest",
            Self::SortOldest => "issues.filter.sort-oldest",
            Self::SortRecentlyUpdated => "issues.filter.sort-recently-updated",
            Self::SortLeastRecentlyUpdated => "issues.filter.sort-least-recently-updated",
            Self::SortMostCommented => "issues.filter.sort-most-commented",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::Menu(menu) => text.literal(menu.entries()[3]),
            Self::Find(menu) => {
                text.literal("Find ");
                text.literal(menu.entries()[4]);
            }
            Self::FindPlaceholder(menu) => {
                text.literal("Find ");
                text.literal(menu.entries()[4]);
                text.literal("…");
            }
            literal => text.literal(match literal {
                Self::NothingToChoose => "Nothing to choose from yet.",
                Self::NoMatches => "No matches.",
                Self::Anyone => "Anyone",
                Self::Nobody => "Nobody",
                Self::You => "You",
                Self::Any => "Any",
                Self::NoMilestone => "No milestone",
                Self::AnyLabel => "Any label",
                Self::NoLabels => "No labels",
                Self::Open => "Open",
                Self::Closed => "Closed",
                Self::AllStates => "All states",
                Self::AnyStatus => "Any status",
                Self::Draft => "Draft",
                Self::Merged => "Merged",
                Self::SortNewest => "Newest",
                Self::SortOldest => "Oldest",
                Self::SortRecentlyUpdated => "Recently updated",
                Self::SortLeastRecentlyUpdated => "Least recently updated",
                Self::SortMostCommented => "Most commented",
                Self::Menu(_) | Self::Find(_) | Self::FindPlaceholder(_) => "",
            }),
        }
    }
}
