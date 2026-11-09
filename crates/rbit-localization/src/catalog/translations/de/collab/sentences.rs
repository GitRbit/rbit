//! German (`de`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;
use crate::plural::Plural;

entries! { De:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { .. } => slot_then(text, 0, " Offen"),
        CollabSentence::ClosedCount { .. } => slot_then(text, 0, " Geschlossen"),
        CollabSentence::IssueSearchHelp => {
            text.literal("Filtern Sie die Ergebnisse über die Menüs oder geben Sie Begriffe wie ");
            examples(text);
            text.literal(
                " ein. Die Textsuche durchsucht Titel und Beschreibungen. Jedes Menü behält Ihre übrigen Filter bei.",
            );
        }
        CollabSentence::PullSearchHelp => {
            text.literal("Wählen Sie unten Filter oder geben Sie Begriffe wie ");
            examples(text);
            text.literal(" ein.");
        }
        CollabSentence::OpenedIssue { comments } => opened(text, " hat dieses Issue ", comments),
        CollabSentence::OpenedPull { comments } => opened(text, " hat diesen Pull-Request ", comments),
        CollabSentence::RowOpened { number, author } => row(text, number, author, " eröffnet"),
        CollabSentence::RowClosed { number } => {
            text.argument(number);
            text.literal(" ");
            slot_then(text, 0, " geschlossen");
        }
        CollabSentence::RowClosedBy { number, author } => row(text, number, author, " geschlossen"),
        CollabSentence::RowMergedBy { number, author } => row(text, number, author, " zusammengeführt"),
        CollabSentence::NoOneAssignYourself => {
            text.literal("Niemand – ");
            text.slot(0);
        }
        CollabSentence::PercentComplete => slot_then(text, 0, " abgeschlossen"),
        CollabSentence::WantsToMerge => {
            text.slot(0);
            text.literal(" möchte ");
            from_into(text);
            text.literal(" zusammenführen");
        }
        CollabSentence::MergedInto => {
            text.slot(0);
            text.literal(" hat ");
            from_into(text);
            text.literal(" zusammengeführt");
        }
        CollabSentence::ProposedToMerge => {
            text.slot(0);
            text.literal(" hat vorgeschlagen, ");
            from_into(text);
            text.literal(" zusammenzuführen");
        }
        CollabSentence::SignInToOpenPull => slot_then(text, 0, ", um einen Pull-Request zu eröffnen."),
        CollabSentence::SignInToCommentOnLines => slot_then(text, 0, ", um Zeilen zu kommentieren."),
        CollabSentence::SignInToJoin => slot_then(text, 0, ", um an der Unterhaltung teilzunehmen."),
        CollabSentence::PullMergedAs => {
            text.literal("Pull-Request als ");
            slot_then(text, 0, " zusammengeführt.");
        }
        CollabSentence::FilesChanged { count } => {
            let words = match text.plural(count) {
                Plural::One => " Datei geändert",
                _ => " Dateien geändert",
            };
            slot_then(text, 0, words);
        }
        CollabSentence::CommentOpened => {
            text.literal("hat dies ");
            slot_then(text, 0, " eröffnet");
        }
        CollabSentence::CommentCommented => {
            text.literal("hat ");
            slot_then(text, 0, " kommentiert");
        }
        CollabSentence::ReviewApproved => slot_then(text, 0, " hat diese Änderungen genehmigt"),
        CollabSentence::ReviewChangesRequested => slot_then(text, 0, " hat Änderungen angefordert"),
        CollabSentence::ReviewDismissed => slot_then(text, 0, " hat ein Review abgegeben (verworfen)"),
        CollabSentence::Reviewed => slot_then(text, 0, " hat ein Review abgegeben"),
    }
}

fn slot_then(text: &mut RichWriter, slot: usize, words: &str) {
    text.slot(slot);
    text.literal(words);
}

/// „a, b, c oder d“ over slots 0–3.
fn examples(text: &mut RichWriter) {
    text.slot(0);
    text.literal(", ");
    text.slot(1);
    text.literal(", ");
    text.slot(2);
    text.literal(" oder ");
    text.slot(3);
}

/// „alice hat dieses Issue vor 2 Tagen eröffnet · 3 Kommentare“.
fn opened(text: &mut RichWriter, words: &str, comments: u64) {
    text.slot(0);
    text.literal(words);
    slot_then(text, 1, " eröffnet · ");
    text.number(comments);
    text.literal(match text.plural(comments) {
        Plural::One => " Kommentar",
        _ => " Kommentare",
    });
}

/// „#12 vor 2 Tagen von alice eröffnet“.
fn row(text: &mut RichWriter, number: &str, author: &str, verb: &str) {
    text.argument(number);
    text.literal(" ");
    slot_then(text, 0, " von ");
    text.argument(author);
    text.literal(verb);
}

/// „3 Commits aus feature in main“: slot 1 from slot 3 into slot 2.
fn from_into(text: &mut RichWriter) {
    text.slot(1);
    text.literal(" aus ");
    text.slot(3);
    text.literal(" in ");
    text.slot(2);
}
