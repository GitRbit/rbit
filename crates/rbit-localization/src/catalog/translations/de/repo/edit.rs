//! German (`de`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};
use crate::plural::Plural;

entries! { De:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("Neue Datei"),
        RepoEditLabel::Edit => text.literal("Bearbeiten"),
        RepoEditLabel::AfterName => text.literal(". "),
        RepoEditLabel::On => text.literal("Auf "),
        RepoEditLabel::NewFileHint => text.literal("Geben Sie einen Pfad mit / ein, um Verzeichnisse anzulegen."),
        RepoEditLabel::EditHint => {
            text.literal("Ändern Sie den Pfad, um die Datei umzubenennen oder zu verschieben.");
        }
        RepoEditLabel::BackToFile => text.literal("Zurück zur Datei"),
        RepoEditLabel::Path => text.literal("Pfad"),
        RepoEditLabel::Content => text.literal("Inhalt"),
        RepoEditLabel::CommitChanges => text.literal("Änderungen committen"),
        RepoEditLabel::Cancel => text.literal("Abbrechen"),
        RepoEditLabel::Delete => text.literal("Löschen"),
        RepoEditLabel::DeleteHint => text.literal(
            "Die Datei bleibt im Verlauf; ein dadurch leeres Verzeichnis verschwindet.",
        ),
        RepoEditLabel::DeleteFile => text.literal("Datei löschen"),
        RepoEditLabel::UploadFiles => text.literal("Dateien hochladen"),
        RepoEditLabel::Into => text.literal("Nach "),
        RepoEditLabel::OnBranch => text.literal(" auf "),
        RepoEditLabel::UploadHint => text.literal("Dateien mit demselben Namen werden ersetzt."),
        RepoEditLabel::Files => text.literal("Dateien"),
        RepoEditLabel::NothingCommitted => text.literal("Es wurde nichts committet. "),
        RepoEditLabel::ReviewNewCommits => text.literal("Prüfen Sie die neuen Commits"),
        RepoEditLabel::KeptAndCommittedOnTop => text.literal(
            ", und committen Sie dann erneut: Ihre Änderungen bleiben unten erhalten und werden auf die neue \
             Version committet.",
        ),
        RepoEditLabel::BranchGone => text.literal(
            "Es wurde nichts committet. Committen Sie stattdessen auf einen neuen Branch; Ihre Änderungen bleiben \
             unten erhalten.",
        ),
        RepoEditLabel::CommitChangesLegend => text.literal("Änderungen committen"),
        RepoEditLabel::CommitMessage => text.literal("Commit-Nachricht"),
        RepoEditLabel::ExtendedDescription => text.literal("Ausführliche Beschreibung"),
        RepoEditLabel::Optional => text.literal("optional"),
        RepoEditLabel::CommitDirectlyTo => text.literal("Direkt committen auf"),
        RepoEditLabel::NotAllowedEnd => text.literal("."),
        RepoEditLabel::NotAllowed => text.literal("Nicht erlaubt: "),
        RepoEditLabel::NewBranchAndPull => {
            text.literal("Neuen Branch erstellen und Pull-Request starten");
        }
        RepoEditLabel::NewBranchName => text.literal("Name des neuen Branches"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size } => {
            text.literal("Höchstens ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size } => {
            text.literal("Bis zu ");
            text.number(files as u64);
            text.literal(match text.plural(files as u64) {
                Plural::One => " Datei mit höchstens ",
                _ => " Dateien mit je höchstens ",
            });
            text.argument(size);
            text.literal(", gemeinsam committet.");
        }
        RepoEditMessage::BranchChanged { branch } => {
            text.argument(branch);
            text.literal(" hat sich geändert, seit Sie begonnen haben");
        }
        RepoEditMessage::BranchMissing { branch } => {
            text.argument(branch);
            text.literal(" existiert nicht mehr");
        }
        RepoEditMessage::NowPointsAt => text.literal("Er zeigt jetzt auf "),
        RepoEditMessage::NotEditable { limit } => {
            text.literal("Diese Datei ist binär oder größer als ");
            text.argument(limit);
            text.literal(
                " und kann daher nicht im Browser bearbeitet werden. Sie können sie löschen oder einen Ersatz \
                 hochladen.",
            );
        }
        RepoEditMessage::EditingTitle { path } => {
            text.argument(path);
            text.literal(" bearbeiten");
        }
        RepoEditMessage::DeleteTitle { path } => {
            text.argument(path);
            text.literal(" löschen");
        }
    }
}
