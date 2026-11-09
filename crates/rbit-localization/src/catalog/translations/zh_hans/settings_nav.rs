//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { ZhHans:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("设置"),
        SettingsNavMessage::Profile => text.literal("个人资料"),
        SettingsNavMessage::Account => text.literal("账户"),
        SettingsNavMessage::Appearance => text.literal("外观"),
        SettingsNavMessage::Language => text.literal("语言"),
        SettingsNavMessage::Security => text.literal("安全"),
        SettingsNavMessage::SshKeys => text.literal("SSH 密钥"),
        SettingsNavMessage::AccessTokens => text.literal("访问令牌"),
        SettingsNavMessage::Notifications => text.literal("通知"),
        SettingsNavMessage::Actions => text.literal("Actions"),
    }
}
