//! The repository header: tabs, star, fork and clone controls, and the notes above them.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the repository header (`repo/subnav.html`, `repo/star_button.html`).
    RepoNavLabel {
        ArchivedNote => "repo.nav.archived-note", "This repository has been archived. It is read-only: you can clone, fork and star it, but not change it.";
        Private => "repo.nav.private", "Private";
        Public => "repo.nav.public", "Public";
        Repository => "repo.nav.label", "Repository";
        Code => "repo.nav.code", "Code";
        Issues => "repo.nav.issues", "Issues";
        PullRequests => "repo.nav.pull-requests", "Pull requests";
        Commits => "repo.nav.commits", "Commits";
        Branches => "repo.nav.branches", "Branches";
        Tags => "repo.nav.tags", "Tags";
        Releases => "repo.nav.releases", "Releases";
        Activity => "repo.nav.activity", "Activity";
        Actions => "repo.nav.actions", "Actions";
        Settings => "repo.nav.settings", "Settings";
        ForkThisRepository => "repo.nav.fork-title", "Fork this repository";
        Fork => "repo.nav.fork", "Fork";
        Forks => "repo.nav.forks", "Forks";
        Clone => "repo.nav.clone", "Clone";
        CloneUrl => "repo.nav.clone-url", "Clone URL";
        CopyCloneUrl => "repo.nav.copy-clone-url", "Copy clone URL";
        SshCloneUrl => "repo.nav.ssh-clone-url", "SSH clone URL";
        CopySshCloneUrl => "repo.nav.copy-ssh-clone-url", "Copy SSH clone URL";
        Copy => "repo.nav.copy", "Copy";
        YourSettingsEnd => "repo.nav.your-settings-end", ".";
        YourSettings => "repo.nav.your-settings", "your settings";
        DownloadZip => "repo.nav.download-zip", "Download ZIP";
        Star => "repo.star.star", "Star";
        Starred => "repo.star.starred", "Starred";
        StarThisRepository => "repo.star.star-title", "Star this repository";
        UnstarThisRepository => "repo.star.unstar-title", "Unstar this repository";
        SignInToStar => "repo.star.sign-in", "Sign in to star this repository";
        Stargazers => "repo.star.stargazers", "Stargazers";
    }
}

/// Counted or named text of the repository header and its lists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoNavMessage {
    /// The fork button's accessible count: "3 forks".
    ForkCount { count: u64 },
    /// The star button's accessible count: "1 stargazer".
    StargazerCount { count: i64 },
}

impl MessageFamily for RepoNavMessage {
    fn id(self) -> &'static str {
        match self {
            Self::ForkCount { .. } => "repo.nav.fork-count",
            Self::StargazerCount { .. } => "repo.star.stargazer-count",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::ForkCount { count } => {
                text.number(count);
                text.literal(if count == 1 { " fork" } else { " forks" });
            }
            Self::StargazerCount { count } => {
                text.number(count.unsigned_abs());
                text.literal(if count == 1 { " stargazer" } else { " stargazers" });
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

lookup!(RepoNavMessage);

/// Repository header sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoNavSentence {
    /// Above the tabs of a fork. Slot 0: the parent repository link.
    ForkedFrom,
    /// Above the tabs of a mirror. Slot 0: the upstream URL.
    MirrorOf,
    /// Under the SSH clone URL. Slot 0: the settings link, worded by `RepoNavLabel::YourSettings`.
    SshKeyNote,
}

impl RepoNavSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<crate::Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for RepoNavSentence {
    fn id(self) -> &'static str {
        match self {
            Self::ForkedFrom => "repo.nav.forked-from",
            Self::MirrorOf => "repo.nav.mirror-of",
            Self::SshKeyNote => "repo.nav.ssh-key-note",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::ForkedFrom => {
                text.literal("forked from ");
                text.slot(0);
            }
            Self::MirrorOf => {
                text.literal("mirror of ");
                text.slot(0);
            }
            Self::SshKeyNote => {
                text.literal("SSH needs a key in ");
                text.slot(0);
                text.literal(".");
            }
        }
    }

    fn translate(self, locale: crate::Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
