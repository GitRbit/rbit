//! French (`fr`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;
use crate::plural::Plural;

entries! { Fr:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { count } => counted(text, count, " ouvert", " ouverts"),
        CollabSentence::ClosedCount { count } => counted(text, count, " fermé", " fermés"),
        CollabSentence::IssueSearchHelp => {
            text.literal("Utilisez les menus pour filtrer les résultats, ou saisissez des termes comme ");
            examples(text);
            text.literal(
                ". Le texte est recherché dans les titres et les descriptions. Chaque menu conserve vos autres filtres.",
            );
        }
        CollabSentence::PullSearchHelp => {
            text.literal("Choisissez des filtres ci-dessous, ou saisissez des termes comme ");
            examples(text);
            text.literal(".");
        }
        CollabSentence::OpenedIssue { comments } => opened(text, " a ouvert ce ticket ", comments),
        CollabSentence::OpenedPull { comments } => opened(text, " a ouvert cette pull request ", comments),
        CollabSentence::RowOpened { number, author } => row(text, number, " ouvert ", Some(author)),
        CollabSentence::RowClosed { number } => row(text, number, " fermé ", None),
        CollabSentence::RowClosedBy { number, author } => row(text, number, " fermée ", Some(author)),
        CollabSentence::RowMergedBy { number, author } => row(text, number, " fusionnée ", Some(author)),
        CollabSentence::NoOneAssignYourself => {
            text.literal("Personne — ");
            text.slot(0);
        }
        CollabSentence::PercentComplete => slot_then(text, 0, " terminé"),
        CollabSentence::WantsToMerge => header(text, " souhaite fusionner "),
        CollabSentence::MergedInto => header(text, " a fusionné "),
        CollabSentence::ProposedToMerge => header(text, " a proposé de fusionner "),
        CollabSentence::SignInToOpenPull => slot_then(text, 0, " pour ouvrir une pull request."),
        CollabSentence::SignInToCommentOnLines => slot_then(text, 0, " pour commenter des lignes."),
        CollabSentence::SignInToJoin => slot_then(text, 0, " pour participer à la conversation."),
        CollabSentence::PullMergedAs => {
            text.literal("Pull request fusionnée avec le commit ");
            text.slot(0);
            text.literal(".");
        }
        CollabSentence::FilesChanged { count } => counted(text, count, " fichier modifié", " fichiers modifiés"),
        CollabSentence::CommentOpened => {
            text.literal("a ouvert ");
            text.slot(0);
        }
        CollabSentence::CommentCommented => {
            text.literal("a commenté ");
            text.slot(0);
        }
        CollabSentence::ReviewApproved => slot_then(text, 0, " a approuvé ces modifications"),
        CollabSentence::ReviewChangesRequested => slot_then(text, 0, " a demandé des modifications"),
        CollabSentence::ReviewDismissed => slot_then(text, 0, " a fait une revue (rejetée)"),
        CollabSentence::Reviewed => slot_then(text, 0, " a fait une revue"),
    }
}

fn slot_then(text: &mut RichWriter, slot: usize, words: &str) {
    text.slot(slot);
    text.literal(words);
}

/// Slot 0 holds `count`; the words agree with it.
fn counted(text: &mut RichWriter, count: u64, one: &str, other: &str) {
    text.slot(0);
    text.literal(match text.plural(count) {
        Plural::One => one,
        _ => other,
    });
}

/// « a, b, c ou d » over slots 0–3.
fn examples(text: &mut RichWriter) {
    text.slot(0);
    text.literal(", ");
    text.slot(1);
    text.literal(", ");
    text.slot(2);
    text.literal(" ou ");
    text.slot(3);
}

/// Under the title: the author (slot 0), when (slot 1) and the comment count.
fn opened(text: &mut RichWriter, verb: &str, comments: u64) {
    text.slot(0);
    text.literal(verb);
    text.slot(1);
    text.literal(" · ");
    text.number(comments);
    text.literal(match text.plural(comments) {
        Plural::One => " commentaire",
        _ => " commentaires",
    });
}

/// A list row: the number, what happened, when (slot 0) and by whom.
fn row(text: &mut RichWriter, number: &str, verb: &str, author: Option<&str>) {
    text.argument(number);
    text.literal(verb);
    text.slot(0);
    if let Some(author) = author {
        text.literal(" par ");
        text.argument(author);
    }
}

/// A pull request's header: the author, the commits, the base and the head (slots 0–3).
fn header(text: &mut RichWriter, verb: &str) {
    text.slot(0);
    text.literal(verb);
    text.slot(1);
    text.literal(" dans ");
    text.slot(2);
    text.literal(" depuis ");
    text.slot(3);
}
