//! German (`de`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { De:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("Einstellungen"),
        SettingsNavMessage::Profile => text.literal("Profil"),
        SettingsNavMessage::Account => text.literal("Konto"),
        SettingsNavMessage::Appearance => text.literal("Darstellung"),
        SettingsNavMessage::Language => text.literal("Sprache"),
        SettingsNavMessage::Security => text.literal("Sicherheit"),
        SettingsNavMessage::SshKeys => text.literal("SSH-Schlüssel"),
        SettingsNavMessage::AccessTokens => text.literal("Zugriffstokens"),
        SettingsNavMessage::Notifications => text.literal("Benachrichtigungen"),
        SettingsNavMessage::Actions => text.literal("Actions"),
    }
}
