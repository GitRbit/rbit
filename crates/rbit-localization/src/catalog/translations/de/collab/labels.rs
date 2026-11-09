//! German (`de`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("Neues Label"),
        LabelMessage::Name => text.literal("Name"),
        LabelMessage::Colour => text.literal("Farbe"),
        LabelMessage::CreateLabel => text.literal("Label erstellen"),
        LabelMessage::Count { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Label",
                _ => " Labels",
            });
        }
        LabelMessage::EmptyTitle => text.literal("Noch keine Labels"),
        LabelMessage::EmptyText => {
            text.literal("Labels ordnen Issues und Pull-Requests: bug, enhancement, good first issue…");
        }
        LabelMessage::CreateDefaults => text.literal("Standardlabels erstellen"),
        LabelMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " offenes Issue",
                _ => " offene Issues",
            });
        }
        LabelMessage::DeleteConfirm { name } => {
            text.literal("Label „");
            text.argument(name);
            text.literal("“ löschen? Es wird von allen Issues entfernt.");
        }
    }
}
