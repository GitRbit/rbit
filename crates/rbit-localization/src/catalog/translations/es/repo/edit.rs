//! Spanish (`es`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};
use crate::plural::Plural;

entries! { Es:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("Nuevo archivo"),
        RepoEditLabel::Edit => text.literal("Editar"),
        RepoEditLabel::On => text.literal("En"),
        RepoEditLabel::NewFileHint => text.literal("Escribe una ruta con / para crear carpetas."),
        RepoEditLabel::EditHint => text.literal("Cambia la ruta para renombrar o mover el archivo."),
        RepoEditLabel::BackToFile => text.literal("Volver al archivo"),
        RepoEditLabel::Path => text.literal("Ruta"),
        RepoEditLabel::Content => text.literal("Contenido"),
        RepoEditLabel::CommitChanges => text.literal("Hacer commit de los cambios"),
        RepoEditLabel::Cancel => text.literal("Cancelar"),
        RepoEditLabel::Delete => text.literal("Eliminar"),
        RepoEditLabel::DeleteHint => text.literal(
            "El archivo permanece en el historial; una carpeta que quede vacía desaparece.",
        ),
        RepoEditLabel::DeleteFile => text.literal("Eliminar archivo"),
        RepoEditLabel::UploadFiles => text.literal("Subir archivos"),
        RepoEditLabel::Into => text.literal("En"),
        RepoEditLabel::OnBranch => text.literal("en"),
        RepoEditLabel::UploadHint => text.literal("Los archivos con el mismo nombre se reemplazan."),
        RepoEditLabel::Files => text.literal("Archivos"),
        RepoEditLabel::NothingCommitted => text.literal("No se hizo ningún commit."),
        RepoEditLabel::ReviewNewCommits => text.literal("Revisa los commits nuevos"),
        RepoEditLabel::KeptAndCommittedOnTop => text.literal(
            "y vuelve a hacer commit: tus cambios se conservan abajo y se aplicarán sobre la nueva versión.",
        ),
        RepoEditLabel::BranchGone => text.literal(
            "No se hizo ningún commit. Haz commit en una rama nueva; tus cambios se conservan abajo.",
        ),
        RepoEditLabel::CommitChangesLegend => text.literal("Hacer commit de los cambios"),
        RepoEditLabel::CommitMessage => text.literal("Mensaje del commit"),
        RepoEditLabel::ExtendedDescription => text.literal("Descripción ampliada"),
        RepoEditLabel::Optional => text.literal("opcional"),
        RepoEditLabel::CommitDirectlyTo => text.literal("Hacer commit directamente en"),
        RepoEditLabel::NotAllowed => text.literal("No permitido:"),
        RepoEditLabel::NewBranchAndPull => {
            text.literal("Crear una rama nueva y abrir una solicitud de cambios");
        }
        RepoEditLabel::NewBranchName => text.literal("Nombre de la rama nueva"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size } => {
            text.literal("Como máximo ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size } => {
            text.literal("Hasta ");
            text.number(files as u64);
            text.literal(match text.plural(files as u64) {
                Plural::One => " archivo de como máximo ",
                _ => " archivos de como máximo ",
            });
            text.argument(size);
            text.literal(" cada uno, en un mismo commit.");
        }
        RepoEditMessage::BranchChanged { branch } => {
            text.argument(branch);
            text.literal(" cambió desde que empezaste");
        }
        RepoEditMessage::BranchMissing { branch } => {
            text.argument(branch);
            text.literal(" ya no existe");
        }
        RepoEditMessage::NowPointsAt => text.literal("Ahora apunta a"),
        RepoEditMessage::NotEditable { limit } => {
            text.literal("Este archivo es binario o pesa más de ");
            text.argument(limit);
            text.literal(", así que no se puede editar en el navegador. Puedes eliminarlo o subir uno que lo reemplace.");
        }
        RepoEditMessage::EditingTitle { path } => {
            text.literal("Editando ");
            text.argument(path);
        }
        RepoEditMessage::DeleteTitle { path } => {
            text.literal("Eliminar ");
            text.argument(path);
        }
    }
}
