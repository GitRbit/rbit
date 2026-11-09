//! Spanish (`es`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;
use crate::plural::Plural;

entries! { Es:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { count } => {
            let word = if text.plural(count) == Plural::One { " abierta" } else { " abiertas" };
            slot_then(text, 0, word);
        }
        CollabSentence::ClosedCount { count } => {
            let word = if text.plural(count) == Plural::One { " cerrada" } else { " cerradas" };
            slot_then(text, 0, word);
        }
        CollabSentence::IssueSearchHelp => {
            text.literal("Usa los menús para filtrar los resultados o escribe términos como ");
            examples(text);
            text.literal(". El texto busca en títulos y descripciones. Cada menú conserva tus otros filtros.");
        }
        CollabSentence::PullSearchHelp => {
            text.literal("Elige los filtros de abajo o escribe términos como ");
            examples(text);
            text.literal(".");
        }
        CollabSentence::OpenedIssue { comments } => opened(text, " abrió esta incidencia ", comments),
        CollabSentence::OpenedPull { comments } => opened(text, " abrió esta solicitud de cambios ", comments),
        CollabSentence::RowOpened { number, author } => row(text, number, " abierta ", Some(author)),
        CollabSentence::RowClosed { number } => row(text, number, " cerrada ", None),
        CollabSentence::RowClosedBy { number, author } => row(text, number, " cerrada ", Some(author)),
        CollabSentence::RowMergedBy { number, author } => row(text, number, " fusionada ", Some(author)),
        CollabSentence::NoOneAssignYourself => {
            text.literal("Nadie: ");
            text.slot(0);
        }
        CollabSentence::PercentComplete => slot_then(text, 0, " completado"),
        CollabSentence::WantsToMerge => header(text, " quiere fusionar "),
        CollabSentence::MergedInto => header(text, " fusionó "),
        CollabSentence::ProposedToMerge => header(text, " propuso fusionar "),
        CollabSentence::SignInToOpenPull => slot_then(text, 0, " para abrir una solicitud de cambios."),
        CollabSentence::SignInToCommentOnLines => slot_then(text, 0, " para comentar líneas."),
        CollabSentence::SignInToJoin => slot_then(text, 0, " para participar en la conversación."),
        CollabSentence::PullMergedAs => {
            text.literal("Solicitud de cambios fusionada como ");
            text.slot(0);
            text.literal(".");
        }
        CollabSentence::FilesChanged { count } => {
            let words = if text.plural(count) == Plural::One { " archivo modificado" } else { " archivos modificados" };
            slot_then(text, 0, words);
        }
        CollabSentence::CommentOpened => {
            text.literal("abrió ");
            text.slot(0);
        }
        CollabSentence::CommentCommented => {
            text.literal("comentó ");
            text.slot(0);
        }
        CollabSentence::ReviewApproved => slot_then(text, 0, " aprobó estos cambios"),
        CollabSentence::ReviewChangesRequested => slot_then(text, 0, " solicitó cambios"),
        CollabSentence::ReviewDismissed => slot_then(text, 0, " revisó (descartada)"),
        CollabSentence::Reviewed => slot_then(text, 0, " revisó"),
    }
}

fn slot_then(text: &mut RichWriter, slot: usize, words: &str) {
    text.slot(slot);
    text.literal(words);
}

/// "a, b, c o d" over slots 0–3.
fn examples(text: &mut RichWriter) {
    text.slot(0);
    text.literal(", ");
    text.slot(1);
    text.literal(", ");
    text.slot(2);
    text.literal(" o ");
    text.slot(3);
}

/// "alice abrió esta incidencia hace 2 días · 3 comentarios".
fn opened(text: &mut RichWriter, words: &str, comments: u64) {
    text.slot(0);
    text.literal(words);
    text.slot(1);
    text.literal(" · ");
    text.number(comments);
    text.literal(match text.plural(comments) {
        Plural::One => " comentario",
        _ => " comentarios",
    });
}

/// "#12 abierta hace 2 días por alice".
fn row(text: &mut RichWriter, number: &str, words: &str, author: Option<&str>) {
    text.argument(number);
    text.literal(words);
    text.slot(0);
    if let Some(author) = author {
        text.literal(" por ");
        text.argument(author);
    }
}

/// "alice quiere fusionar 3 commits en main desde feature".
fn header(text: &mut RichWriter, words: &str) {
    text.slot(0);
    text.literal(words);
    text.slot(1);
    text.literal(" en ");
    text.slot(2);
    text.literal(" desde ");
    text.slot(3);
}
