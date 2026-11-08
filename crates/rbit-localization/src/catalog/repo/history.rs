//! A repository's history: commits, a commit, blame, comparisons, branches and tags,
//! the branch graph and the shared diff pieces.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the history pages and diffs.
    RepoHistoryLabel {
        BrowseFilesAtCommit => "repo.commits.browse-label", "Browse files at this commit";
        BrowseFiles => "repo.commits.browse", "Browse files";
        Pagination => "repo.commits.pagination", "Pagination";
        Newer => "repo.commits.newer", "Newer";
        Older => "repo.commits.older", "Older";
        Committed => "repo.commit.committed", "committed";
        CommittedBy => "repo.commit.committed-by", "committed by";
        Commit => "repo.commit.commit", "commit";
        Checks => "repo.commit.checks", "Checks";
        ViewFile => "repo.blame.view-file", "View file";
        PathInThatCommit => "repo.blame.source-path", "Path in that commit";
        FileEmpty => "repo.blame.empty", "This file is empty.";
        BinaryCannotBeBlamed => "repo.blame.binary", "Binary files cannot be blamed.";
        TooLargeToBlame => "repo.blame.too-large", "This file is too large to blame.";
        ViewTheFile => "repo.blame.view-the-file", "View the file";
        CompareChanges => "repo.compare.title", "Compare changes";
        CompareLead => "repo.compare.lead", "Pick a base and the branch, tag or commit to compare against it, also across forks.";
        Comparison => "repo.compare.label", "Comparison";
        MergeBase => "repo.compare.merge-base", "merge base";
        Swap => "repo.compare.swap", "Swap";
        CreatePullRequest => "repo.compare.create-pull", "Create pull request";
        UnrelatedHistories => "repo.compare.unrelated", "These revisions have entirely different histories, so there is no diff to show.";
        NothingToCompare => "repo.compare.nothing-title", "Nothing to compare";
        NothingToCompareText => "repo.compare.nothing", "Everything in the compared revision is already in the base.";
        Commits => "repo.compare.commits", "Commits";
        Compare => "repo.refs.compare", "Compare";
        CompareWithDefault => "repo.refs.compare-title", "Compare with the default branch";
        BranchView => "repo.refs.view-label", "Branch view";
        List => "repo.refs.list", "List";
        GitTree => "repo.refs.git-tree", "Git tree";
        NoMatchingRefs => "repo.refs.no-match-refs", "No matching refs.";
        NoBranchesYet => "repo.refs.no-branches-title", "No branches yet";
        NoBranchesText => "repo.refs.no-branches", "Push a commit to create the first branch.";
        NoTagsYet => "repo.refs.no-tags-title", "No tags yet";
        NoTagsText => "repo.refs.no-tags", "Tags mark releases and other important points in history.";
        Protected => "repo.refs.protected", "Protected";
        ProtectedTitle => "repo.refs.protected-title", "A branch protection rule applies to this branch";
        DownloadTarGz => "repo.refs.download-tar", "Download tar.gz";
        BranchCommitGraph => "repo.graph.label", "Branch commit graph";
        GraphHelp => "repo.graph.help", "Follow colored lines to trace history. Select a track to highlight its commits.";
        ClearHighlight => "repo.graph.clear", "Clear highlight";
        HistoryConnections => "repo.graph.connections", "History connections";
        CommitDetails => "repo.graph.details", "Commit details";
        Merge => "repo.graph.merge", "Merge";
        GraphTruncated => "repo.graph.truncated", "Showing the latest 200 commits across branches. Lines at the bottom continue into older history. Open a branch’s history to explore further.";
        LinesAdded => "repo.diff.lines-added", "Added";
        LinesDeleted => "repo.diff.lines-deleted", "Deleted";
        StatusAdded => "repo.diff.status-added", "Added";
        StatusDeleted => "repo.diff.status-deleted", "Deleted";
        StatusModified => "repo.diff.status-modified", "Modified";
        StatusRenamed => "repo.diff.status-renamed", "Renamed";
        StatusCopied => "repo.diff.status-copied", "Copied";
        ChangedFiles => "repo.diff.changed-files", "Changed files";
        FileTree => "repo.diff.file-tree", "File tree";
        FilterChangedFiles => "repo.diff.filter-label", "Filter changed files";
        FilterFiles => "repo.diff.filter", "Filter files…";
        NoFilesMatch => "repo.diff.no-match", "No files match.";
        Changes => "repo.diff.changes", "Changes";
        ToggleFileTree => "repo.diff.toggle-tree", "Toggle file tree";
        CollapseAll => "repo.diff.collapse-all", "Collapse all";
        ExpandAll => "repo.diff.expand-all", "Expand all";
        DiffTruncated => "repo.diff.truncated", "This change is very large, so some diffs are not shown. Clone the repository to see the full change.";
        CollapseHint => "repo.diff.collapse-hint", "Click to collapse or expand · Alt-click for every file";
        CopyPath => "repo.diff.copy-path", "Copy path";
        ViewFileAtCommit => "repo.diff.view-title", "View the file at this commit";
        View => "repo.diff.view", "View";
        BinaryNotShown => "repo.diff.binary", "Binary file not shown.";
        DiffTooLarge => "repo.diff.too-large", "Diff too large to display.";
        SubmoduleCommit => "repo.diff.submodule", "Submodule commit";
        NoContentChanges => "repo.diff.no-changes", "No content changes.";
    }
}

/// Counted or named text of the history pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoHistoryMessage<'a> {
    /// A day's commit list: "Commits on 3 May 2026".
    CommitsOn { date: &'a str },
    /// The title of a commit link: "View commit 1a2b3c…".
    ViewCommit { id: &'a str },
    /// The noun before a commit's parents: "parent" or "parents".
    ParentNoun { count: usize },
    /// The blame section's label: "Blame of main.rs".
    BlameOf { file: &'a str },
    /// The document title of a blame page: "Blame src/main.rs".
    BlameTitle { path: &'a str },
    /// Commits ahead of the base, `count` already formatted ("12", "1000+").
    Ahead { count: &'a str },
    /// Commits behind the base, `count` already formatted.
    Behind { count: &'a str },
    /// Under a cut-off commit list: "Only the newest 250 commits are listed."
    OnlyNewest { count: usize },
    /// The graph track button's label: "Highlight track 2".
    HighlightTrack { track: usize },
    /// The graph track button: "Track 2".
    Track { track: usize },
    /// A branch label's title: "Branch: main".
    BranchName { name: &'a str },
    /// Branches of a graph row not shown: "3 more branches".
    MoreBranches { count: usize },
    /// The noun after a changed-file count: "file changed" or "files changed".
    FilesChangedNoun { count: usize },
    /// A comparison's document title: "Compare main...feature".
    CompareTitle { base: &'a str, head: &'a str },
}

impl MessageFamily for RepoHistoryMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::CommitsOn { .. } => "repo.commits.day",
            Self::ViewCommit { .. } => "repo.commits.view",
            Self::ParentNoun { .. } => "repo.commit.parent-noun",
            Self::BlameOf { .. } => "repo.blame.label",
            Self::BlameTitle { .. } => "repo.blame.title",
            Self::Ahead { .. } => "repo.compare.ahead",
            Self::Behind { .. } => "repo.compare.behind",
            Self::OnlyNewest { .. } => "repo.compare.only-newest",
            Self::HighlightTrack { .. } => "repo.graph.highlight-track",
            Self::Track { .. } => "repo.graph.track",
            Self::BranchName { .. } => "repo.graph.branch",
            Self::MoreBranches { .. } => "repo.graph.more-branches",
            Self::FilesChangedNoun { .. } => "repo.diff.files-changed",
            Self::CompareTitle { .. } => "repo.compare.document-title",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::CommitsOn { date } => labelled(text, "Commits on ", date),
            Self::ViewCommit { id } => labelled(text, "View commit ", id),
            Self::ParentNoun { count } => text.literal(if count == 1 { "parent" } else { "parents" }),
            Self::BlameOf { file } => labelled(text, "Blame of ", file),
            Self::BlameTitle { path } => labelled(text, "Blame ", path),
            Self::Ahead { count } => {
                text.argument(count);
                text.literal(" ahead");
            }
            Self::Behind { count } => {
                text.argument(count);
                text.literal(" behind");
            }
            Self::OnlyNewest { count } => {
                text.literal("Only the newest ");
                text.number(count as u64);
                text.literal(if count == 1 { " commit is listed." } else { " commits are listed." });
            }
            Self::HighlightTrack { track } => {
                text.literal("Highlight track ");
                text.number(track as u64);
            }
            Self::Track { track } => {
                text.literal("Track ");
                text.number(track as u64);
            }
            Self::BranchName { name } => labelled(text, "Branch: ", name),
            Self::MoreBranches { count } => {
                text.number(count as u64);
                text.literal(if count == 1 { " more branch" } else { " more branches" });
            }
            Self::FilesChangedNoun { count } => text.literal(if count == 1 { "file changed" } else { "files changed" }),
            Self::CompareTitle { base, head } => {
                text.literal("Compare ");
                text.argument(base);
                text.literal("...");
                text.argument(head);
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// Fixed wording followed by one value.
fn labelled(text: &mut MessageWriter, wording: &str, value: &str) {
    text.literal(wording);
    text.argument(value);
}

lookup!(RepoHistoryMessage<'_>);
