//! Spanish (`es`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { Es:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("Configuración"),
        SettingsNavMessage::Profile => text.literal("Perfil"),
        SettingsNavMessage::Account => text.literal("Cuenta"),
        SettingsNavMessage::Appearance => text.literal("Apariencia"),
        SettingsNavMessage::Language => text.literal("Idioma"),
        SettingsNavMessage::Security => text.literal("Seguridad"),
        SettingsNavMessage::SshKeys => text.literal("Claves SSH"),
        SettingsNavMessage::AccessTokens => text.literal("Tokens de acceso"),
        SettingsNavMessage::Notifications => text.literal("Notificaciones"),
        SettingsNavMessage::Actions => text.literal("Acciones"),
    }
}
