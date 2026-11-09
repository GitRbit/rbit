//! German (`de`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Changes => text.literal("Änderungen"),
        ReviewMessage::ToggleFileTree => text.literal("Dateibaum ein-/ausblenden"),
        ReviewMessage::CollapseAll => text.literal("Alle einklappen"),
        ReviewMessage::ExpandAll => text.literal("Alle ausklappen"),
        ReviewMessage::OutdatedComments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " veralteter Kommentar steht in der Unterhaltung",
                _ => " veraltete Kommentare stehen in der Unterhaltung",
            });
        }
        ReviewMessage::FinishReview => text.literal("Review abschließen"),
        ReviewMessage::Truncated => text.literal(
            "Diese Änderung ist sehr groß, daher werden einige Diffs nicht angezeigt. Klonen Sie das Repository, um die vollständige Änderung zu sehen.",
        ),
        ReviewMessage::FileToggleHint => {
            text.literal("Klicken zum Ein- oder Ausklappen | Alt-Klick für alle Dateien");
        }
        ReviewMessage::ViewFile => text.literal("Datei anzeigen"),
        ReviewMessage::CommentOnLine { line } => {
            text.literal("Kommentar zu Zeile ");
            text.number(line);
        }
        ReviewMessage::StartReview => text.literal("Review beginnen"),
        ReviewMessage::AddSingleComment => text.literal("Einzelnen Kommentar hinzufügen"),
        ReviewMessage::BinaryFile => text.literal("Binärdatei wird nicht angezeigt."),
        ReviewMessage::TooLarge => text.literal("Diff zu groß für die Anzeige."),
        ReviewMessage::SubmoduleCommit => text.literal("Submodul-Commit"),
        ReviewMessage::NoContentChanges => text.literal("Keine inhaltlichen Änderungen."),
        ReviewMessage::NoChangesTitle => text.literal("Keine Änderungen"),
        ReviewMessage::NoChangesText => text.literal("Der Quell-Branch fügt dem Ziel-Branch nichts hinzu."),
        ReviewMessage::Added => text.literal("Hinzugefügt"),
        ReviewMessage::Deleted => text.literal("Gelöscht"),
        ReviewMessage::Modified => text.literal("Geändert"),
        ReviewMessage::Renamed => text.literal("Umbenannt"),
        ReviewMessage::Copied => text.literal("Kopiert"),
        ReviewMessage::Outdated => text.literal("Veraltet"),
        ReviewMessage::Pending => text.literal("Ausstehend"),
        ReviewMessage::ResolvedThread { path, line } => {
            text.literal("Gelöste Unterhaltung · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::ReplyPlaceholder => text.literal("Antworten…"),
        ReviewMessage::AddToReview => text.literal("Zum Review hinzufügen"),
        ReviewMessage::ReopenConversation => text.literal("Unterhaltung erneut öffnen"),
        ReviewMessage::ResolveConversation => text.literal("Unterhaltung als gelöst markieren"),
        ReviewMessage::ReviewChanges => text.literal("Änderungen prüfen"),
        ReviewMessage::PendingCount { count } => {
            text.number(count);
            text.literal(" ausstehend");
        }
        ReviewMessage::ReviewPlaceholder => text.literal("Kommentar hinterlassen (Markdown wird unterstützt)"),
        ReviewMessage::RequestChanges => text.literal("Änderungen anfordern"),
        ReviewMessage::Approve => text.literal("Genehmigen"),
        ReviewMessage::DiscardReview => text.literal("Ausstehendes Review verwerfen"),
        ReviewMessage::DismissReview => text.literal("Review verwerfen"),
        ReviewMessage::DismissReason => text.literal("Warum wird dieses Review verworfen?"),
        ReviewMessage::Dismiss => text.literal("Verwerfen"),
        ReviewMessage::DismissedBy { by, message } => {
            text.literal("Verworfen von ");
            text.argument(by);
            text.literal(": ");
            text.argument(message);
        }
        ReviewMessage::Reviewers => text.literal("Reviewer"),
        ReviewMessage::NoReviews => text.literal("Noch keine Reviews"),
        ReviewMessage::RemoveRequestFor { name } => {
            text.literal("Review-Anfrage an ");
            text.argument(name);
            text.literal(" entfernen");
        }
        ReviewMessage::RemoveRequest => text.literal("Anfrage entfernen"),
        ReviewMessage::RequestReviewFrom => text.literal("Review anfordern von"),
        ReviewMessage::Username => text.literal("Benutzername"),
        ReviewMessage::RequestReview => text.literal("Review anfordern"),
        ReviewMessage::Approved => text.literal("Genehmigt"),
        ReviewMessage::ChangesRequested => text.literal("Änderungen angefordert"),
        ReviewMessage::Dismissed => text.literal("Verworfen"),
        ReviewMessage::Commented => text.literal("Kommentiert"),
        ReviewMessage::AwaitingReview => text.literal("Wartet auf Review"),
    }
}
