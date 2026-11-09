//! German (`de`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::PullRequests => text.literal("Pull-Requests"),
        PullMessage::SearchPulls => text.literal("Pull-Requests durchsuchen"),
        PullMessage::ClearSearch => text.literal("Suche und Filter zurücksetzen"),
        PullMessage::FiltersLabel => text.literal("Pull-Request-Filter"),
        PullMessage::StateNav => text.literal("Pull-Request-Zustand"),
        PullMessage::AuthorFilter { author } => {
            text.literal("Autor: ");
            text.argument(author);
        }
        PullMessage::New => text.literal("Neuer"),
        PullMessage::NewSuffix => text.literal("\u{a0}Pull-Request"),
        PullMessage::EmptyTitle => text.literal("Keine Pull-Requests vorhanden"),
        PullMessage::EmptyText => text.literal(
            "Pull-Requests schlagen Änderungen aus einem Branch oder Fork vor. Vergleichen Sie Branches, um einen zu eröffnen.",
        ),
        PullMessage::OpenTitle => text.literal("Pull-Request eröffnen"),
        PullMessage::OpenIntro => text.literal(
            "Schlagen Sie die Änderungen eines Branches aus diesem Repository oder einem Fork für den Ziel-Branch vor.",
        ),
        PullMessage::OpenAsDraft => text.literal("Als Entwurf eröffnen"),
        PullMessage::Create => text.literal("Pull-Request erstellen"),
        PullMessage::DifferentHistories => text.literal("Diese Branches haben völlig verschiedene Historien."),
        PullMessage::ChooseBranches => text.literal("Wählen Sie zwei vorhandene Branches zum Vergleichen."),
        PullMessage::UpToDate { base, head } => {
            text.argument(base);
            text.literal(" enthält bereits alle Commits aus ");
            text.argument(head);
            text.literal(".");
        }
        PullMessage::BaseRepository => text.literal("Ziel-Repository"),
        PullMessage::BaseRefs => text.literal("Ziel-Branches und -Tags"),
        PullMessage::Base => text.literal("Ziel"),
        PullMessage::HeadRepository => text.literal("Quell-Repository"),
        PullMessage::Compare => text.literal("Vergleichen"),
        PullMessage::CompareRefs => text.literal("Branches und Tags vergleichen"),
        PullMessage::PullRequest => text.literal("Pull-Request"),
        PullMessage::Conversation => text.literal("Unterhaltung"),
        PullMessage::Commits => text.literal("Commits"),
        PullMessage::FilesChanged => text.literal("Geänderte Dateien"),
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Commit",
                _ => " Commits",
            });
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("Commits"),
        PullMessage::ChangeBase => text.literal("Ziel-Branch ändern"),
        PullMessage::BaseBranch => text.literal("Ziel-Branch"),
        PullMessage::BaseBranches => text.literal("Ziel-Branches"),
        PullMessage::ChangeBaseHint => {
            text.literal("Ein neuer Ziel-Branch aktualisiert die Commits, den Diff und die Merge-Prüfungen.");
        }
        PullMessage::UpdateBase => text.literal("Ziel-Branch aktualisieren"),
        PullMessage::IsClosed => text.literal("Dieser Pull-Request ist geschlossen."),
        PullMessage::IsMerged => text.literal("Pull-Request zusammengeführt."),
        PullMessage::DeleteBranch { branch } => {
            text.argument(branch);
            text.literal(" löschen");
        }
        PullMessage::ConvertToDraft => text.literal("In Entwurf umwandeln"),
        PullMessage::NoCommitsTitle => text.literal("Keine Commits"),
        PullMessage::NoCommitsText => text.literal("Quell- und Ziel-Branch zeigen auf denselben Commit."),
        PullMessage::NewestCommits { count } => {
            text.literal("Nur ");
            text.literal(match text.plural(count) {
                Plural::One => "der neueste ",
                _ => "die neuesten ",
            });
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Commit wird angezeigt.",
                _ => " Commits werden angezeigt.",
            });
        }
    }
}
