//! German (`de`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;
use crate::plural::Plural;

entries! { De:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("Wiki"),
        WikiMessage::Pages => text.literal("Seiten"),
        WikiMessage::AllPages => text.literal("Alle Seiten"),
        WikiMessage::PagesLabel => text.literal("Wiki-Seiten"),
        WikiMessage::BreadcrumbLabel => text.literal("Navigationspfad"),
        WikiMessage::SearchPages => text.literal("Seiten durchsuchen"),
        WikiMessage::Search => text.literal("Suchen"),
        WikiMessage::NewPage => text.literal("Neue Seite"),
        WikiMessage::EditPage => text.literal("Seite bearbeiten"),
        WikiMessage::Edit => text.literal("Bearbeiten"),
        WikiMessage::History => text.literal("Verlauf"),
        WikiMessage::Delete => text.literal("Löschen"),
        WikiMessage::LastEdited => text.literal("Zuletzt bearbeitet"),
        WikiMessage::Revision { generation } => {
            text.literal("Revision ");
            text.number(u64::try_from(generation).unwrap_or_default());
        }
        WikiMessage::RevisionColumn => text.literal("Revision"),
        WikiMessage::Author => text.literal("Autor"),
        WikiMessage::Date => text.literal("Datum"),
        WikiMessage::UnknownAuthor => text.literal("Unbekannter Autor"),
        WikiMessage::Created => text.literal("Erstellt"),
        WikiMessage::Edited => text.literal("Bearbeitet"),
        WikiMessage::Renamed => text.literal("Umbenannt"),
        WikiMessage::Deleted => text.literal("Gelöscht"),
        WikiMessage::Restored => text.literal("Wiederhergestellt"),
        WikiMessage::View => text.literal("Anzeigen"),
        WikiMessage::CompareWithPrevious => text.literal("Mit vorheriger vergleichen"),
        WikiMessage::Restore => text.literal("Diese Revision wiederherstellen"),
        WikiMessage::OlderRevisions => text.literal("Ältere Revisionen"),
        WikiMessage::OldRevisionNotice => {
            text.literal("Sie sehen eine ältere Revision dieser Seite.");
        }
        WikiMessage::ViewCurrent => text.literal("Aktuelle Seite anzeigen"),
        WikiMessage::HistoryOf { title } => {
            text.literal("Verlauf von ");
            text.argument(title);
        }
        WikiMessage::Comparing { base, head } => {
            text.literal("Änderungen von Revision ");
            text.number(u64::try_from(base).unwrap_or_default());
            text.literal(" zu Revision ");
            text.number(u64::try_from(head).unwrap_or_default());
        }
        WikiMessage::Changes { additions, deletions } => {
            text.number(additions);
            text.literal(match text.plural(additions) {
                Plural::One => " Hinzufügung, ",
                _ => " Hinzufügungen, ",
            });
            text.number(deletions);
            text.literal(match text.plural(deletions) {
                Plural::One => " Löschung",
                _ => " Löschungen",
            });
        }
        WikiMessage::NoDifferences => text.literal("Diese Revisionen haben denselben Inhalt."),
        WikiMessage::Title => text.literal("Titel"),
        WikiMessage::Path => text.literal("Pfad"),
        WikiMessage::PathHint => text.literal(
            "Buchstaben, Ziffern, - _ und ., getrennt durch /. Leer lassen, um ihn aus dem Titel abzuleiten.",
        ),
        WikiMessage::Content => text.literal("Inhalt"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("Seite speichern"),
        WikiMessage::Preview => text.literal("Vorschau"),
        WikiMessage::NothingToPreview => text.literal("Keine Vorschau verfügbar."),
        WikiMessage::Cancel => text.literal("Abbrechen"),
        WikiMessage::TitleEmpty => text.literal("Geben Sie einen Titel ein."),
        WikiMessage::TitleTooLong => text.literal("Der Titel ist zu lang."),
        WikiMessage::TitleInvalid => text.literal("Der Titel enthält unzulässige Zeichen."),
        WikiMessage::PathEmpty => {
            text.literal("Geben Sie einen Pfad ein oder einen Titel, aus dem er abgeleitet werden kann.");
        }
        WikiMessage::PathTooLong => text.literal("Der Pfad oder einer seiner Teile ist zu lang."),
        WikiMessage::PathTooDeep => text.literal("Der Pfad hat zu viele Teile."),
        WikiMessage::PathInvalid => text.literal(
            "Jeder Teil des Pfads muss mit einem Buchstaben oder einer Ziffer beginnen und darf nur Buchstaben, \
             Ziffern, - _ und . enthalten.",
        ),
        WikiMessage::PathReserved => text.literal("Der Pfad verwendet einen reservierten Namen."),
        WikiMessage::ContentTooLarge => text.literal("Der Inhalt ist zu groß."),
        WikiMessage::ContentInvalid => text.literal("Der Inhalt enthält unzulässige Zeichen."),
        WikiMessage::Conflict => text.literal(
            "Diese Seite wurde geändert, nachdem Sie begonnen haben. Ihr Text bleibt unten erhalten; prüfen Sie die \
             aktuelle Seite, bevor Sie erneut speichern.",
        ),
        WikiMessage::CompareChanges => text.literal("Änderungen vergleichen"),
        WikiMessage::PathTaken => text.literal("Eine andere Seite verwendet diesen Pfad bereits."),
        WikiMessage::Unchanged => text.literal("Nichts geändert."),
        WikiMessage::DeleteTitle { title } => {
            text.argument(title);
            text.literal(" löschen?");
        }
        WikiMessage::DeletePage => text.literal("Seite löschen"),
        WikiMessage::Disabled => text.literal("Das Wiki ist für dieses Repository deaktiviert."),
        WikiMessage::About => text.literal(
            "Ein Wiki enthält Dokumentationsseiten mit eigenem Verlauf, getrennt vom Code.",
        ),
        WikiMessage::TurnOn => text.literal("Wiki aktivieren"),
        WikiMessage::TurnOff => text.literal("Wiki deaktivieren"),
        WikiMessage::Empty => text.literal("Dieses Wiki hat noch keine Seiten."),
        WikiMessage::CreateFirst => text.literal("Erste Seite erstellen"),
        WikiMessage::Missing => text.literal("Keine Seite hat diesen Pfad."),
        WikiMessage::CreatePath { path } => {
            text.argument(path);
            text.literal(" erstellen");
        }
        WikiMessage::SearchCount { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Seite passt zu „",
                _ => " Seiten passen zu „",
            });
            text.argument(query);
            text.literal("“");
        }
        WikiMessage::NoResults { query } => {
            text.literal("Keine Seiten passen zu „");
            text.argument(query);
            text.literal("“.");
        }
        WikiMessage::MorePages => text.literal("Weitere Seiten"),
    }
}
