//! German (`de`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};

entries! { De:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("Alle Personen mit einem Konto auf dieser Instanz."),
        AdminPageMessage::UsersSearch => text.literal("Nach Benutzername, Name oder E-Mail suchen"),
        AdminPageMessage::NewUser => text.literal("Neuer Benutzer"),
        AdminPageMessage::NoUsers => text.literal("Keine Benutzer gefunden"),
        AdminPageMessage::NoUsersText => text.literal("Versuchen Sie eine andere Suche."),
        AdminPageMessage::User => text.literal("Benutzer"),
        AdminPageMessage::Joined => text.literal("Beigetreten"),
        AdminPageMessage::Admin => text.literal("Admin"),
        AdminPageMessage::Suspended => text.literal("Gesperrt"),
        AdminPageMessage::EmailVerified => text.literal("E-Mail-Adresse bestätigt"),
        AdminPageMessage::Verified => text.literal("Bestätigt"),
        AdminPageMessage::You => text.literal("Sie"),
        AdminPageMessage::RemoveAdmin => text.literal("Administratorrechte entziehen"),
        AdminPageMessage::MakeAdmin => text.literal("Zum Administrator machen"),
        AdminPageMessage::LiftSuspension => text.literal("Sperre aufheben"),
        AdminPageMessage::Suspend => text.literal("Sperren"),
        AdminPageMessage::SendReset => text.literal("Passwort-Zurücksetzung senden"),
        AdminPageMessage::SetPasswordMenu => text.literal("Passwort festlegen…"),
        AdminPageMessage::DeleteMenu => text.literal("Löschen…"),
        AdminPageMessage::ResetSent => text.literal("E-Mail zum Zurücksetzen des Passworts gesendet."),
        AdminPageMessage::PasswordSet => text.literal("Passwort aktualisiert."),
        AdminPageMessage::NewUserIntro => {
            text.literal("Administratoren können Konten auch bei geschlossener Registrierung erstellen.");
        }
        AdminPageMessage::Username => text.literal("Benutzername"),
        AdminPageMessage::UsernameHint => text.literal("Buchstaben, Ziffern, '-', '_' und '.'"),
        AdminPageMessage::Email => text.literal("E-Mail"),
        AdminPageMessage::InitialPassword => text.literal("Anfangspasswort"),
        AdminPageMessage::SiteAdministrator => text.literal("Website-Administrator"),
        AdminPageMessage::SiteAdministratorText => {
            text.literal("Vollzugriff auf alle Repositorys und auf diesen Bereich.");
        }
        AdminPageMessage::CreateUser => text.literal("Benutzer erstellen"),
        AdminPageMessage::RegistrationClosed => {
            text.literal(
                "Die öffentliche Registrierung ist derzeit geschlossen; nur so können Benutzer hinzugefügt werden.",
            );
        }
        AdminPageMessage::SetPassword => text.literal("Passwort festlegen"),
        AdminPageMessage::ResetHint => {
            text.literal(
                "Damit die Person stattdessen selbst ein Passwort wählt, senden Sie aus der Benutzerliste eine \
                 Passwort-Zurücksetzung.",
            );
        }
        AdminPageMessage::DeleteUser => text.literal("Benutzer löschen"),
        AdminPageMessage::DeleteUserLead => text.literal("Folgendes wird endgültig gelöscht:"),
        AdminPageMessage::OrganizationsIntro => text.literal("Gemeinsame Namensräume und ihre Mitglieder."),
        AdminPageMessage::SearchByName => text.literal("Nach Name suchen"),
        AdminPageMessage::NoOrganizations => text.literal("Keine Organisationen gefunden"),
        AdminPageMessage::NoOrganizationsText => {
            text.literal("Organisationen werden hier aufgeführt, sobald es welche gibt.");
        }
        AdminPageMessage::Organization => text.literal("Organisation"),
        AdminPageMessage::Members => text.literal("Mitglieder"),
        AdminPageMessage::Created => text.literal("Erstellt"),
        AdminPageMessage::RepositoriesIntro => {
            text.literal("Alle Repositorys, öffentlich und privat, zuletzt aktive zuerst.");
        }
        AdminPageMessage::RepositoriesSearch => text.literal("Nach Inhaber/Name oder Beschreibung suchen"),
        AdminPageMessage::NoRepositories => text.literal("Keine Repositorys gefunden"),
        AdminPageMessage::NoRepositoriesText => {
            text.literal("Repositorys werden hier aufgeführt, sobald es welche gibt.");
        }
        AdminPageMessage::Repository => text.literal("Repository"),
        AdminPageMessage::Updated => text.literal("Aktualisiert"),
        AdminPageMessage::DeleteRepository => text.literal("Repository löschen"),
        AdminPageMessage::DeleteRepositoryText => {
            text.literal(
                "Dadurch wird das Repository einschließlich aller Git-Daten endgültig gelöscht. Dies kann nicht \
                 rückgängig gemacht werden.",
            );
        }
        AdminPageMessage::AuditIntro => {
            text.literal(
                "Anmeldungen, Änderungen an der Zwei-Faktor-Authentifizierung, Sitzungen, Zugriffstokens und \
                 Administratoraktionen, neueste zuerst.",
            );
        }
        AdminPageMessage::AuditSearch => text.literal("Benutzer, Ziel oder IP-Adresse"),
        AdminPageMessage::Category => text.literal("Kategorie"),
        AdminPageMessage::Filter => text.literal("Filtern"),
        AdminPageMessage::AllEvents => text.literal("Alle Ereignisse"),
        AdminPageMessage::Clear => text.literal("Zurücksetzen"),
        AdminPageMessage::NoEvents => text.literal("Keine Ereignisse"),
        AdminPageMessage::NoEventsText => text.literal("Noch passt nichts zu diesen Filtern."),
        AdminPageMessage::As => text.literal("als"),
        AdminPageMessage::By => text.literal("von"),
        AdminPageMessage::On => text.literal("Ziel:"),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => {
            text.literal("Änderungen gelten sofort für die gesamte Instanz.");
        }
        InstanceSettingsMessage::Saved => text.literal("Einstellungen gespeichert."),
        InstanceSettingsMessage::SiteName => text.literal("Website-Name"),
        InstanceSettingsMessage::SiteNameHint => {
            text.literal("Wird in der Kopfzeile, in Seitentiteln und bei Installation als App angezeigt.");
        }
        InstanceSettingsMessage::Branding => text.literal("Branding und Identität"),
        InstanceSettingsMessage::BrandingHint => {
            text.literal(
                "Texte sind reiner Text. Bilder und Links akzeptieren HTTPS-URLs oder lokale /Pfade, die Ihr \
                 Reverse-Proxy ausliefert. Leere Bildfelder verwenden die mitgelieferten Standards.",
            );
        }
        InstanceSettingsMessage::PrimaryColor => {
            text.literal("Farbe der Hauptschaltfläche (leer: Design-Farbe)");
        }
        InstanceSettingsMessage::PrimaryTextColor => text.literal("Textfarbe der Hauptschaltfläche"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => {
            text.literal(
                "Wählen Sie Farben mit genügend Kontrast für gut lesbare Schaltflächen. Legen Sie beide Farben \
                 fest oder lassen Sie beide leer.",
            );
        }
        InstanceSettingsMessage::Description => text.literal("Website-Beschreibung"),
        InstanceSettingsMessage::WelcomeTitle => {
            text.literal("Begrüßungstitel (leer: Website-Name)");
        }
        InstanceSettingsMessage::WelcomeMessage => text.literal("Begrüßungstext"),
        InstanceSettingsMessage::LogoUrl => text.literal("Logo-URL"),
        InstanceSettingsMessage::FaviconUrl => text.literal("Favicon-URL"),
        InstanceSettingsMessage::AppIconUrl => {
            text.literal("App-Symbol-URL (quadratisch, mindestens 512 Pixel)");
        }
        InstanceSettingsMessage::FooterText => text.literal("Fußzeilentext"),
        InstanceSettingsMessage::SupportUrl => text.literal("Support-URL"),
        InstanceSettingsMessage::TermsUrl => text.literal("URL der Nutzungsbedingungen"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("URL der Datenschutzerklärung"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("„Powered by rbit“ anzeigen"),
        InstanceSettingsMessage::ShowVersion => text.literal("Softwareversion anzeigen"),
        InstanceSettingsMessage::ShowFeatures => {
            text.literal("Self-Hosting-Funktionen auf der Startseite anzeigen");
        }
        InstanceSettingsMessage::Registration => text.literal("Registrierung"),
        InstanceSettingsMessage::Open => text.literal("Offen"),
        InstanceSettingsMessage::OpenText => {
            text.literal("Jede Person kann auf der Registrierungsseite ein Konto erstellen.");
        }
        InstanceSettingsMessage::Closed => text.literal("Geschlossen"),
        InstanceSettingsMessage::ClosedText => text.literal("Nur Administratoren können Konten erstellen."),
        InstanceSettingsMessage::OrgCreation => text.literal("Wer Organisationen erstellen darf"),
        InstanceSettingsMessage::AnyUser => text.literal("Alle Benutzer"),
        InstanceSettingsMessage::AnyUserText => {
            text.literal("Jeder angemeldete Benutzer kann Organisationen erstellen.");
        }
        InstanceSettingsMessage::AdminsOnly => text.literal("Nur Administratoren"),
        InstanceSettingsMessage::AdminsOnlyText => {
            text.literal("Benutzer können weiterhin zu bestehenden Organisationen hinzugefügt werden.");
        }
        InstanceSettingsMessage::DefaultVisibility => text.literal("Standardsichtbarkeit von Repositorys"),
        InstanceSettingsMessage::Public => text.literal("Öffentlich"),
        InstanceSettingsMessage::PublicText => {
            text.literal("Neue Repositorys sind für alle sichtbar, sofern nicht geändert.");
        }
        InstanceSettingsMessage::Private => text.literal("Privat"),
        InstanceSettingsMessage::PrivateText => {
            text.literal("Neue Repositorys sind zunächst privat; Inhaber können sie dennoch öffentlich machen.");
        }
        InstanceSettingsMessage::VisibilityHint => {
            text.literal("Im Formular für neue Repositorys vorausgewählt.");
        }
        InstanceSettingsMessage::DigestTime => text.literal("Uhrzeit der E-Mail-Zusammenfassung"),
        InstanceSettingsMessage::DigestHint => {
            text.literal(
                "Wann tägliche und wöchentliche Benachrichtigungszusammenfassungen an Benutzer gesendet werden, die \
                 keine Uhrzeit gewählt haben. Wöchentliche Zusammenfassungen werden montags versendet.",
            );
        }
        InstanceSettingsMessage::Save => text.literal("Einstellungen speichern"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.literal("Mindestens ");
            text.number(length);
            text.literal(
                " Zeichen. Teilen Sie es dem Benutzer auf sicherem Weg mit. Er muss es bei der ersten Anmeldung \
                 ersetzen.",
            );
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal(
                "Das neue Passwort ersetzt das aktuelle sofort, und alle Links zum Zurücksetzen des Passworts \
                 werden ungültig. Teilen Sie es ",
            );
            text.argument(name);
            text.literal(" auf sicherem Weg mit.");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal(", die zugehörigen Repositorys (");
            text.argument(repositories);
            text.literal(
                ", einschließlich aller Git-Daten), Zugriffstokens und Organisationsmitgliedschaften. Dies kann \
                 nicht rückgängig gemacht werden.",
            );
        }
    }
}
