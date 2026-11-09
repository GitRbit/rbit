//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;
use crate::plural::Plural;

entries! { PtBr:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { count } => {
            text.slot(0);
            text.literal(match text.plural(count) {
                Plural::One => " aberto",
                _ => " abertos",
            });
        }
        CollabSentence::ClosedCount { count } => {
            text.slot(0);
            text.literal(match text.plural(count) {
                Plural::One => " fechado",
                _ => " fechados",
            });
        }
        CollabSentence::IssueSearchHelp => {
            text.literal("Use os menus para filtrar os resultados ou digite termos como ");
            examples(text);
            text.literal(". O texto pesquisa títulos e descrições. Cada menu mantém os seus outros filtros.");
        }
        CollabSentence::PullSearchHelp => {
            text.literal("Escolha os filtros abaixo ou digite termos como ");
            examples(text);
            text.literal(".");
        }
        CollabSentence::OpenedIssue { comments } => opened(text, " abriu esta issue ", comments),
        CollabSentence::OpenedPull { comments } => opened(text, " abriu este pull request ", comments),
        CollabSentence::RowOpened { number, author } => row(text, number, " aberto ", Some(author)),
        CollabSentence::RowClosed { number } => row(text, number, " fechado ", None),
        CollabSentence::RowClosedBy { number, author } => row(text, number, " fechado ", Some(author)),
        CollabSentence::RowMergedBy { number, author } => row(text, number, " mesclado ", Some(author)),
        CollabSentence::NoOneAssignYourself => {
            text.literal("Ninguém — ");
            text.slot(0);
        }
        CollabSentence::PercentComplete => slot_then(text, 0, " concluído"),
        CollabSentence::WantsToMerge => header(text, " quer mesclar "),
        CollabSentence::MergedInto => header(text, " mesclou "),
        CollabSentence::ProposedToMerge => header(text, " propôs mesclar "),
        CollabSentence::SignInToOpenPull => slot_then(text, 0, " para abrir um pull request."),
        CollabSentence::SignInToCommentOnLines => slot_then(text, 0, " para comentar em linhas."),
        CollabSentence::SignInToJoin => slot_then(text, 0, " para participar da conversa."),
        CollabSentence::PullMergedAs => {
            text.literal("Pull request mesclado como ");
            text.slot(0);
            text.literal(".");
        }
        CollabSentence::FilesChanged { count } => {
            text.slot(0);
            text.literal(match text.plural(count) {
                Plural::One => " arquivo alterado",
                _ => " arquivos alterados",
            });
        }
        CollabSentence::CommentOpened => {
            text.literal("abriu ");
            text.slot(0);
        }
        CollabSentence::CommentCommented => {
            text.literal("comentou ");
            text.slot(0);
        }
        CollabSentence::ReviewApproved => slot_then(text, 0, " aprovou estas alterações"),
        CollabSentence::ReviewChangesRequested => slot_then(text, 0, " solicitou alterações"),
        CollabSentence::ReviewDismissed => slot_then(text, 0, " revisou (descartada)"),
        CollabSentence::Reviewed => slot_then(text, 0, " revisou"),
    }
}

fn slot_then(text: &mut RichWriter, slot: usize, words: &str) {
    text.slot(slot);
    text.literal(words);
}

/// “a, b, c ou d” over slots 0–3.
fn examples(text: &mut RichWriter) {
    text.slot(0);
    text.literal(", ");
    text.slot(1);
    text.literal(", ");
    text.slot(2);
    text.literal(" ou ");
    text.slot(3);
}

/// “alice abriu esta issue há 2 dias · 3 comentários”.
fn opened(text: &mut RichWriter, words: &str, comments: u64) {
    text.slot(0);
    text.literal(words);
    text.slot(1);
    text.literal(" · ");
    text.number(comments);
    text.literal(match text.plural(comments) {
        Plural::One => " comentário",
        _ => " comentários",
    });
}

/// “#12 aberto há 2 dias por alice”.
fn row(text: &mut RichWriter, number: &str, words: &str, author: Option<&str>) {
    text.argument(number);
    text.literal(words);
    text.slot(0);
    if let Some(author) = author {
        text.literal(" por ");
        text.argument(author);
    }
}

/// “alice quer mesclar 3 commits em main a partir de feature”.
fn header(text: &mut RichWriter, words: &str) {
    text.slot(0);
    text.literal(words);
    text.slot(1);
    text.literal(" em ");
    text.slot(2);
    text.literal(" a partir de ");
    text.slot(3);
}
