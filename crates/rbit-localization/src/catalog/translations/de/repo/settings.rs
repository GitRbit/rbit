//! German (`de`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};
use crate::plural::Plural;

entries! { De:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("Einstellungen"),
        RepoSettingsLabel::AdminsOf => text.literal("Nur Administratoren von"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("können diese Seite sehen."),
        RepoSettingsLabel::SettingsSaved => text.literal("Einstellungen gespeichert."),
        RepoSettingsLabel::General => text.literal("Allgemein"),
        RepoSettingsLabel::Description => text.literal("Beschreibung"),
        RepoSettingsLabel::Optional => text.literal("optional"),
        RepoSettingsLabel::Visibility => text.literal("Sichtbarkeit"),
        RepoSettingsLabel::Public => text.literal("Öffentlich"),
        RepoSettingsLabel::PublicHint => text.literal("Jeder kann dieses Repository sehen und klonen."),
        RepoSettingsLabel::Private => text.literal("Privat"),
        RepoSettingsLabel::PrivateHint => text.literal("Nur Personen mit Zugriff können es sehen und klonen."),
        RepoSettingsLabel::DefaultBranch => text.literal("Standardbranch"),
        RepoSettingsLabel::PushABranch => {
            text.literal("Pushen Sie einen Branch, um den Standardbranch auszuwählen.");
        }
        RepoSettingsLabel::DefaultBranchHint => text.literal(
            "Wird beim Durchsuchen zuerst angezeigt und von git clone ausgecheckt.",
        ),
        RepoSettingsLabel::SaveChanges => text.literal("Änderungen speichern"),
        RepoSettingsLabel::RepositoryName => text.literal("Name des Repositorys"),
        RepoSettingsLabel::Name => text.literal("Name"),
        RepoSettingsLabel::RenameHint => text.literal(
            "Links und Klon-URLs mit dem alten Namen funktionieren dann nicht mehr.",
        ),
        RepoSettingsLabel::Rename => text.literal("Umbenennen"),
        RepoSettingsLabel::DangerZone => text.literal("Gefahrenbereich"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("Archivierung dieses Repositorys aufheben"),
        RepoSettingsLabel::UnarchiveHint => text.literal(
            "Es nimmt wieder Pushes, Issues, Pull-Requests, Kommentare und Releases an, und seine Spiegel werden \
             fortgesetzt.",
        ),
        RepoSettingsLabel::Unarchive => text.literal("Archivierung aufheben"),
        RepoSettingsLabel::ArchiveTitle => text.literal("Dieses Repository archivieren"),
        RepoSettingsLabel::ArchiveHint => text.literal(
            "Es wird schreibgeschützt: Es kann weiterhin geklont, geforkt und mit einem Stern markiert werden, \
             aber niemand kann pushen, zusammenführen oder Issues, Pull-Requests und Releases ändern, und seine \
             Spiegel pausieren. Sie können die Archivierung jederzeit aufheben.",
        ),
        RepoSettingsLabel::Archive => text.literal("Repository archivieren"),
        RepoSettingsLabel::DeleteTitle => text.literal("Dieses Repository löschen"),
        RepoSettingsLabel::DeleteHint => text.literal(
            "Seine Dateien, sein Verlauf und seine Sterne werden endgültig entfernt. Dies kann nicht rückgängig \
             gemacht werden.",
        ),
        RepoSettingsLabel::Type => text.literal("Geben Sie"),
        RepoSettingsLabel::ToConfirm => text.literal("zur Bestätigung ein"),
        RepoSettingsLabel::DeleteRepository => text.literal("Repository löschen"),
        RepoSettingsLabel::DeployKeys => text.literal("Deploy-Schlüssel"),
        RepoSettingsLabel::DeployKeysHint => text.literal(
            "Schlüssel, die einem Server oder CI-Job SSH-Zugriff nur auf dieses Repository geben.",
        ),
        RepoSettingsLabel::DeployKeysLeadStart => text.literal(
            "Ein Deploy-Schlüssel gibt einem Server oder CI-Job SSH-Zugriff ausschließlich auf",
        ),
        RepoSettingsLabel::DeployKeysLeadEnd => text.literal(
            "– Schlüssel sind schreibgeschützt, sofern Sie ihnen nicht das Pushen erlauben.",
        ),
        RepoSettingsLabel::NewDeployKey => text.literal("Neuer Deploy-Schlüssel"),
        RepoSettingsLabel::Title => text.literal("Titel"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("z. B. Produktionsserver"),
        RepoSettingsLabel::TitleHint => text.literal("Standardmäßig der Kommentar des Schlüssels."),
        RepoSettingsLabel::Key => text.literal("Schlüssel"),
        RepoSettingsLabel::KeyHint => text.literal(
            "Ein Schlüssel kann nur zu einem Repository oder Konto gehören; erzeugen Sie für jedes einen \
             eigenen Schlüssel.",
        ),
        RepoSettingsLabel::AllowWrite => text.literal("Schreibzugriff erlauben"),
        RepoSettingsLabel::AllowWriteHint => text.literal(
            "Der Schlüssel kann in dieses Repository pushen, nicht nur es klonen.",
        ),
        RepoSettingsLabel::AddDeployKey => text.literal("Deploy-Schlüssel hinzufügen"),
        RepoSettingsLabel::Keys => text.literal("Schlüssel"),
        RepoSettingsLabel::NoDeployKeys => text.literal("Keine Deploy-Schlüssel"),
        RepoSettingsLabel::NoDeployKeysText => text.literal(
            "Fügen Sie einen Schlüssel hinzu, damit ein Server oder CI-Job dieses Repository über SSH klonen kann.",
        ),
        RepoSettingsLabel::Delete => text.literal("Löschen"),
        RepoSettingsLabel::Mirror => text.literal("Spiegel"),
        RepoSettingsLabel::MirrorHint => text.literal(
            "Quelle, Abrufintervall und Ergebnis des letzten Abrufs.",
        ),
        RepoSettingsLabel::MirrorLead => text.literal(
            "Dieses Repository ruft die Branches und Tags seiner Quelle ab und nimmt keine Pushes an.",
        ),
        RepoSettingsLabel::Upstream => text.literal("Quelle"),
        RepoSettingsLabel::LastFetch => text.literal("Letzter Abruf"),
        RepoSettingsLabel::Failed => text.literal("Fehlgeschlagen"),
        RepoSettingsLabel::Ok => text.literal("OK"),
        RepoSettingsLabel::NotYet => text.literal("Noch nicht"),
        RepoSettingsLabel::NextFetch => text.literal("Nächster Abruf"),
        RepoSettingsLabel::Error => text.literal("Fehler"),
        RepoSettingsLabel::FetchNow => text.literal("Jetzt abrufen"),
        RepoSettingsLabel::FetchEvery => text.literal("Abrufen alle"),
        RepoSettingsLabel::Minutes => text.literal("Minuten"),
        RepoSettingsLabel::Save => text.literal("Speichern"),
        RepoSettingsLabel::Saved => text.literal("Gespeichert."),
        RepoSettingsLabel::FetchQueued => text.literal(
            "Ein Abruf ist eingeplant; laden Sie die Seite gleich neu, um das Ergebnis zu sehen.",
        ),
        RepoSettingsLabel::PushQueued => text.literal(
            "Ein Push ist eingeplant; laden Sie die Seite gleich neu, um das Ergebnis zu sehen.",
        ),
        RepoSettingsLabel::StopMirroring => text.literal("Spiegeln beenden"),
        RepoSettingsLabel::StopHint => text.literal(
            "Das Repository behält seinen Inhalt, ruft nichts mehr ab und nimmt Pushes wie jedes andere an.",
        ),
        RepoSettingsLabel::PushMirrors => text.literal("Push-Spiegel"),
        RepoSettingsLabel::PushMirrorsHint => text.literal(
            "Remotes, die nach jedem Push die Branches und Tags dieses Repositorys erhalten.",
        ),
        RepoSettingsLabel::PushMirrorsLead => text.literal(
            "Remotes, die die Branches und Tags dieses Repositorys erhalten. Jeder Push überschreibt die Branches \
             und Tags des Remotes und löscht diejenigen, die es hier nicht mehr gibt.",
        ),
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("Push-Spiegel sind deaktiviert"),
        RepoSettingsLabel::PushMirrorsDisabledText => text.literal(
            "Ein Administrator dieser Instanz hat Push-Spiegel deaktiviert.",
        ),
        RepoSettingsLabel::NoPushMirrors => text.literal("Noch keine Push-Spiegel"),
        RepoSettingsLabel::NoPushMirrorsText => text.literal(
            "Fügen Sie unten ein Remote hinzu, um anderswo eine Kopie dieses Repositorys zu führen.",
        ),
        RepoSettingsLabel::Paused => text.literal("Pausiert"),
        RepoSettingsLabel::OnPush => text.literal("Bei Push"),
        RepoSettingsLabel::LastPush => text.literal("Letzter Push"),
        RepoSettingsLabel::NotPushedYet => text.literal("Noch nicht gepusht"),
        RepoSettingsLabel::As => text.literal("als"),
        RepoSettingsLabel::SecretConfigured => text.literal("Secret konfiguriert"),
        RepoSettingsLabel::PushNow => text.literal("Jetzt pushen"),
        RepoSettingsLabel::Edit => text.literal("Bearbeiten"),
        RepoSettingsLabel::Remove => text.literal("Entfernen"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("Push-Spiegel hinzufügen"),
        RepoSettingsLabel::AddPushMirror => text.literal("Push-Spiegel hinzufügen"),
        RepoSettingsLabel::PushMirrorAdded => text.literal(
            "Push-Spiegel hinzugefügt. Der erste Push läuft; laden Sie die Seite gleich neu, um das Ergebnis zu \
             sehen.",
        ),
        RepoSettingsLabel::PushMirrorRemoved => text.literal(
            "Push-Spiegel entfernt. Das Remote behält, was es bereits hat.",
        ),
        RepoSettingsLabel::EditPushMirror => text.literal("Push-Spiegel bearbeiten"),
        RepoSettingsLabel::PushMirror => text.literal("Push-Spiegel"),
        RepoSettingsLabel::Enabled => text.literal("Aktiviert"),
        RepoSettingsLabel::EnabledHint => text.literal(
            "Ein pausierter Spiegel behält seine Einstellungen und wird nicht gepusht.",
        ),
        RepoSettingsLabel::Cancel => text.literal("Abbrechen"),
        RepoSettingsLabel::RemoteUrl => text.literal("Remote-URL"),
        RepoSettingsLabel::UrlHintStart => text.literal("Eine mit"),
        RepoSettingsLabel::UrlHintOr => text.literal("oder"),
        RepoSettingsLabel::UrlHintEnd => {
            text.literal("beginnende URL ohne Benutzernamen oder Passwort darin.");
        }
        RepoSettingsLabel::UserName => text.literal("Benutzername"),
        RepoSettingsLabel::PasswordOrToken => text.literal("Passwort oder Token"),
        RepoSettingsLabel::SecretPlaceholder => {
            text.literal("Konfiguriert – leer lassen, um es beizubehalten");
        }
        RepoSettingsLabel::SecretHint => text.literal(
            "Wird per HTTP-Basic-Authentifizierung an das Remote gesendet. Es kann nicht erneut angezeigt werden.",
        ),
        RepoSettingsLabel::SecretHintKept => text.literal(
            "Wird per HTTP-Basic-Authentifizierung an das Remote gesendet. Es kann nicht erneut angezeigt werden \
             und muss neu eingegeben werden, wenn das Remote auf einen anderen Host umzieht.",
        ),
        RepoSettingsLabel::PushAfterPush => text.literal("Nach jedem Push pushen"),
        RepoSettingsLabel::PushAfterPushHint => text.literal(
            "Einige Sekunden nach jeder Änderung an diesem Repository. Jeder Spiegel wird außerdem regelmäßig \
             gepusht.",
        ),
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes } => {
            text.literal("Mindestens ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " Minute.",
                _ => " Minuten.",
            });
        }
        RepoSettingsMessage::PushSchedule { minutes } => {
            text.literal("Jeder Spiegel wird mindestens alle ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " Minute gepusht.",
                _ => " Minuten gepusht.",
            });
        }
        RepoSettingsMessage::DeleteKey { title } => {
            text.argument(title);
            text.literal(" löschen");
        }
    }
}
