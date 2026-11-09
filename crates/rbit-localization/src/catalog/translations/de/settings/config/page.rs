//! German (`de`) entries for the fixed labels of `catalog/settings/config.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText};

entries! { De:
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("Konfiguration"),
        ConfigMessage::Intro => text.literal("Verwalten Sie Ihre selbst gehostete Instanz mit nach Funktionen geordneten Einstellungen."),
        ConfigMessage::RestartServer => text.literal("Server neu starten"),
        ConfigMessage::Saved => text.literal("Einstellungen gespeichert. Starten Sie den Server neu, um Deployment-Änderungen anzuwenden."),
        ConfigMessage::RestartRequired => text.literal("Neustart erforderlich"),
        ConfigMessage::EnvironmentWins => text.literal("Umgebungsvariablen haben Vorrang."),
        ConfigMessage::LivePolicies => text.literal("Branding und Richtlinien – sofort wirksam"),
        ConfigMessage::SettingsSection => text.literal("Einstellungsbereich"),
        ConfigMessage::OpenSection => text.literal("Bereich öffnen"),
        ConfigMessage::Sections => text.literal("Konfigurationsbereiche"),
        ConfigMessage::LiveOverrides => text.literal("Gespeicherte Live-Richtlinien für Branding und Registrierung überschreiben diese Deployment-Standardwerte."),
        ConfigMessage::IdentityProviders => text.literal("Identitätsanbieter"),
        ConfigMessage::ProvidersHint => text.literal("Fügen Sie zuerst Anbieterkarten hinzu oder entfernen Sie sie, geben Sie dann neue Zugangsdaten ein und speichern Sie. Änderungen an Anbietern bleiben in diesem Formular, bis Sie den Bereich speichern."),
        ConfigMessage::CallbackLead => text.literal("Registrieren Sie bei jedem Anbieter eine Callback-URL: Ihre öffentliche URL gefolgt von"),
        ConfigMessage::CallbackTail => text.literal(", wobei Sie PROVIDER_ID durch die ID unten ersetzen. Für OpenID Connect legen Sie die Issuer-URL fest, für GitHub Enterprise die Enterprise-URL."),
        ConfigMessage::NoProviders => text.literal("Keine externen Anmeldeanbieter konfiguriert. Die lokale Anmeldung ist verfügbar."),
        ConfigMessage::RemoveProvider => text.literal("Anbieter entfernen"),
        ConfigMessage::AddProvider => text.literal("Identitätsanbieter hinzufügen"),
        ConfigMessage::NewProvider => text.literal("Neuer Anmeldeanbieter"),
        ConfigMessage::OnlySection => text.literal("Nur dieser Bereich wird aktualisiert."),
        ConfigMessage::Recovery => text.literal("Wiederherstellung und Deployment-Standardwerte"),
        ConfigMessage::RecoveryLead => text.literal("Wenn gespeicherte Einstellungen den Start verhindern, setzen Sie"),
        ConfigMessage::RecoveryTail => text.literal("in der Serverumgebung, starten Sie neu und korrigieren oder entfernen Sie dann die Überschreibungen hier."),
        ConfigMessage::RemovingOverrides => text.literal("Das Entfernen der Überschreibungen stellt beim nächsten Neustart die Konfiguration aus Datei und Umgebung wieder her. Live-Branding und Richtlinien bleiben erhalten."),
        ConfigMessage::RemoveOverrides => text.literal("Alle Deployment-Überschreibungen entfernen"),
        ConfigMessage::Optional => text.literal("(optional)"),
        ConfigMessage::OnePerLine => text.literal("Ein Eintrag pro Zeile."),
        ConfigMessage::ProviderDefaults => text.literal("Anbieter-Standardwerte verwenden"),
        ConfigMessage::Configured => text.literal("Konfiguriert – leer lassen, um beizubehalten"),
        ConfigMessage::NotConfigured => text.literal("Nicht konfiguriert"),
        ConfigMessage::SecretHint => text.literal("Leer lassen, um die aktuellen Zugangsdaten zu behalten. Neue Zugangsdaten werden nach einem Fehler nie erneut angezeigt; geben Sie sie beim Korrigieren des Formulars erneut ein. Schützen Sie die Datenbank der Instanz und ihre Backups."),
        ConfigMessage::ClearSecret => text.literal("Diese Zugangsdaten löschen"),
        ConfigMessage::BlankDefault => text.literal("Leer lassen, um den Standardwert zu verwenden."),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.literal("Einstellungen für „");
            text.argument(section);
            text.literal("“ speichern");
        }
    }
}
