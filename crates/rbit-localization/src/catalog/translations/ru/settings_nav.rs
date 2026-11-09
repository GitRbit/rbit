//! Russian (`ru`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { Ru:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("Настройки"),
        SettingsNavMessage::Profile => text.literal("Профиль"),
        SettingsNavMessage::Account => text.literal("Учётная запись"),
        SettingsNavMessage::Appearance => text.literal("Оформление"),
        SettingsNavMessage::Language => text.literal("Язык"),
        SettingsNavMessage::Security => text.literal("Безопасность"),
        SettingsNavMessage::SshKeys => text.literal("SSH-ключи"),
        SettingsNavMessage::AccessTokens => text.literal("Токены доступа"),
        SettingsNavMessage::Notifications => text.literal("Уведомления"),
        SettingsNavMessage::Actions => text.literal("Actions"),
    }
}
