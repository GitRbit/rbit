//! Spanish (`es`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("Nueva etiqueta"),
        LabelMessage::Name => text.literal("Nombre"),
        LabelMessage::Colour => text.literal("Color"),
        LabelMessage::CreateLabel => text.literal("Crear etiqueta"),
        LabelMessage::Count { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " etiqueta",
                _ => " etiquetas",
            });
        }
        LabelMessage::EmptyTitle => text.literal("Aún no hay etiquetas"),
        LabelMessage::EmptyText => text.literal(
            "Las etiquetas clasifican incidencias y solicitudes de cambios: error, mejora, buena primera incidencia…",
        ),
        LabelMessage::CreateDefaults => text.literal("Crear las etiquetas predeterminadas"),
        LabelMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " incidencia abierta",
                _ => " incidencias abiertas",
            });
        }
        LabelMessage::DeleteConfirm { name } => {
            text.literal("¿Eliminar la etiqueta «");
            text.argument(name);
            text.literal("»? Se quitará de todas las incidencias.");
        }
    }
}
