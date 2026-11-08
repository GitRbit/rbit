//! The pull request list, the new pull request page with its base and head pickers, and
//! the header, tabs and commits of one pull request.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullMessage<'a> {
    PullRequests,
    SearchPulls,
    ClearSearch,
    FiltersLabel,
    StateNav,
    AuthorFilter {
        author: &'a str,
    },
    /// "New" on narrow screens; [`Self::NewSuffix`] follows on wider ones.
    New,
    NewSuffix,
    EmptyTitle,
    EmptyText,
    OpenTitle,
    OpenIntro,
    OpenAsDraft,
    Create,
    DifferentHistories,
    ChooseBranches,
    UpToDate {
        base: &'a str,
        head: &'a str,
    },
    BaseRepository,
    BaseRefs,
    Base,
    HeadRepository,
    Compare,
    CompareRefs,
    /// The accessible name of the tabs.
    PullRequest,
    Conversation,
    Commits,
    FilesChanged,
    /// The header's commit count; `None` while it is still being counted.
    CommitsPhrase {
        count: Option<u64>,
    },
    ChangeBase,
    BaseBranch,
    BaseBranches,
    ChangeBaseHint,
    UpdateBase,
    IsClosed,
    IsMerged,
    DeleteBranch {
        branch: &'a str,
    },
    ConvertToDraft,
    NoCommitsTitle,
    NoCommitsText,
    NewestCommits {
        count: u64,
    },
}

impl MessageFamily for PullMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::PullRequests => "pulls.title",
            Self::SearchPulls => "pulls.search.label",
            Self::ClearSearch => "pulls.search.clear",
            Self::FiltersLabel => "pulls.list.filters",
            Self::StateNav => "pulls.list.state",
            Self::AuthorFilter { .. } => "pulls.list.author-filter",
            Self::New => "pulls.list.new",
            Self::NewSuffix => "pulls.list.new-suffix",
            Self::EmptyTitle => "pulls.list.empty-title",
            Self::EmptyText => "pulls.list.empty-text",
            Self::OpenTitle => "pulls.new.title",
            Self::OpenIntro => "pulls.new.intro",
            Self::OpenAsDraft => "pulls.new.draft",
            Self::Create => "pulls.new.submit",
            Self::DifferentHistories => "pulls.new.different-histories",
            Self::ChooseBranches => "pulls.new.choose-branches",
            Self::UpToDate { .. } => "pulls.new.up-to-date",
            Self::BaseRepository => "pulls.picker.base-repository",
            Self::BaseRefs => "pulls.picker.base-refs",
            Self::Base => "pulls.picker.base",
            Self::HeadRepository => "pulls.picker.head-repository",
            Self::Compare => "pulls.picker.compare",
            Self::CompareRefs => "pulls.picker.compare-refs",
            Self::PullRequest => "pulls.tabs.label",
            Self::Conversation => "pulls.tabs.conversation",
            Self::Commits => "pulls.tabs.commits",
            Self::FilesChanged => "pulls.tabs.files",
            Self::CommitsPhrase { .. } => "pulls.header.commits",
            Self::ChangeBase => "pulls.base.change",
            Self::BaseBranch => "pulls.base.label",
            Self::BaseBranches => "pulls.base.options",
            Self::ChangeBaseHint => "pulls.base.hint",
            Self::UpdateBase => "pulls.base.submit",
            Self::IsClosed => "pulls.closed",
            Self::IsMerged => "pulls.merged",
            Self::DeleteBranch { .. } => "pulls.delete-branch",
            Self::ConvertToDraft => "pulls.convert-to-draft",
            Self::NoCommitsTitle => "pulls.commits.empty-title",
            Self::NoCommitsText => "pulls.commits.empty-text",
            Self::NewestCommits { .. } => "pulls.commits.newest",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::AuthorFilter { author } => {
                text.literal("author: ");
                text.argument(author);
            }
            Self::UpToDate { base, head } => {
                text.argument(base);
                text.literal(" is up to date with all commits from ");
                text.argument(head);
                text.literal(".");
            }
            Self::CommitsPhrase { count: Some(count) } => {
                text.number(count);
                text.literal(if count == 1 { " commit" } else { " commits" });
            }
            Self::CommitsPhrase { count: None } => text.literal("commits"),
            Self::DeleteBranch { branch } => {
                text.literal("Delete ");
                text.argument(branch);
            }
            Self::NewestCommits { count } => {
                text.literal("Only the newest ");
                text.number(count);
                text.literal(if count == 1 { " commit is listed." } else { " commits are listed." });
            }
            literal => text.literal(match literal {
                Self::PullRequests => "Pull requests",
                Self::SearchPulls => "Search pull requests",
                Self::ClearSearch => "Clear search and filters",
                Self::FiltersLabel => "Pull request filters",
                Self::StateNav => "Pull request state",
                Self::New => "New",
                Self::NewSuffix => "pull request",
                Self::EmptyTitle => "No pull requests here",
                Self::EmptyText => "Pull requests propose changes from a branch or a fork. Compare branches to open one.",
                Self::OpenTitle => "Open a pull request",
                Self::OpenIntro => "Propose the changes of a branch, from this repository or a fork, for the base branch.",
                Self::OpenAsDraft => "Open as a draft",
                Self::Create => "Create pull request",
                Self::DifferentHistories => "These branches have entirely different histories.",
                Self::ChooseBranches => "Choose two existing branches to compare.",
                Self::BaseRepository => "Base repository",
                Self::BaseRefs => "Base branches and tags",
                Self::Base => "Base",
                Self::HeadRepository => "Head repository",
                Self::Compare => "Compare",
                Self::CompareRefs => "Compare branches and tags",
                Self::PullRequest => "Pull request",
                Self::Conversation => "Conversation",
                Self::Commits => "Commits",
                Self::FilesChanged => "Files changed",
                Self::ChangeBase => "Change base branch",
                Self::BaseBranch => "Base branch",
                Self::BaseBranches => "Base branches",
                Self::ChangeBaseHint => "Changing the base updates the commits, diff and merge checks.",
                Self::UpdateBase => "Update base branch",
                Self::IsClosed => "This pull request is closed.",
                Self::IsMerged => "Pull request merged.",
                Self::ConvertToDraft => "Convert to draft",
                Self::NoCommitsTitle => "No commits",
                Self::NoCommitsText => "The head and the base point at the same commit.",
                Self::AuthorFilter { .. }
                | Self::UpToDate { .. }
                | Self::CommitsPhrase { .. }
                | Self::DeleteBranch { .. }
                | Self::NewestCommits { .. } => "",
            }),
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
