//! Spanish (`es`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};
use crate::plural::Plural;

entries! { Es:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("Versiones"),
        ReleaseLabel::NewRelease => text.literal("Nueva versión"),
        ReleaseLabel::EditRelease => text.literal("Editar versión"),
        ReleaseLabel::NoReleasesYet => text.literal("Aún no hay versiones"),
        ReleaseLabel::EmptyForWriters => text.literal(
            "Una versión empaqueta un tag con notas y archivos. Envía un tag y crea después una versión para él.",
        ),
        ReleaseLabel::EmptyForReaders => {
            text.literal("Las versiones marcan entregas de este proyecto con notas y descargas.");
        }
        ReleaseLabel::Draft => text.literal("Borrador"),
        ReleaseLabel::PreRelease => text.literal("Versión preliminar"),
        ReleaseLabel::Latest => text.literal("Más reciente"),
        ReleaseLabel::Bot => text.literal("bot"),
        ReleaseLabel::Created => text.literal("creó"),
        ReleaseLabel::Edit => text.literal("Editar"),
        ReleaseLabel::Downloads => text.literal("Descargas"),
        ReleaseLabel::SourceZip => text.literal("Código fuente (zip)"),
        ReleaseLabel::SourceTarGz => text.literal("Código fuente (tar.gz)"),
        ReleaseLabel::Files => text.literal("Archivos"),
        ReleaseLabel::Remove => text.literal("Quitar"),
        ReleaseLabel::FileToAttach => text.literal("Archivo que adjuntar"),
        ReleaseLabel::Upload => text.literal("Subir"),
        ReleaseLabel::DeleteTitle => text.literal("Eliminar esta versión"),
        ReleaseLabel::DeleteHintStart => text.literal("Se eliminan la versión, sus notas y sus archivos. El tag"),
        ReleaseLabel::DeleteHintEnd => text.literal("se conserva."),
        ReleaseLabel::DeleteRelease => text.literal("Eliminar versión"),
        ReleaseLabel::FormLead => text.literal("Una versión describe un tag: sus notas y los archivos que descargar."),
        ReleaseLabel::Tag => text.literal("Tag"),
        ReleaseLabel::AnExisting => text.literal("Un"),
        ReleaseLabel::TagLink => text.literal("tag"),
        ReleaseLabel::OrNewTag => text.literal("existente o uno nuevo creado a partir de la rama de abajo."),
        ReleaseLabel::CreateTagFrom => text.literal("Crear un tag nuevo a partir de"),
        ReleaseLabel::TargetHint => text.literal("solo se usa si el tag no existe"),
        ReleaseLabel::OneOfTags => text.literal("Uno de los"),
        ReleaseLabel::TagsLink => text.literal("tags del repositorio"),
        ReleaseLabel::Title => text.literal("Título"),
        ReleaseLabel::TitleHint => text.literal("opcional; por defecto, el tag"),
        ReleaseLabel::Notes => text.literal("Notas"),
        ReleaseLabel::IsPreRelease => text.literal("Es una versión preliminar"),
        ReleaseLabel::PreReleaseHint => text.literal(
            "Se marca como no lista para producción; como más reciente se prefiere una versión definitiva.",
        ),
        ReleaseLabel::SaveRelease => text.literal("Guardar versión"),
        ReleaseLabel::PublishRelease => text.literal("Publicar versión"),
        ReleaseLabel::SaveDraft => text.literal("Guardar borrador"),
        ReleaseLabel::Cancel => text.literal("Cancelar"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256 } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " descarga",
                _ => " descargas",
            });
        }
        ReleaseMessage::RemoveFile { name } => {
            text.literal("Quitar ");
            text.argument(name);
        }
        ReleaseMessage::UploadLimit { size } => {
            text.literal("Hasta ");
            text.argument(size);
            text.literal(" por archivo. Las descargas se sirven siempre como archivos, nunca se muestran.");
        }
        ReleaseMessage::Uploaded { name } => {
            text.literal("Se subió ");
            text.argument(name);
            text.literal(".");
        }
        ReleaseMessage::Removed { name } => {
            text.literal("Se quitó ");
            text.argument(name);
            text.literal(".");
        }
        ReleaseMessage::EditTitle { tag } => {
            text.literal("Editar ");
            text.argument(tag);
        }
    }
}
