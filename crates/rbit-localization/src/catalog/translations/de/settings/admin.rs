//! German (`de`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};

entries! { De:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("Website-Administration"),
        AdminMessage::Administration => text.literal("Administration"),
        AdminMessage::Dashboard => text.literal("Dashboard"),
        AdminMessage::Users => text.literal("Benutzer"),
        AdminMessage::Organizations => text.literal("Organisationen"),
        AdminMessage::Repositories => text.literal("Repositorys"),
        AdminMessage::Settings => text.literal("Einstellungen"),
        AdminMessage::AuditLog => text.literal("Audit-Log"),
        AdminMessage::Configuration => text.literal("Konfiguration"),
        AdminMessage::Runners => text.literal("Runner"),
        AdminMessage::Search => text.literal("Suchen"),
        AdminMessage::Pagination => text.literal("Seitennavigation"),
        AdminMessage::Previous => text.literal("Zurück"),
        AdminMessage::Next => text.literal("Weiter"),
        AdminMessage::NoResults => text.literal("Keine Ergebnisse"),
        AdminMessage::Cancel => text.literal("Abbrechen"),
        AdminMessage::Actions => text.literal("Aktionen"),
        AdminMessage::Private => text.literal("Privat"),
        AdminMessage::Archived => text.literal("Archiviert"),
        AdminMessage::Delete => text.literal("Löschen"),
        AdminMessage::TypeToConfirmLead => text.literal("Geben Sie"),
        AdminMessage::TypeToConfirmTail => text.literal("zur Bestätigung ein"),
        AdminMessage::NewPassword => text.literal("Neues Passwort"),
        AdminMessage::ConfirmPassword => text.literal("Neues Passwort bestätigen"),
        AdminMessage::Unavailable => text.literal("nicht verfügbar"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("Eine Übersicht über diese Instanz."),
        DashboardMessage::RestartServer => text.literal("Server neu starten"),
        DashboardMessage::RestartUnavailableHint => {
            text.literal("Auf diesem Server-Host ist kein Neustart möglich");
        }
        DashboardMessage::SoftwareUpdates => text.literal("Software-Updates"),
        DashboardMessage::ConfigureUpdates => {
            text.literal("Update-Prüfungen und Versionsdatenschutz konfigurieren");
        }
        DashboardMessage::LiveMonitoring => text.literal("Live-Überwachung"),
        DashboardMessage::MonitoringIntro => {
            text.literal("Datenverkehr, Antwortzeiten und Git-Aktivität der letzten fünf Minuten.");
        }
        DashboardMessage::Snapshot => text.literal("Momentaufnahme"),
        DashboardMessage::Pause => text.literal("Pausieren"),
        DashboardMessage::NeedsJavaScript => {
            text.literal(
                "Live-Updates erfordern JavaScript. Laden Sie diese Seite neu, um eine aktuelle Momentaufnahme zu \
                 erhalten.",
            );
        }
        DashboardMessage::System => text.literal("System"),
        DashboardMessage::InstanceSettings => text.literal("Instanzeinstellungen"),
        DashboardMessage::Edit => text.literal("Bearbeiten"),
        DashboardMessage::Users => text.literal("Benutzer"),
        DashboardMessage::Organizations => text.literal("Organisationen"),
        DashboardMessage::SharedNamespaces => text.literal("Gemeinsame Namensräume"),
        DashboardMessage::Repositories => text.literal("Repositorys"),
        DashboardMessage::AccessTokens => text.literal("Zugriffstokens"),
        DashboardMessage::AcrossAllUsers => text.literal("Über alle Benutzer"),
        DashboardMessage::Uptime => text.literal("Betriebszeit"),
        DashboardMessage::DataDirectory => text.literal("Datenverzeichnis"),
        DashboardMessage::DiskUsage => text.literal("Speicherbelegung"),
        DashboardMessage::SiteName => text.literal("Website-Name"),
        DashboardMessage::Explore => text.literal("Entdecken-Seite"),
        DashboardMessage::ExploreEnabled => text.literal("An"),
        DashboardMessage::ExploreDisabled => text.literal("Aus"),
        DashboardMessage::Browsing => text.literal("Durchsuchen"),
        DashboardMessage::BrowsingOpen => text.literal("Alle, angemeldet oder nicht"),
        DashboardMessage::BrowsingSignedIn => text.literal("Nur angemeldete Benutzer"),
        DashboardMessage::Registration => text.literal("Registrierung"),
        DashboardMessage::RegistrationOpen => text.literal("Für alle offen"),
        DashboardMessage::RegistrationClosed => {
            text.literal("Geschlossen (Administratoren erstellen Konten)");
        }
        DashboardMessage::OrgPolicy => text.literal("Organisationen"),
        DashboardMessage::AnyUserCreates => text.literal("Jeder Benutzer kann erstellen"),
        DashboardMessage::AdminsCreate => text.literal("Nur Administratoren können erstellen"),
        DashboardMessage::NewRepositories => text.literal("Neue Repositorys"),
        DashboardMessage::PublicByDefault => text.literal("Standardmäßig öffentlich"),
        DashboardMessage::PrivateByDefault => text.literal("Standardmäßig privat"),
        DashboardMessage::EmailDigests => text.literal("E-Mail-Zusammenfassungen"),
        DashboardMessage::Restarting => text.literal("Server wird neu gestartet"),
        DashboardMessage::RestartRequested => text.literal("Neustart angefordert"),
        DashboardMessage::RestartingText => {
            text.literal(
                "Der Server beendet laufende HTTP-Anfragen, stoppt SSH und Hintergrundjobs und schließt den \
                 Speicher, bevor er neu startet. Lang laufende Übertragungen können eine Weile dauern.",
            );
        }
        DashboardMessage::Waiting => text.literal("Warten, bis der Server wieder erreichbar ist…"),
        DashboardMessage::ReturnToDashboard => text.literal("Zurück zum Dashboard"),
        DashboardMessage::WaitNoScript => {
            text.literal("Warten Sie einen Moment und kehren Sie dann zum Dashboard zurück, um sich neu zu verbinden.");
        }
        DashboardMessage::RestartQuestion => text.literal("Diese rbit-Anwendung neu starten?"),
        DashboardMessage::RestartText => {
            text.literal(
                "Die Website ist kurz nicht erreichbar, während die Anwendung herunterfährt und mit ihrer \
                 gespeicherten Konfiguration neu startet. HTTP-Anfragen dürfen abgeschlossen werden; \
                 SSH-Verbindungen werden getrennt. Benutzer behalten ihre gespeicherten Sitzungen.",
            );
        }
        DashboardMessage::RestartAudited => {
            text.literal("Diese Aktion wird im Audit-Log der Administration aufgezeichnet.");
        }
        DashboardMessage::RestartUnavailable => text.literal("Neustart nicht verfügbar"),
        DashboardMessage::RestartUnsupported => {
            text.literal(
                "Dieser Server-Host unterstützt keine Neustarts der Anwendung. Starten Sie rbit über Ihren \
                 Dienstmanager neu.",
            );
        }
        DashboardMessage::BackToDashboard => text.literal("Zurück zum Dashboard"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("Anfragen im Zeitfenster"),
        MonitoringMessage::ErrorRate => text.literal("Serverfehlerrate"),
        MonitoringMessage::ProcessUptime => text.literal("Prozesslaufzeit"),
        MonitoringMessage::Samples => text.literal("5-Sekunden-Messwerte"),
        MonitoringMessage::FiveMinutesAgo => text.literal("vor 5 Minuten"),
        MonitoringMessage::LatestSample => text.literal("Letzter Messwert"),
        MonitoringMessage::Collecting => {
            text.literal("Erster Messwert wird erfasst. Die Diagramme beginnen nach fünf Sekunden.");
        }
        MonitoringMessage::HttpResponses => text.literal("HTTP-Antworten"),
        MonitoringMessage::CurrentWindow => text.literal("Aktuelles Zeitfenster"),
        MonitoringMessage::Status => text.literal("Status"),
        MonitoringMessage::RequestsColumn => text.literal("Anfragen"),
        MonitoringMessage::Share => text.literal("Anteil"),
        MonitoringMessage::GitActivity => text.literal("Git-Aktivität"),
        MonitoringMessage::Operation => text.literal("Vorgang"),
        MonitoringMessage::Exchanges => text.literal("Austausche"),
        MonitoringMessage::CloneFetch => text.literal("Klonen / Abrufen"),
        MonitoringMessage::Push => text.literal("Push"),
        MonitoringMessage::GitNote => {
            text.literal(
                "Smart-HTTP-Austausche, einschließlich Fehlschlägen. SSH und Referenzankündigungen sind nicht \
                 enthalten. Die Antwortzeit misst die Zeit bis zu den Headern; gestreamte Übertragungen können \
                 weiterlaufen.",
            );
        }
        MonitoringMessage::Footnote => {
            text.literal(
                "Nur dieser Prozess; der Verlauf wird beim Neustart zurückgesetzt. Überwachungsanfragen sind nicht \
                 enthalten.",
            );
        }
        MonitoringMessage::Informational => text.literal("Informativ"),
        MonitoringMessage::Successful => text.literal("Erfolgreich"),
        MonitoringMessage::Redirects => text.literal("Weiterleitungen"),
        MonitoringMessage::ClientErrors => text.literal("Clientfehler"),
        MonitoringMessage::ServerErrors => text.literal("Serverfehler"),
        MonitoringMessage::RequestTraffic => text.literal("Anfrageverkehr"),
        MonitoringMessage::ResponseTime => text.literal("Antwortzeit"),
        MonitoringMessage::RequestsPerSecond => text.literal("Anfr./s"),
        MonitoringMessage::MillisecondsAverage => text.literal("ms Ø"),
        MonitoringMessage::WaitingFirst => text.literal("Warten auf den ersten Messwert"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("Versionshinweise zu rbit ");
            text.argument(version);
            text.literal(" anzeigen");
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.argument(admins);
            text.literal(" Admins · ");
            text.argument(suspended);
            text.literal(" gesperrt");
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("1 Admin · ");
            text.argument(suspended);
            text.literal(" gesperrt");
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.argument(public);
            text.literal(" öffentlich · ");
            text.argument(private);
            text.literal(" privat");
        }
        AdminText::Summary(first, last, total) => {
            text.argument(first);
            text.literal("–");
            text.argument(last);
            text.literal(" von ");
            text.argument(total);
        }
        AdminText::DigestsAt(hour) => {
            text.literal("Versand um ");
            text.argument(hour);
            text.literal(", sofern Benutzer nichts anderes wählen");
        }
        AdminText::DeleteLabel(name) => {
            text.argument(name);
            text.literal(" löschen");
        }
        AdminText::ActionsFor(name) => {
            text.literal("Aktionen für ");
            text.argument(name);
        }
        AdminText::PasswordLength(length) => {
            text.literal("Mindestens ");
            text.number(length);
            text.literal(" Zeichen.");
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.argument(title);
            text.literal(" in den letzten fünf Minuten. Zuletzt: ");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal(". Ohne Anfragen entstehen Lücken in der Antwortzeit.");
        }
        AdminText::UpdatedAt(time) => {
            text.literal("Aktualisiert um ");
            text.argument(time);
            text.literal(" UTC");
        }
        AdminText::Coverage(samples, limit) => {
            text.number(samples);
            text.literal(" von ");
            text.number(limit);
            text.literal(" Messwerten");
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · Administration");
        }
        AdminText::SetPasswordFor(name) => {
            text.literal("Passwort für ");
            text.argument(name);
            text.literal(" festlegen · Administration");
        }
        AdminText::DeletePageTitle(name) => {
            text.argument(name);
            text.literal(" löschen · Administration");
        }
    }
}
