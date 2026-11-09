//! German (`de`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};
use crate::plural::Plural;

entries! { De:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("Neues Repository"),
        RepoCreateLabel::RepositoryType => text.literal("Art des Repositorys"),
        RepoCreateLabel::Empty => text.literal("Leer"),
        RepoCreateLabel::Mirror => text.literal("Spiegel"),
        RepoCreateLabel::Import => text.literal("Import"),
        RepoCreateLabel::NewLead => text.literal(
            "Erstellen Sie ein leeres Repository, importieren Sie ein Projekt oder halten Sie einen Spiegel mit \
             seiner Quelle synchron.",
        ),
        RepoCreateLabel::Owner => text.literal("Inhaber"),
        RepoCreateLabel::Name => text.literal("Name"),
        RepoCreateLabel::Description => text.literal("Beschreibung"),
        RepoCreateLabel::Optional => text.literal("optional"),
        RepoCreateLabel::Visibility => text.literal("Sichtbarkeit"),
        RepoCreateLabel::Public => text.literal("Öffentlich"),
        RepoCreateLabel::Private => text.literal("Privat"),
        RepoCreateLabel::PublicRepository => {
            text.literal("Jeder kann dieses Repository sehen und klonen.");
        }
        RepoCreateLabel::PublicMirror => text.literal("Jeder kann den Spiegel sehen und klonen."),
        RepoCreateLabel::PublicImport => text.literal("Jeder kann das Repository sehen und klonen."),
        RepoCreateLabel::PrivateHint => text.literal(
            "Nur der Inhaber kann es sehen. Bei einer Organisation sind das ihre Mitglieder.",
        ),
        RepoCreateLabel::Cancel => text.literal("Abbrechen"),
        RepoCreateLabel::CreateRepository => text.literal("Repository erstellen"),
        RepoCreateLabel::MirrorLead => text.literal(
            "Ein Spiegel ruft die Branches und Tags eines öffentlichen Repositorys an anderer Stelle regelmäßig \
             ab. Hier ist er schreibgeschützt: Änderungen kommen aus der Quelle.",
        ),
        RepoCreateLabel::MirrorsDisabled => text.literal("Spiegel sind deaktiviert"),
        RepoCreateLabel::MirrorsDisabledText => text.literal(
            "Ein Administrator dieser Instanz hat Pull-Spiegel deaktiviert.",
        ),
        RepoCreateLabel::UpstreamUrl => text.literal("Quell-URL"),
        RepoCreateLabel::UrlHintStart => text.literal("Eine mit"),
        RepoCreateLabel::UrlHintOr => text.literal("oder"),
        RepoCreateLabel::UrlHintEnd => text.literal("beginnende URL eines öffentlichen Repositorys."),
        RepoCreateLabel::FromTheUrl => text.literal("aus der URL"),
        RepoCreateLabel::FetchEvery => text.literal("Abrufen alle"),
        RepoCreateLabel::Minutes => text.literal("Minuten"),
        RepoCreateLabel::CreateMirror => text.literal("Spiegel erstellen"),
        RepoCreateLabel::MirrorARepository => text.literal("Repository spiegeln"),
        RepoCreateLabel::ImportLead => text.literal(
            "Kopiert ein Repository von GitHub oder Gitea mit seinem Verlauf und auf Wunsch mit Issues, \
             Pull-Requests, Labels, Meilensteinen, Releases und Git-LFS-Dateien. Die Nummern von Issues und \
             Pull-Requests bleiben erhalten.",
        ),
        RepoCreateLabel::ImportingDisabled => text.literal("Importieren ist deaktiviert"),
        RepoCreateLabel::ImportingDisabledText => text.literal(
            "Ein Administrator dieser Instanz hat das Importieren deaktiviert.",
        ),
        RepoCreateLabel::ImportFrom => text.literal("Importieren von"),
        RepoCreateLabel::ServerAddress => text.literal("Serveradresse"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("für github.com nicht nötig"),
        RepoCreateLabel::Repository => text.literal("Repository"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("inhaber/name oder die URL"),
        RepoCreateLabel::AccessToken => text.literal("Zugriffstoken"),
        RepoCreateLabel::OptionalForPublic => text.literal("optional für öffentliche Repositorys"),
        RepoCreateLabel::TokenHint => text.literal(
            "Ein Token mit Lesezugriff auf das Repository erhöht die Ratenlimits der Quelle und ist für private \
             Repositorys erforderlich. Es wird nur während des Imports verwendet und nie gespeichert.",
        ),
        RepoCreateLabel::AsOnTheSource => text.literal("wie in der Quelle"),
        RepoCreateLabel::BringAlong => text.literal("Mitnehmen"),
        RepoCreateLabel::Wiki => text.literal("Wiki"),
        RepoCreateLabel::WikiNotSupported => text.literal("Nicht unterstützt: Diese Instanz hat keine Wikis."),
        RepoCreateLabel::StartImport => text.literal("Import starten"),
        RepoCreateLabel::ImportARepository => text.literal("Repository importieren"),
        RepoCreateLabel::ImportTitle => text.literal("Import"),
        RepoCreateLabel::FailedNote => text.literal("Alles bis dahin Importierte ist im Repository."),
        RepoCreateLabel::NotReached => text.literal("Nicht erreicht"),
        RepoCreateLabel::Started => text.literal("Gestartet"),
        RepoCreateLabel::WaitingForSlot => text.literal("Warten auf einen freien Platz"),
        RepoCreateLabel::Ended => text.literal("Beendet"),
        RepoCreateLabel::UpdatesByItself => text.literal("Diese Seite aktualisiert sich selbst."),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("Übersprungen oder unvollständig"),
        RepoCreateLabel::GoToRepository => text.literal("Zum Repository"),
        RepoCreateLabel::OpensAutomatically => text.literal(
            "Das Repository öffnet sich automatisch, sobald der erste Abruf des Spiegels abgeschlossen ist.",
        ),
        RepoCreateLabel::FetchInProgress => text.literal("Spiegel wird abgerufen"),
        RepoCreateLabel::TransferSizeNote => text.literal(
            "Die Übertragungsgröße hängt von der Quelle ab. Diese Seite zeigt den aktuellen Schritt, während die \
             Arbeit fortschreitet.",
        ),
        RepoCreateLabel::RefreshStatus => text.literal("Status aktualisieren"),
        RepoCreateLabel::SettingsAndRetry => text.literal("Spiegel-Einstellungen und Wiederholen"),
        RepoCreateLabel::NoScriptHint => text.literal(
            "Prüfen Sie den Fortschritt mit „Status aktualisieren“ und öffnen Sie das Repository, sobald es \
             bereit ist.",
        ),
        RepoCreateLabel::Connecting => text.literal("Verbindung zum Quell-Repository wird hergestellt."),
        RepoCreateLabel::Fetching => text.literal(
            "Branches, Tags und Git-Objekte werden heruntergeladen. Große Repositorys können mehrere Minuten \
             dauern.",
        ),
        RepoCreateLabel::Finalizing => {
            text.literal("Inhalt des Repositorys und Standardbranch werden vorbereitet.");
        }
        RepoCreateLabel::FetchFailedRetry => text.literal(
            "Der Spiegel konnte nicht abgerufen werden. Ein Administrator des Repositorys kann es in den \
             Spiegel-Einstellungen erneut versuchen.",
        ),
        RepoCreateLabel::Complete => text.literal("Spiegeln abgeschlossen. Repository wird geöffnet…"),
        RepoCreateLabel::Queued => text.literal("Warten auf einen freien Spiegel-Worker."),
        RepoCreateLabel::Stopped => text.literal("Spiegeln wurde beendet. Repository wird geöffnet…"),
        RepoCreateLabel::Mirroring => text.literal("Spiegeln"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes } => {
            text.literal("Mindestens ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " Minute.",
                _ => " Minuten.",
            });
        }
        RepoCreateMessage::ImportFrom { source } => {
            text.literal("Von ");
            text.argument(source);
            text.literal(":");
        }
        RepoCreateMessage::MoreWarnings { count } => {
            text.literal("und ");
            text.number(count);
            text.literal(" weitere (siehe Serverprotokoll)");
        }
        RepoCreateMessage::MirroringName { name } => {
            text.argument(name);
            text.literal(" wird gespiegelt");
        }
        RepoCreateMessage::FetchFailed { error } => {
            text.literal("Der Spiegel konnte nicht abgerufen werden: ");
            text.argument(error);
        }
    }
}
