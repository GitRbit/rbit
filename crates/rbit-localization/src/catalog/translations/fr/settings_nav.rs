//! French (`fr`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { Fr:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("Paramètres"),
        SettingsNavMessage::Profile => text.literal("Profil"),
        SettingsNavMessage::Account => text.literal("Compte"),
        SettingsNavMessage::Appearance => text.literal("Apparence"),
        SettingsNavMessage::Language => text.literal("Langue"),
        SettingsNavMessage::Security => text.literal("Sécurité"),
        SettingsNavMessage::SshKeys => text.literal("Clés SSH"),
        SettingsNavMessage::AccessTokens => text.literal("Jetons d’accès"),
        SettingsNavMessage::Notifications => text.literal("Notifications"),
        SettingsNavMessage::Actions => text.literal("Actions"),
    }
}
