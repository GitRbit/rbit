//! Spanish (`es`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;
use crate::plural::Plural;

entries! { Es:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("Wiki"),
        WikiMessage::Pages => text.literal("Páginas"),
        WikiMessage::AllPages => text.literal("Todas las páginas"),
        WikiMessage::PagesLabel => text.literal("Páginas de la wiki"),
        WikiMessage::BreadcrumbLabel => text.literal("Ruta de navegación"),
        WikiMessage::SearchPages => text.literal("Buscar páginas"),
        WikiMessage::Search => text.literal("Buscar"),
        WikiMessage::NewPage => text.literal("Nueva página"),
        WikiMessage::EditPage => text.literal("Editar página"),
        WikiMessage::Edit => text.literal("Editar"),
        WikiMessage::History => text.literal("Historial"),
        WikiMessage::Delete => text.literal("Eliminar"),
        WikiMessage::LastEdited => text.literal("Última edición"),
        WikiMessage::Revision { generation } => {
            text.literal("Revisión ");
            text.number(u64::try_from(generation).unwrap_or_default());
        }
        WikiMessage::RevisionColumn => text.literal("Revisión"),
        WikiMessage::Author => text.literal("Autor"),
        WikiMessage::Date => text.literal("Fecha"),
        WikiMessage::UnknownAuthor => text.literal("Autor desconocido"),
        WikiMessage::Created => text.literal("Creada"),
        WikiMessage::Edited => text.literal("Editada"),
        WikiMessage::Renamed => text.literal("Renombrada"),
        WikiMessage::Deleted => text.literal("Eliminada"),
        WikiMessage::Restored => text.literal("Restaurada"),
        WikiMessage::View => text.literal("Ver"),
        WikiMessage::CompareWithPrevious => text.literal("Comparar con la anterior"),
        WikiMessage::Restore => text.literal("Restaurar esta revisión"),
        WikiMessage::OlderRevisions => text.literal("Revisiones anteriores"),
        WikiMessage::OldRevisionNotice => text.literal("Estás viendo una revisión anterior de esta página."),
        WikiMessage::ViewCurrent => text.literal("Ver la página actual"),
        WikiMessage::HistoryOf { title } => {
            text.literal("Historial de ");
            text.argument(title);
        }
        WikiMessage::Comparing { base, head } => {
            text.literal("Cambios de la revisión ");
            text.number(u64::try_from(base).unwrap_or_default());
            text.literal(" a la revisión ");
            text.number(u64::try_from(head).unwrap_or_default());
        }
        WikiMessage::Changes { additions, deletions } => {
            text.number(additions);
            text.literal(match text.plural(additions) {
                Plural::One => " adición, ",
                _ => " adiciones, ",
            });
            text.number(deletions);
            text.literal(match text.plural(deletions) {
                Plural::One => " eliminación",
                _ => " eliminaciones",
            });
        }
        WikiMessage::NoDifferences => text.literal("Estas revisiones tienen el mismo contenido."),
        WikiMessage::Title => text.literal("Título"),
        WikiMessage::Path => text.literal("Ruta"),
        WikiMessage::PathHint => text.literal(
            "Letras, dígitos, - _ y . separados por /. Déjala vacía para obtenerla del título.",
        ),
        WikiMessage::Content => text.literal("Contenido"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("Guardar página"),
        WikiMessage::Preview => text.literal("Vista previa"),
        WikiMessage::NothingToPreview => text.literal("No hay nada que previsualizar."),
        WikiMessage::Cancel => text.literal("Cancelar"),
        WikiMessage::TitleEmpty => text.literal("Introduce un título."),
        WikiMessage::TitleTooLong => text.literal("El título es demasiado largo."),
        WikiMessage::TitleInvalid => text.literal("El título contiene caracteres no permitidos."),
        WikiMessage::PathEmpty => text.literal("Introduce una ruta o un título del que obtenerla."),
        WikiMessage::PathTooLong => text.literal("La ruta o alguna de sus partes es demasiado larga."),
        WikiMessage::PathTooDeep => text.literal("La ruta tiene demasiadas partes."),
        WikiMessage::PathInvalid => text.literal(
            "Cada parte de la ruta debe empezar por una letra o un dígito y usar solo letras, dígitos, - _ y .",
        ),
        WikiMessage::PathReserved => text.literal("La ruta usa un nombre reservado."),
        WikiMessage::ContentTooLarge => text.literal("El contenido es demasiado grande."),
        WikiMessage::ContentInvalid => text.literal("El contenido contiene caracteres no permitidos."),
        WikiMessage::Conflict => text.literal(
            "Esta página cambió después de que empezaras. Tu texto se conserva abajo; revisa la página actual antes \
             de volver a guardar.",
        ),
        WikiMessage::CompareChanges => text.literal("Comparar los cambios"),
        WikiMessage::PathTaken => text.literal("Otra página ya usa esta ruta."),
        WikiMessage::Unchanged => text.literal("No cambió nada."),
        WikiMessage::DeleteTitle { title } => {
            text.literal("¿Eliminar ");
            text.argument(title);
            text.literal("?");
        }
        WikiMessage::DeletePage => text.literal("Eliminar página"),
        WikiMessage::Disabled => text.literal("La wiki está desactivada en este repositorio."),
        WikiMessage::About => text.literal(
            "Una wiki contiene páginas de documentación con su propio historial, separadas del código.",
        ),
        WikiMessage::TurnOn => text.literal("Activar la wiki"),
        WikiMessage::TurnOff => text.literal("Desactivar la wiki"),
        WikiMessage::Empty => text.literal("Esta wiki aún no tiene páginas."),
        WikiMessage::CreateFirst => text.literal("Crear la primera página"),
        WikiMessage::Missing => text.literal("Ninguna página tiene esta ruta."),
        WikiMessage::CreatePath { path } => {
            text.literal("Crear ");
            text.argument(path);
        }
        WikiMessage::SearchCount { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " página coincide con «",
                _ => " páginas coinciden con «",
            });
            text.argument(query);
            text.literal("»");
        }
        WikiMessage::NoResults { query } => {
            text.literal("Ninguna página coincide con «");
            text.argument(query);
            text.literal("».");
        }
        WikiMessage::MorePages => text.literal("Más páginas"),
        WikiMessage::DeleteKeepsHistory => text.literal(
            "La página se quita de la wiki. Su historial se conserva y quienes tienen acceso de escritura pueden              restaurarla desde las páginas eliminadas.",
        ),
        WikiMessage::DeletedPages => text.literal("Páginas eliminadas"),
        WikiMessage::NoDeletedPages => text.literal("No se ha eliminado ninguna página."),
        WikiMessage::DeletedNotice => text.literal("Esta página se eliminó. Su historial se conserva."),
        WikiMessage::RestorePage => text.literal("Restaurar página"),
        WikiMessage::MoreDeletedPages => text.literal("Eliminaciones anteriores"),
        WikiMessage::RestoreBlocked => text.literal(
            "Otra página usa ahora esta ruta. Cambia primero el nombre de esa página y luego restaura esta.",
        ),
        WikiMessage::RenamedFrom { path } => {
            text.literal("Renombrada desde ");
            text.argument(path);
        }
    }
}
