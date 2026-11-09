//! German (`de`) entries for `catalog/repo/browse.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::RepoBrowseSentence;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};
use crate::plural::Plural;

entries! { De:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("Info"),
        RepoBrowseLabel::NoDescription => text.literal("Keine Beschreibung."),
        RepoBrowseLabel::Visibility => text.literal("Sichtbarkeit"),
        RepoBrowseLabel::History => text.literal("Verlauf"),
        RepoBrowseLabel::CommitHistory => text.literal("Commit-Verlauf"),
        RepoBrowseLabel::AddFile => text.literal("Datei hinzufügen"),
        RepoBrowseLabel::UploadFiles => text.literal("Dateien hochladen"),
        RepoBrowseLabel::Files => text.literal("Dateien"),
        RepoBrowseLabel::ParentDirectory => text.literal("Übergeordnetes Verzeichnis"),
        RepoBrowseLabel::Path => text.literal("Pfad"),
        RepoBrowseLabel::Latest => text.literal("Neueste"),
        RepoBrowseLabel::YourAccess => text.literal("Ihr Zugriff"),
        RepoBrowseLabel::LeaveRepository => text.literal("Repository verlassen"),
        RepoBrowseLabel::Languages => text.literal("Sprachen"),
        RepoBrowseLabel::RepositoryLinks => text.literal("Repository-Links"),
        RepoBrowseLabel::StoredWithLfs => text.literal("Mit Git LFS gespeichert"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("Objekt nicht auf diesem Server"),
        RepoBrowseLabel::EditThisFile => text.literal("Diese Datei bearbeiten"),
        RepoBrowseLabel::DeleteThisFile => text.literal("Diese Datei löschen"),
        RepoBrowseLabel::Blame => text.literal("Blame"),
        RepoBrowseLabel::Raw => text.literal("Roh"),
        RepoBrowseLabel::Download => text.literal("Herunterladen"),
        RepoBrowseLabel::BinaryNotShown => text.literal("Binärdatei wird nicht angezeigt."),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("Diese Datei ist zu groß für die Anzeige."),
        RepoBrowseLabel::ViewRaw => text.literal("Rohdatei anzeigen"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("Repository-Dokumente"),
        RepoBrowseLabel::DocumentOutline => text.literal("Gliederung"),
        RepoBrowseLabel::FilterHeadings => text.literal("Überschriften filtern"),
        RepoBrowseLabel::DocumentHeadings => text.literal("Überschriften des Dokuments"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("Keine passenden Überschriften."),
        RepoBrowseLabel::EmptyTitle => text.literal("Dieses Repository ist leer"),
        RepoBrowseLabel::EmptyLead => {
            text.literal("Pushen Sie ein bestehendes Projekt oder beginnen Sie ein neues über die Befehlszeile.");
        }
        RepoBrowseLabel::RepositoryUrl => text.literal("Repository-URL"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("Repository-URL kopieren"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("SSH-URL des Repositorys"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("SSH-URL des Repositorys kopieren"),
        RepoBrowseLabel::CreateNewRepository => text.literal("Neues Repository erstellen"),
        RepoBrowseLabel::PushExistingRepository => text.literal("Bestehendes Repository pushen"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("Dateiansicht schließen"),
        RepoBrowseLabel::LanguageFiles => text.literal("Dateien der Sprache"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("Keine Dateien für diese Sprache gefunden."),
        RepoBrowseLabel::NoMatchingFiles => text.literal("Keine passenden Dateien."),
        RepoBrowseLabel::FileContent => text.literal("Dateiinhalt"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("Wählen Sie eine Datei aus, um ihren Inhalt anzuzeigen."),
        RepoBrowseLabel::SearchThisListing => text.literal("Diese Liste durchsuchen"),
        RepoBrowseLabel::SearchNames => text.literal("Namen suchen"),
        RepoBrowseLabel::Search => text.literal("Suchen"),
        RepoBrowseLabel::ListingPages => text.literal("Seiten der Liste"),
        RepoBrowseLabel::Previous => text.literal("Zurück"),
        RepoBrowseLabel::Next => text.literal("Weiter"),
        RepoBrowseLabel::BranchesAndTags => text.literal("Branches und Tags"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("Branches und Tags durchsuchen"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("Branch oder Tag suchen"),
        RepoBrowseLabel::Find => text.literal("Suchen"),
        RepoBrowseLabel::Branch => text.literal("Branch"),
        RepoBrowseLabel::Tag => text.literal("Tag"),
        RepoBrowseLabel::Default => text.literal("Standard"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("Keine passenden Branches oder Tags."),
        RepoBrowseLabel::NoForksYet => text.literal("Noch keine Forks"),
        RepoBrowseLabel::ForksEmpty => text.literal(
            "Forks sind Kopien dieses Repositorys, an denen andere unabhängig arbeiten.",
        ),
        RepoBrowseLabel::ForkLead => text.literal(
            "Ein Fork ist Ihre eigene Kopie der Branches und Tags des Repositorys. Er übernimmt die Sichtbarkeit \
             des Originals.",
        ),
        RepoBrowseLabel::ForkLeadPrivate => text.literal(
            "Ein Fork ist Ihre eigene Kopie der Branches und Tags des Repositorys. Er übernimmt die Sichtbarkeit \
             des Originals – dieser bleibt privat.",
        ),
        RepoBrowseLabel::Owner => text.literal("Inhaber"),
        RepoBrowseLabel::Name => text.literal("Name"),
        RepoBrowseLabel::CreateFork => text.literal("Fork erstellen"),
        RepoBrowseLabel::Cancel => text.literal("Abbrechen"),
        RepoBrowseLabel::NoStargazersYet => text.literal("Noch keine Sterne"),
        RepoBrowseLabel::StargazersEmpty => {
            text.literal("Personen, die dieses Repository mit einem Stern markieren, erscheinen hier.");
        }
        RepoBrowseLabel::RepositoryActivity => text.literal("Repository-Aktivität"),
        RepoBrowseLabel::GoToFile => text.literal("Zu Datei springen"),
        RepoBrowseLabel::MatchingFiles => text.literal("Passende Dateien"),
        RepoBrowseLabel::LoadingFiles => text.literal("Dateien werden geladen…"),
        RepoBrowseLabel::UnableToLoadFiles => text.literal("Dateien konnten nicht geladen werden. Drücken Sie die Eingabetaste, um zu suchen."),
        RepoBrowseLabel::SeeAllMatches => text.literal("Alle Treffer anzeigen"),
        RepoBrowseLabel::NoFilesInRevision => text.literal("Diese Revision enthält keine Dateien."),
        RepoBrowseLabel::TooManyFilesToSearch => text.literal("Diese Revision enthält zu viele Dateien, um alle zu durchsuchen."),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "Branch",
            _ => "Branches",
        }),
        RepoBrowseMessage::TagNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "Tag",
            _ => "Tags",
        }),
        RepoBrowseMessage::CollaboratorAccess { permission } => {
            text.literal("Zugriff „");
            text.argument(permission);
            text.literal("“ als Mitarbeiter.");
        }
        RepoBrowseMessage::LineCount { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " Zeile",
                _ => " Zeilen",
            });
        }
        RepoBrowseMessage::LanguageFiles { language } => {
            text.argument(language);
            text.literal("-Dateien");
        }
        RepoBrowseMessage::ListingResults { total, page, pages } => {
            text.number(total as u64);
            text.literal(match text.plural(total as u64) {
                Plural::One => " Ergebnis · ",
                _ => " Ergebnisse · ",
            });
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
        }
        RepoBrowseMessage::RefPage { page, pages, total } => {
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
            text.literal(" · ");
            text.number(total as u64);
            text.literal(match text.plural(total as u64) {
                Plural::One => " Ref",
                _ => " Refs",
            });
        }
        RepoBrowseMessage::ForkTitle { owner, name } => {
            text.argument(owner);
            text.literal("/");
            text.argument(name);
            text.literal(" forken");
        }
        RepoBrowseMessage::CommitNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "Commit",
            _ => "Commits",
        }),
        RepoBrowseMessage::NoFileMatches { query } => {
            text.literal("Keine Datei passt zu „");
            text.argument(query);
            text.literal("“.");
        }
        RepoBrowseMessage::MoreFileMatches { count } => {
            text.literal("Weitere Treffer: ");
            text.number(count as u64);
            text.literal(". Geben Sie mehr vom Namen ein, um die Liste einzugrenzen.");
        }
    }
    RepoBrowseSentence => RichWriter |text| {
        RepoBrowseSentence::EmptySshNote => {
            text.literal("Die folgenden Befehle verwenden HTTPS. Um über SSH zu pushen, verwenden Sie stattdessen die SSH-URL und hinterlegen Sie einen Schlüssel in ");
            text.slot(0);
            text.literal(".");
        }
    }
}
