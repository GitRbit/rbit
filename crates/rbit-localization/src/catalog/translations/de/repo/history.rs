//! German (`de`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};
use crate::plural::Plural;

entries! { De:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("Dateien in diesem Commit durchsuchen"),
        RepoHistoryLabel::BrowseFiles => text.literal("Dateien durchsuchen"),
        RepoHistoryLabel::Pagination => text.literal("Seitennavigation"),
        RepoHistoryLabel::Newer => text.literal("Neuer"),
        RepoHistoryLabel::Older => text.literal("Älter"),
        RepoHistoryLabel::Committed => text.literal("committete"),
        RepoHistoryLabel::CommittedBy => text.literal("committet von"),
        RepoHistoryLabel::Commit => text.literal("Commit"),
        RepoHistoryLabel::Checks => text.literal("Prüfungen"),
        RepoHistoryLabel::ViewFile => text.literal("Datei anzeigen"),
        RepoHistoryLabel::PathInThatCommit => text.literal("Pfad in diesem Commit"),
        RepoHistoryLabel::FileEmpty => text.literal("Diese Datei ist leer."),
        RepoHistoryLabel::BinaryCannotBeBlamed => text.literal("Für Binärdateien gibt es kein Blame."),
        RepoHistoryLabel::TooLargeToBlame => text.literal("Diese Datei ist zu groß für Blame."),
        RepoHistoryLabel::ViewTheFile => text.literal("Datei anzeigen"),
        RepoHistoryLabel::CompareChanges => text.literal("Änderungen vergleichen"),
        RepoHistoryLabel::CompareLead => text.literal(
            "Wählen Sie eine Basis und den Branch, Tag oder Commit, der damit verglichen werden soll – auch über \
             Forks hinweg.",
        ),
        RepoHistoryLabel::Comparison => text.literal("Vergleich"),
        RepoHistoryLabel::MergeBase => text.literal("Merge-Basis"),
        RepoHistoryLabel::Swap => text.literal("Tauschen"),
        RepoHistoryLabel::CreatePullRequest => text.literal("Pull-Request erstellen"),
        RepoHistoryLabel::UnrelatedHistories => text.literal(
            "Diese Revisionen haben völlig unterschiedliche Verläufe, daher gibt es keinen Diff.",
        ),
        RepoHistoryLabel::NothingToCompare => text.literal("Nichts zu vergleichen"),
        RepoHistoryLabel::NothingToCompareText => text.literal(
            "Alles aus der verglichenen Revision ist bereits in der Basis enthalten.",
        ),
        RepoHistoryLabel::Commits => text.literal("Commits"),
        RepoHistoryLabel::Compare => text.literal("Vergleichen"),
        RepoHistoryLabel::CompareWithDefault => text.literal("Mit dem Standardbranch vergleichen"),
        RepoHistoryLabel::BranchView => text.literal("Branch-Ansicht"),
        RepoHistoryLabel::List => text.literal("Liste"),
        RepoHistoryLabel::GitTree => text.literal("Git-Baum"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("Keine passenden Refs."),
        RepoHistoryLabel::NoBranchesYet => text.literal("Noch keine Branches"),
        RepoHistoryLabel::NoBranchesText => text.literal("Pushen Sie einen Commit, um den ersten Branch zu erstellen."),
        RepoHistoryLabel::NoTagsYet => text.literal("Noch keine Tags"),
        RepoHistoryLabel::NoTagsText => text.literal(
            "Tags markieren Releases und andere wichtige Punkte im Verlauf.",
        ),
        RepoHistoryLabel::Protected => text.literal("Geschützt"),
        RepoHistoryLabel::ProtectedTitle => text.literal("Für diesen Branch gilt eine Branch-Schutzregel"),
        RepoHistoryLabel::DownloadTarGz => text.literal("tar.gz herunterladen"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("Commit-Graph der Branches"),
        RepoHistoryLabel::GraphHelp => text.literal(
            "Folgen Sie den farbigen Linien, um den Verlauf nachzuvollziehen. Wählen Sie eine Spur aus, um ihre \
             Commits hervorzuheben.",
        ),
        RepoHistoryLabel::ClearHighlight => text.literal("Hervorhebung aufheben"),
        RepoHistoryLabel::HistoryConnections => text.literal("Verbindungen im Verlauf"),
        RepoHistoryLabel::CommitDetails => text.literal("Commit-Details"),
        RepoHistoryLabel::Merge => text.literal("Merge"),
        RepoHistoryLabel::GraphTruncated => text.literal(
            "Angezeigt werden die neuesten 200 Commits aller Branches. Linien am unteren Rand führen in älteren \
             Verlauf weiter. Öffnen Sie den Verlauf eines Branches, um weiter zu suchen.",
        ),
        RepoHistoryLabel::LinesAdded => text.literal("Hinzugefügt"),
        RepoHistoryLabel::LinesDeleted => text.literal("Gelöscht"),
        RepoHistoryLabel::StatusAdded => text.literal("Hinzugefügt"),
        RepoHistoryLabel::StatusDeleted => text.literal("Gelöscht"),
        RepoHistoryLabel::StatusModified => text.literal("Geändert"),
        RepoHistoryLabel::StatusRenamed => text.literal("Umbenannt"),
        RepoHistoryLabel::StatusCopied => text.literal("Kopiert"),
        RepoHistoryLabel::ChangedFiles => text.literal("Geänderte Dateien"),
        RepoHistoryLabel::FileTree => text.literal("Dateibaum"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("Geänderte Dateien filtern"),
        RepoHistoryLabel::FilterFiles => text.literal("Dateien filtern…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("Keine passenden Dateien."),
        RepoHistoryLabel::Changes => text.literal("Änderungen"),
        RepoHistoryLabel::ToggleFileTree => text.literal("Dateibaum ein- oder ausblenden"),
        RepoHistoryLabel::CollapseAll => text.literal("Alle einklappen"),
        RepoHistoryLabel::ExpandAll => text.literal("Alle ausklappen"),
        RepoHistoryLabel::DiffTruncated => text.literal(
            "Diese Änderung ist sehr groß, daher werden einige Diffs nicht angezeigt. Klonen Sie das Repository, \
             um die vollständige Änderung zu sehen.",
        ),
        RepoHistoryLabel::CollapseHint => {
            text.literal("Klicken zum Ein- oder Ausklappen · Alt-Klick für alle Dateien");
        }
        RepoHistoryLabel::CopyPath => text.literal("Pfad kopieren"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("Datei in diesem Commit anzeigen"),
        RepoHistoryLabel::View => text.literal("Anzeigen"),
        RepoHistoryLabel::BinaryNotShown => text.literal("Binärdatei wird nicht angezeigt."),
        RepoHistoryLabel::DiffTooLarge => text.literal("Diff zu groß für die Anzeige."),
        RepoHistoryLabel::SubmoduleCommit => text.literal("Submodul-Commit"),
        RepoHistoryLabel::NoContentChanges => text.literal("Keine inhaltlichen Änderungen."),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date } => {
            text.literal("Commits am ");
            text.argument(date);
        }
        RepoHistoryMessage::ViewCommit { id } => {
            text.literal("Commit ");
            text.argument(id);
            text.literal(" anzeigen");
        }
        RepoHistoryMessage::ParentNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "Eltern-Commit",
            _ => "Eltern-Commits",
        }),
        RepoHistoryMessage::BlameOf { file } => {
            text.literal("Blame von ");
            text.argument(file);
        }
        RepoHistoryMessage::BlameTitle { path } => {
            text.literal("Blame ");
            text.argument(path);
        }
        RepoHistoryMessage::Ahead { count } => {
            text.argument(count);
            text.literal(" voraus");
        }
        RepoHistoryMessage::Behind { count } => {
            text.argument(count);
            text.literal(" zurück");
        }
        RepoHistoryMessage::OnlyNewest { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "Nur der neueste ",
                _ => "Nur die neuesten ",
            });
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " Commit wird aufgeführt.",
                _ => " Commits werden aufgeführt.",
            });
        }
        RepoHistoryMessage::HighlightTrack { track } => {
            text.literal("Spur ");
            text.number(track as u64);
            text.literal(" hervorheben");
        }
        RepoHistoryMessage::Track { track } => {
            text.literal("Spur ");
            text.number(track as u64);
        }
        RepoHistoryMessage::BranchName { name } => {
            text.literal("Branch: ");
            text.argument(name);
        }
        RepoHistoryMessage::MoreBranches { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " weiterer Branch",
                _ => " weitere Branches",
            });
        }
        RepoHistoryMessage::FilesChangedNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "Datei geändert",
            _ => "Dateien geändert",
        }),
        RepoHistoryMessage::CompareTitle { base, head } => {
            text.argument(base);
            text.literal("...");
            text.argument(head);
            text.literal(" vergleichen");
        }
    }
}
