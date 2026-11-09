//! Spanish (`es`) entries for `catalog/repo/nav.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::RepoNavSentence;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};
use crate::plural::Plural;

entries! { Es:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ArchivedNote => text.literal(
            "Este repositorio está archivado. Es de solo lectura: puedes clonarlo, bifurcarlo y marcarlo con \
             estrella, pero no modificarlo.",
        ),
        RepoNavLabel::Private => text.literal("Privado"),
        RepoNavLabel::Public => text.literal("Público"),
        RepoNavLabel::Repository => text.literal("Repositorio"),
        RepoNavLabel::Code => text.literal("Código"),
        RepoNavLabel::Issues => text.literal("Incidencias"),
        RepoNavLabel::PullRequests => text.literal("Solicitudes de cambios"),
        RepoNavLabel::Commits => text.literal("Commits"),
        RepoNavLabel::Branches => text.literal("Ramas"),
        RepoNavLabel::Tags => text.literal("Tags"),
        RepoNavLabel::Releases => text.literal("Versiones"),
        RepoNavLabel::Activity => text.literal("Actividad"),
        RepoNavLabel::Actions => text.literal("Acciones"),
        RepoNavLabel::Settings => text.literal("Configuración"),
        RepoNavLabel::ForkThisRepository => text.literal("Bifurcar este repositorio"),
        RepoNavLabel::Fork => text.literal("Bifurcar"),
        RepoNavLabel::Forks => text.literal("Bifurcaciones"),
        RepoNavLabel::Clone => text.literal("Clonar"),
        RepoNavLabel::CloneUrl => text.literal("URL de clonación"),
        RepoNavLabel::CopyCloneUrl => text.literal("Copiar la URL de clonación"),
        RepoNavLabel::SshCloneUrl => text.literal("URL de clonación SSH"),
        RepoNavLabel::CopySshCloneUrl => text.literal("Copiar la URL de clonación SSH"),
        RepoNavLabel::Copy => text.literal("Copiar"),
        RepoNavLabel::YourSettings => text.literal("tu configuración"),
        RepoNavLabel::YourSettingsEnd => text.literal("."),
        RepoNavLabel::DownloadZip => text.literal("Descargar ZIP"),
        RepoNavLabel::Star => text.literal("Marcar con estrella"),
        RepoNavLabel::Starred => text.literal("Con estrella"),
        RepoNavLabel::StarThisRepository => text.literal("Marcar este repositorio con estrella"),
        RepoNavLabel::UnstarThisRepository => text.literal("Quitar la estrella de este repositorio"),
        RepoNavLabel::SignInToStar => text.literal("Inicia sesión para marcar este repositorio con estrella"),
        RepoNavLabel::Stargazers => text.literal("Estrellas"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " bifurcación",
                _ => " bifurcaciones",
            });
        }
        RepoNavMessage::StargazerCount { count } => {
            text.number(count.unsigned_abs());
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " estrella",
                _ => " estrellas",
            });
        }
    }
    RepoNavSentence => RichWriter |text| {
        RepoNavSentence::ForkedFrom => {
            text.literal("bifurcado de ");
            text.slot(0);
        }
        RepoNavSentence::MirrorOf => {
            text.literal("réplica de ");
            text.slot(0);
        }
        RepoNavSentence::SshKeyNote => {
            text.literal("SSH necesita una clave en ");
            text.slot(0);
            text.literal(".");
        }
    }
}
