//! Creating repositories: empty, mirrored or imported, and the progress pages that follow.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the new-repository, mirror and import pages.
    RepoCreateLabel {
        NewRepository => "repo.create.title", "New repository";
        RepositoryType => "repo.create.type", "Repository type";
        Empty => "repo.create.tab-empty", "Empty";
        Mirror => "repo.create.tab-mirror", "Mirror";
        Import => "repo.create.tab-import", "Import";
        NewLead => "repo.create.lead", "Create an empty repository, import a project, or keep a mirror in sync with its source.";
        Owner => "repo.create.owner", "Owner";
        Name => "repo.create.name", "Name";
        Description => "repo.create.description", "Description";
        Optional => "repo.create.optional", "optional";
        Visibility => "repo.create.visibility", "Visibility";
        Public => "repo.create.public", "Public";
        Private => "repo.create.private", "Private";
        PublicRepository => "repo.create.public-hint", "Anyone can see and clone this repository.";
        PublicMirror => "repo.create.public-mirror-hint", "Anyone can see and clone the mirror.";
        PublicImport => "repo.create.public-import-hint", "Anyone can see and clone the repository.";
        PrivateHint => "repo.create.private-hint", "Only the owner can see it. For an organization, that means its members.";
        Cancel => "repo.create.cancel", "Cancel";
        CreateRepository => "repo.create.submit", "Create repository";
        MirrorLead => "repo.mirror.lead", "A mirror fetches the branches and tags of a public repository elsewhere on a schedule. It is read-only here: changes come from upstream.";
        MirrorsDisabled => "repo.mirror.disabled-title", "Mirrors are disabled";
        MirrorsDisabledText => "repo.mirror.disabled", "An administrator of this instance has turned pull mirrors off.";
        UpstreamUrl => "repo.mirror.url", "Upstream URL";
        UrlHintStart => "repo.mirror.url-hint-start", "An";
        UrlHintOr => "repo.mirror.url-hint-or", "or";
        UrlHintEnd => "repo.mirror.url-hint-end", "URL of a public repository.";
        FromTheUrl => "repo.mirror.name-placeholder", "from the URL";
        FetchEvery => "repo.mirror.interval", "Fetch every";
        Minutes => "repo.mirror.minutes", "minutes";
        CreateMirror => "repo.mirror.submit", "Create mirror";
        MirrorARepository => "repo.mirror.document-title", "Mirror a repository";
        ImportLead => "repo.import.lead", "Copies a repository from GitHub or Gitea with its history and, if you like, its issues, pull requests, labels, milestones, releases and Git LFS files. Issue and pull request numbers stay the same.";
        ImportingDisabled => "repo.import.disabled-title", "Importing is disabled";
        ImportingDisabledText => "repo.import.disabled", "An administrator of this instance has turned importing off.";
        ImportFrom => "repo.import.from", "Import from";
        ServerAddress => "repo.import.host", "Server address";
        NotNeededForGitHub => "repo.import.host-hint", "not needed for github.com";
        Repository => "repo.import.repository", "Repository";
        OwnerNameOrUrl => "repo.import.repository-placeholder", "owner/name or its URL";
        AccessToken => "repo.import.token", "Access token";
        OptionalForPublic => "repo.import.token-optional", "optional for public repositories";
        TokenHint => "repo.import.token-hint", "A token that can read the repository raises the source's rate limits and is needed for private repositories. It is used only while the import runs and is never stored.";
        AsOnTheSource => "repo.import.name-placeholder", "as on the source";
        BringAlong => "repo.import.options", "Bring along";
        Wiki => "repo.import.wiki", "Wiki";
        WikiNotSupported => "repo.import.wiki-hint", "Not supported: this instance has no wikis.";
        StartImport => "repo.import.submit", "Start import";
        ImportARepository => "repo.import.document-title", "Import a repository";
        ImportTitle => "repo.import.status-title", "Import";
        FailedNote => "repo.import.failed-note", "Everything imported until then is in the repository.";
        NotReached => "repo.import.not-reached", "Not reached";
        Started => "repo.import.started", "Started";
        WaitingForSlot => "repo.import.waiting", "Waiting for a free slot";
        Ended => "repo.import.ended", "Ended";
        UpdatesByItself => "repo.import.live", "This page updates by itself.";
        SkippedOrIncomplete => "repo.import.warnings", "Skipped or incomplete";
        GoToRepository => "repo.import.done", "Go to the repository";
        OpensAutomatically => "repo.mirror.progress-lead", "The repository will open automatically when its first mirror fetch completes.";
        FetchInProgress => "repo.mirror.progress-label", "Mirror fetch in progress";
        TransferSizeNote => "repo.mirror.progress-note", "Transfer size is determined by the upstream. This page shows the current stage as work progresses.";
        RefreshStatus => "repo.mirror.refresh", "Refresh status";
        SettingsAndRetry => "repo.mirror.settings-retry", "Mirror settings and retry";
        NoScriptHint => "repo.mirror.noscript", "Use Refresh status to check progress and open the repository when it is ready.";
        Connecting => "repo.mirror.connecting", "Connecting to the upstream repository.";
        Fetching => "repo.mirror.fetching", "Downloading branches, tags and Git objects. Large repositories may take several minutes.";
        Finalizing => "repo.mirror.finalizing", "Preparing repository content and the default branch.";
        FetchFailedRetry => "repo.mirror.failed", "The mirror could not be fetched. A repository administrator can retry from mirror settings.";
        Complete => "repo.mirror.complete", "Mirroring complete. Opening the repository…";
        Queued => "repo.mirror.queued", "Waiting for an available mirror worker.";
        Stopped => "repo.mirror.stopped", "Mirroring has stopped. Opening the repository…";
        Mirroring => "repo.mirror.document-title-progress", "Mirroring";
    }
}

/// Counted or named text of the creation and progress pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoCreateMessage<'a> {
    /// Under the fetch interval: "At least 10 minutes."
    AtLeastMinutes { minutes: u64 },
    /// Where an import comes from: "From GitHub:" before the source's link.
    ImportFrom { source: &'a str },
    /// Warnings left out of the list: "and 3 more (see the server log)".
    MoreWarnings { count: u64 },
    /// The progress page's heading: "Mirroring project".
    MirroringName { name: &'a str },
    /// A failed fetch with the upstream's error.
    FetchFailed { error: &'a str },
}

impl MessageFamily for RepoCreateMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::AtLeastMinutes { .. } => "repo.mirror.min-interval",
            Self::ImportFrom { .. } => "repo.import.source",
            Self::MoreWarnings { .. } => "repo.import.more-warnings",
            Self::MirroringName { .. } => "repo.mirror.progress-title",
            Self::FetchFailed { .. } => "repo.mirror.failed-with",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::AtLeastMinutes { minutes } => {
                text.literal("At least ");
                text.number(minutes);
                text.literal(if minutes == 1 { " minute." } else { " minutes." });
            }
            Self::ImportFrom { source } => {
                text.literal("From ");
                text.argument(source);
                text.literal(":");
            }
            Self::MoreWarnings { count } => {
                text.literal("and ");
                text.number(count);
                text.literal(" more (see the server log)");
            }
            Self::MirroringName { name } => {
                text.literal("Mirroring ");
                text.argument(name);
            }
            Self::FetchFailed { error } => {
                text.literal("The mirror could not be fetched: ");
                text.argument(error);
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

lookup!(RepoCreateMessage<'_>);
