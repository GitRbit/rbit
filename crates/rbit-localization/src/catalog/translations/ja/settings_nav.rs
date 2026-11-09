//! Japanese (`ja`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { Ja:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("設定"),
        SettingsNavMessage::Profile => text.literal("プロフィール"),
        SettingsNavMessage::Account => text.literal("アカウント"),
        SettingsNavMessage::Appearance => text.literal("外観"),
        SettingsNavMessage::Language => text.literal("言語"),
        SettingsNavMessage::Security => text.literal("セキュリティ"),
        SettingsNavMessage::SshKeys => text.literal("SSH キー"),
        SettingsNavMessage::AccessTokens => text.literal("アクセストークン"),
        SettingsNavMessage::Notifications => text.literal("通知"),
        SettingsNavMessage::Actions => text.literal("Actions"),
    }
}
