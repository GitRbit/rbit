//! German (`de`) entries for `catalog/repo/nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};
use crate::plural::Plural;

entries! { De:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ForkedFrom => text.literal("geforkt von"),
        RepoNavLabel::MirrorOf => text.literal("Spiegel von"),
        RepoNavLabel::ArchivedNote => text.literal(
            "Dieses Repository wurde archiviert. Es ist schreibgeschützt: Sie können es klonen, forken und mit \
             einem Stern markieren, aber nicht ändern.",
        ),
        RepoNavLabel::Private => text.literal("Privat"),
        RepoNavLabel::Public => text.literal("Öffentlich"),
        RepoNavLabel::Repository => text.literal("Repository"),
        RepoNavLabel::Code => text.literal("Code"),
        RepoNavLabel::Issues => text.literal("Issues"),
        RepoNavLabel::PullRequests => text.literal("Pull-Requests"),
        RepoNavLabel::Commits => text.literal("Commits"),
        RepoNavLabel::Branches => text.literal("Branches"),
        RepoNavLabel::Tags => text.literal("Tags"),
        RepoNavLabel::Releases => text.literal("Releases"),
        RepoNavLabel::Activity => text.literal("Aktivität"),
        RepoNavLabel::Actions => text.literal("Actions"),
        RepoNavLabel::Settings => text.literal("Einstellungen"),
        RepoNavLabel::ForkThisRepository => text.literal("Dieses Repository forken"),
        RepoNavLabel::Fork => text.literal("Forken"),
        RepoNavLabel::Forks => text.literal("Forks"),
        RepoNavLabel::Clone => text.literal("Klonen"),
        RepoNavLabel::CloneUrl => text.literal("Klon-URL"),
        RepoNavLabel::CopyCloneUrl => text.literal("Klon-URL kopieren"),
        RepoNavLabel::SshCloneUrl => text.literal("SSH-Klon-URL"),
        RepoNavLabel::CopySshCloneUrl => text.literal("SSH-Klon-URL kopieren"),
        RepoNavLabel::Copy => text.literal("Kopieren"),
        RepoNavLabel::SshKeyNote => text.literal("Für SSH ist ein Schlüssel nötig in"),
        RepoNavLabel::YourSettings => text.literal("Ihren Einstellungen"),
        RepoNavLabel::DownloadZip => text.literal("ZIP herunterladen"),
        RepoNavLabel::Star => text.literal("Stern"),
        RepoNavLabel::Starred => text.literal("Markiert"),
        RepoNavLabel::StarThisRepository => text.literal("Dieses Repository mit einem Stern markieren"),
        RepoNavLabel::UnstarThisRepository => text.literal("Stern von diesem Repository entfernen"),
        RepoNavLabel::SignInToStar => {
            text.literal("Melden Sie sich an, um dieses Repository mit einem Stern zu markieren");
        }
        RepoNavLabel::Stargazers => text.literal("Sterne"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Fork",
                _ => " Forks",
            });
        }
        RepoNavMessage::StargazerCount { count } => {
            text.number(count.unsigned_abs());
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " Stern",
                _ => " Sterne",
            });
        }
    }
}
