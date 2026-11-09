//! Korean (`ko`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { Ko:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("설정"),
        SettingsNavMessage::Profile => text.literal("프로필"),
        SettingsNavMessage::Account => text.literal("계정"),
        SettingsNavMessage::Appearance => text.literal("모양"),
        SettingsNavMessage::Language => text.literal("언어"),
        SettingsNavMessage::Security => text.literal("보안"),
        SettingsNavMessage::SshKeys => text.literal("SSH 키"),
        SettingsNavMessage::AccessTokens => text.literal("액세스 토큰"),
        SettingsNavMessage::Notifications => text.literal("알림"),
        SettingsNavMessage::Actions => text.literal("Actions"),
    }
}
