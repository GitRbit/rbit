//! Browsing a repository: the file tree, file pages, documents, the ref picker, forks
//! and stargazers.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the repository's browsing pages.
    RepoBrowseLabel {
        About => "repo.browse.about", "About";
        NoDescription => "repo.browse.no-description", "No description.";
        Visibility => "repo.browse.visibility", "Visibility";
        History => "repo.browse.history", "History";
        CommitHistory => "repo.browse.commit-history", "Commit history";
        AddFile => "repo.browse.add-file", "Add file";
        UploadFiles => "repo.browse.upload-files", "Upload files";
        Files => "repo.browse.files", "Files";
        ParentDirectory => "repo.browse.parent-directory", "Parent directory";
        Path => "repo.browse.path", "Path";
        Latest => "repo.browse.latest", "Latest";
        YourAccess => "repo.browse.your-access", "Your access";
        LeaveRepository => "repo.browse.leave", "Leave repository";
        Languages => "repo.browse.languages", "Languages";
        RepositoryLinks => "repo.browse.links", "Repository links";
        StoredWithLfs => "repo.blob.lfs", "Stored with Git LFS";
        LfsObjectMissing => "repo.blob.lfs-missing", "Object not on this server";
        EditThisFile => "repo.blob.edit", "Edit this file";
        DeleteThisFile => "repo.blob.delete", "Delete this file";
        Blame => "repo.blob.blame", "Blame";
        Raw => "repo.blob.raw", "Raw";
        Download => "repo.blob.download", "Download";
        BinaryNotShown => "repo.blob.binary", "Binary file not shown.";
        TooLargeToDisplay => "repo.blob.too-large", "This file is too large to display.";
        ViewRaw => "repo.blob.view-raw", "View raw";
        RepositoryDocuments => "repo.documents.label", "Repository documents";
        DocumentOutline => "repo.documents.outline", "Document outline";
        FilterHeadings => "repo.documents.filter", "Filter headings";
        DocumentHeadings => "repo.documents.headings", "Document headings";
        NoMatchingHeadings => "repo.documents.no-match", "No matching headings.";
        EmptyTitle => "repo.empty.title", "This repository is empty";
        EmptyLead => "repo.empty.lead", "Push an existing project, or start a new one from the command line.";
        RepositoryUrl => "repo.empty.url", "Repository URL";
        CopyRepositoryUrl => "repo.empty.copy-url", "Copy repository URL";
        SshRepositoryUrl => "repo.empty.ssh-url", "SSH repository URL";
        CopySshRepositoryUrl => "repo.empty.copy-ssh-url", "Copy SSH repository URL";
        EmptySshNote => "repo.empty.ssh-note", "The commands below use HTTPS. To push over SSH, use the SSH URL instead and add a key in ";
        CreateNewRepository => "repo.empty.create", "Create a new repository";
        PushExistingRepository => "repo.empty.push", "Push an existing repository";
        CloseFileBrowser => "repo.languages.close", "Close file browser";
        LanguageFiles => "repo.languages.files", "Language files";
        NoFilesForLanguage => "repo.languages.none", "No files found for this language.";
        NoMatchingFiles => "repo.languages.no-match", "No matching files.";
        FileContent => "repo.languages.content", "File content";
        SelectFileToPreview => "repo.languages.select", "Select a file to preview its content.";
        SearchThisListing => "repo.listing.search-label", "Search this listing";
        SearchNames => "repo.listing.search-placeholder", "Search names";
        Search => "repo.listing.search", "Search";
        ListingPages => "repo.listing.pages", "Listing pages";
        Previous => "repo.listing.previous", "Previous";
        Next => "repo.listing.next", "Next";
        BranchesAndTags => "repo.refs.picker-title", "Branches and tags";
        BrowseBranchesAndTags => "repo.refs.browse", "Browse branches and tags";
        FindBranchOrTag => "repo.refs.find-label", "Find a branch or tag";
        Find => "repo.refs.find", "Find";
        Branch => "repo.refs.branch", "Branch";
        Tag => "repo.refs.tag", "Tag";
        Default => "repo.refs.default", "default";
        NoMatchingRefs => "repo.refs.no-match", "No matching branches or tags.";
        NoForksYet => "repo.forks.empty-title", "No forks yet";
        ForksEmpty => "repo.forks.empty", "Forks are copies of this repository that others work on independently.";
        ForkLead => "repo.fork.lead", "A fork is your own copy of the repository's branches and tags. It keeps the original's visibility.";
        ForkLeadPrivate => "repo.fork.lead-private", "A fork is your own copy of the repository's branches and tags. It keeps the original's visibility — this one stays private.";
        Owner => "repo.fork.owner", "Owner";
        Name => "repo.fork.name", "Name";
        CreateFork => "repo.fork.create", "Create fork";
        Cancel => "repo.fork.cancel", "Cancel";
        NoStargazersYet => "repo.stargazers.empty-title", "No stargazers yet";
        StargazersEmpty => "repo.stargazers.empty", "People who star this repository appear here.";
        RepositoryActivity => "repo.activity.title", "Repository activity";
        GoToFile => "repo.finder.go-to-file", "Go to file";
        MatchingFiles => "repo.finder.matching", "Matching files";
        LoadingFiles => "repo.finder.loading", "Loading files…";
        UnableToLoadFiles => "repo.finder.failed", "Unable to load files. Press Enter to search.";
        SeeAllMatches => "repo.finder.see-all", "See all matches";
        NoFilesInRevision => "repo.finder.empty", "This revision has no files.";
        TooManyFilesToSearch => "repo.finder.truncated", "This revision has too many files to search them all.";
    }
}

/// Counted or named text of the browsing pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoBrowseMessage<'a> {
    /// The noun after a branch count shown on its own: "branch" or "branches".
    BranchNoun { count: usize },
    /// The noun after a tag count shown on its own: "tag" or "tags".
    TagNoun { count: usize },
    /// The viewer's access, `permission` being the role's name: "Write access as a collaborator."
    CollaboratorAccess { permission: &'a str },
    /// A text file's length: "12 lines".
    LineCount { count: usize },
    /// The heading of a language's file list: "Rust files".
    LanguageFiles { language: &'a str },
    /// A listing's result count and page: "40 results · 1 / 2".
    ListingResults { total: usize, page: usize, pages: usize },
    /// The ref picker's page and count: "1 / 3 · 52 refs".
    RefPage { page: usize, pages: usize, total: usize },
    /// The fork page's heading: "Fork alice/project".
    ForkTitle { owner: &'a str, name: &'a str },
    /// The noun after a commit count beside the latest commit: "commit" or "commits".
    CommitNoun { count: usize },
    /// "Go to file" found nothing: "No file matches “query”."
    NoFileMatches { query: &'a str },
    /// Matches beyond those listed: "12 more files match. Type more of the name to narrow the list."
    MoreFileMatches { count: usize },
}

impl MessageFamily for RepoBrowseMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::BranchNoun { .. } => "repo.browse.branch-noun",
            Self::TagNoun { .. } => "repo.browse.tag-noun",
            Self::CollaboratorAccess { .. } => "repo.browse.collaborator-access",
            Self::LineCount { .. } => "repo.blob.line-count",
            Self::LanguageFiles { .. } => "repo.languages.title",
            Self::ListingResults { .. } => "repo.listing.results",
            Self::RefPage { .. } => "repo.refs.page",
            Self::ForkTitle { .. } => "repo.fork.title",
            Self::CommitNoun { .. } => "repo.browse.commit-noun",
            Self::NoFileMatches { .. } => "repo.finder.no-match",
            Self::MoreFileMatches { .. } => "repo.finder.more",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::BranchNoun { count } => text.literal(if count == 1 { "branch" } else { "branches" }),
            Self::TagNoun { count } => text.literal(if count == 1 { "tag" } else { "tags" }),
            Self::CollaboratorAccess { permission } => {
                text.argument(permission);
                text.literal(" access as a collaborator.");
            }
            Self::LineCount { count } => {
                text.number(count as u64);
                text.literal(if count == 1 { " line" } else { " lines" });
            }
            Self::LanguageFiles { language } => {
                text.argument(language);
                text.literal(" files");
            }
            Self::ListingResults { total, page, pages } => {
                text.number(total as u64);
                text.literal(if total == 1 { " result · " } else { " results · " });
                text.number(page as u64);
                text.literal(" / ");
                text.number(pages as u64);
            }
            Self::RefPage { page, pages, total } => {
                text.number(page as u64);
                text.literal(" / ");
                text.number(pages as u64);
                text.literal(" · ");
                text.number(total as u64);
                text.literal(if total == 1 { " ref" } else { " refs" });
            }
            Self::ForkTitle { owner, name } => {
                text.literal("Fork ");
                text.argument(owner);
                text.literal("/");
                text.argument(name);
            }
            Self::CommitNoun { count } => text.literal(if count == 1 { "commit" } else { "commits" }),
            Self::NoFileMatches { query } => {
                text.literal("No file matches “");
                text.argument(query);
                text.literal("”.");
            }
            Self::MoreFileMatches { count } => {
                text.number(count as u64);
                text.literal(if count == 1 { " more file matches." } else { " more files match." });
                text.literal(" Type more of the name to narrow the list.");
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

lookup!(RepoBrowseMessage<'_>);
