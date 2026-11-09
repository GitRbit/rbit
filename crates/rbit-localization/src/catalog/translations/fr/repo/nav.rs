//! French (`fr`) entries for `catalog/repo/nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};
use crate::plural::Plural;

entries! { Fr:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ForkedFrom => text.literal("fork de"),
        RepoNavLabel::MirrorOf => text.literal("miroir de"),
        RepoNavLabel::ArchivedNote => text.literal(
            "Ce dépôt a été archivé. Il est en lecture seule\u{a0}: vous pouvez le cloner, en créer un fork et lui \
             ajouter une étoile, mais pas le modifier.",
        ),
        RepoNavLabel::Private => text.literal("Privé"),
        RepoNavLabel::Public => text.literal("Public"),
        RepoNavLabel::Repository => text.literal("Dépôt"),
        RepoNavLabel::Code => text.literal("Code"),
        RepoNavLabel::Issues => text.literal("Tickets"),
        RepoNavLabel::PullRequests => text.literal("Pull requests"),
        RepoNavLabel::Commits => text.literal("Commits"),
        RepoNavLabel::Branches => text.literal("Branches"),
        RepoNavLabel::Tags => text.literal("Tags"),
        RepoNavLabel::Releases => text.literal("Releases"),
        RepoNavLabel::Activity => text.literal("Activité"),
        RepoNavLabel::Actions => text.literal("Actions"),
        RepoNavLabel::Settings => text.literal("Paramètres"),
        RepoNavLabel::ForkThisRepository => text.literal("Créer un fork de ce dépôt"),
        RepoNavLabel::Fork => text.literal("Fork"),
        RepoNavLabel::Forks => text.literal("Forks"),
        RepoNavLabel::Clone => text.literal("Cloner"),
        RepoNavLabel::CloneUrl => text.literal("URL de clonage"),
        RepoNavLabel::CopyCloneUrl => text.literal("Copier l’URL de clonage"),
        RepoNavLabel::SshCloneUrl => text.literal("URL de clonage SSH"),
        RepoNavLabel::CopySshCloneUrl => text.literal("Copier l’URL de clonage SSH"),
        RepoNavLabel::Copy => text.literal("Copier"),
        RepoNavLabel::SshKeyNote => text.literal("SSH nécessite une clé dans"),
        RepoNavLabel::YourSettings => text.literal("vos paramètres"),
        RepoNavLabel::DownloadZip => text.literal("Télécharger le ZIP"),
        RepoNavLabel::Star => text.literal("Ajouter une étoile"),
        RepoNavLabel::Starred => text.literal("Étoile ajoutée"),
        RepoNavLabel::StarThisRepository => text.literal("Ajouter une étoile à ce dépôt"),
        RepoNavLabel::UnstarThisRepository => text.literal("Retirer l’étoile de ce dépôt"),
        RepoNavLabel::SignInToStar => text.literal("Connectez-vous pour ajouter une étoile à ce dépôt"),
        RepoNavLabel::Stargazers => text.literal("Étoiles"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " fork",
                _ => " forks",
            });
        }
        RepoNavMessage::StargazerCount { count } => {
            let count = count.unsigned_abs();
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " étoile",
                _ => " étoiles",
            });
        }
    }
}
