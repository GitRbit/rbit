//! German (`de`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::Merge => text.literal("Zusammenführen"),
        MergeMessage::ConflictingFiles => text.literal("Dateien mit Konflikten"),
        MergeMessage::WorkInProgress => text.literal("An diesem Pull-Request wird noch gearbeitet."),
        MergeMessage::ReadyForReview => text.literal("Bereit für Review"),
        MergeMessage::MergeMethod => text.literal("Merge-Methode"),
        MergeMessage::CreateMergeCommit => text.literal("Merge-Commit erstellen"),
        MergeMessage::SquashAndMerge => text.literal("Squash und zusammenführen"),
        MergeMessage::RebaseAndMerge => text.literal("Rebase und zusammenführen"),
        MergeMessage::EditMessage => text.literal("Commit-Nachricht bearbeiten"),
        MergeMessage::Message => text.literal("Nachricht"),
        MergeMessage::TitlePlaceholder { title, squash } => {
            text.argument(title);
            text.literal(" – oder bei Squash ");
            text.argument(squash);
        }
        MergeMessage::DeleteHead => text.literal("Quell-Branch nach dem Zusammenführen löschen"),
        MergeMessage::Bypass => {
            text.literal("Ohne Erfüllung der Branch-Schutzregeln zusammenführen (wird protokolliert)");
        }
        MergeMessage::MergePull => text.literal("Pull-Request zusammenführen"),
        MergeMessage::WriteAccessOnly => {
            text.literal("Nur Personen mit Schreibzugriff auf dieses Repository können zusammenführen.");
        }
        MergeMessage::PullOpen => text.literal("Der Pull-Request ist offen"),
        MergeMessage::PullClosed => text.literal("Der Pull-Request ist geschlossen"),
        MergeMessage::StillDraft => text.literal("Dieser Pull-Request ist noch ein Entwurf"),
        MergeMessage::HeadExists => text.literal("Der Quell-Branch existiert"),
        MergeMessage::HeadDeleted => text.literal("Der Quell-Branch wurde gelöscht"),
        MergeMessage::NoConflicts => text.literal("Keine Konflikte mit dem Ziel-Branch"),
        MergeMessage::Conflicts => text.literal("Dieser Branch hat Konflikte, die gelöst werden müssen"),
        MergeMessage::CheckingConflicts => text.literal("Konflikte werden geprüft…"),
        MergeMessage::MergeabilityError => text.literal("Die Zusammenführbarkeit konnte nicht geprüft werden"),
        MergeMessage::Approvals { have, need } => {
            text.number(have);
            text.literal(" von ");
            text.number(need);
            text.literal(match text.plural(need) {
                Plural::One => " erforderlichen Genehmigung",
                _ => " erforderlichen Genehmigungen",
            });
        }
        MergeMessage::NoChangesRequested => text.literal("Keine Änderungen angefordert"),
        MergeMessage::ChangesRequestedBy { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Reviewer hat Änderungen angefordert",
                _ => " Reviewer haben Änderungen angefordert",
            });
        }
        MergeMessage::MayPush => text.literal("Sie dürfen auf den Ziel-Branch pushen"),
        MergeMessage::MayNotPush => text.literal("Sie dürfen nicht auf den Ziel-Branch pushen"),
        MergeMessage::NoChecks => text.literal("Keine Statusprüfungen"),
        MergeMessage::ChecksPending => text.literal("Einige Prüfungen sind noch nicht abgeschlossen"),
        MergeMessage::ChecksPassed => text.literal("Alle Prüfungen waren erfolgreich"),
        MergeMessage::ChecksFailed => text.literal("Einige Prüfungen waren nicht erfolgreich"),
        MergeMessage::RequiredPassed { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " erforderliche Prüfung erfolgreich",
                _ => " erforderliche Prüfungen erfolgreich",
            });
        }
        MergeMessage::RequiredFailed { failing, required } => {
            text.number(failing);
            text.literal(" von ");
            text.number(required);
            text.literal(" erforderlichen Prüfungen fehlgeschlagen");
        }
        MergeMessage::RequiredWaiting { waiting, required } => {
            text.literal("Warten auf ");
            text.number(waiting);
            text.literal(" von ");
            text.number(required);
            text.literal(" erforderlichen Prüfungen");
        }
    }
}
