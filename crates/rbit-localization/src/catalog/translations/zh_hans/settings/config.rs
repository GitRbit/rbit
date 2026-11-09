//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/config.rs`.
//!
//! `ConfigText` has a message per schema key, too many for one file: its arms live in the
//! sibling modules under `config/`, each a table declared with [`config_table!`].

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

/// Declares a sibling module's part of the `ConfigText` table, written like an
/// [`entries!`] table: `fn write` writes a message it has and returns `true`, or returns
/// `false` for the other parts to try.
macro_rules! config_table {
    ($family:ident<$lifetime:lifetime> => $writer:ident |$text:ident| { $($arms:tt)* }) => {
        /// Writes `message` and returns `true`, or returns `false` when this part lacks it.
        #[allow(
            unreachable_code,
            unreachable_patterns,
            unused_variables,
            clippy::match_single_binding,
            clippy::too_many_lines,
            reason = "a part lists only its own keys"
        )]
        pub(super) fn write(message: $family<'_>, $text: &mut $writer) -> bool {
            match message {
                $($arms)*
                _ => return false,
            }
            true
        }
    };
}

mod fields_a;
mod fields_b;
mod fields_c;
mod fields_d;
mod sections;

entries! { ZhHans:
    ConfigText<'a> => MessageWriter |text| {
        message => {
            return fields_a::write(message, text)
                || fields_b::write(message, text)
                || fields_c::write(message, text)
                || fields_d::write(message, text)
                || sections::write(message, text);
        }
    }
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("配置"),
        ConfigMessage::Intro => text.literal("按功能分类的设置，用于管理你的自托管实例。"),
        ConfigMessage::RestartServer => text.literal("重启服务器"),
        ConfigMessage::Saved => text.literal("设置已保存。重启服务器后部署更改才会生效。"),
        ConfigMessage::RestartRequired => text.literal("需要重启"),
        ConfigMessage::EnvironmentWins => text.literal("环境变量优先。"),
        ConfigMessage::LivePolicies => text.literal("品牌与策略——立即生效"),
        ConfigMessage::SettingsSection => text.literal("设置分区"),
        ConfigMessage::OpenSection => text.literal("打开分区"),
        ConfigMessage::Sections => text.literal("配置分区"),
        ConfigMessage::LiveOverrides => text.literal("已保存的实时品牌和注册策略会覆盖这些部署默认值。"),
        ConfigMessage::IdentityProviders => text.literal("身份提供方"),
        ConfigMessage::ProvidersHint => text.literal(
            "请先添加和移除提供方卡片，再输入新的凭据并保存。提供方的更改会保留在此表单中，直到你保存该分区。",
        ),
        ConfigMessage::CallbackLead => text.literal("在每个提供方处注册一个回调 URL：你的公开 URL 后接"),
        ConfigMessage::CallbackTail => text.literal(
            "，并将 PROVIDER_ID 替换为下方的 ID。对于 OpenID Connect，请设置签发者 URL；对于 GitHub Enterprise，请设置 Enterprise URL。",
        ),
        ConfigMessage::NoProviders => text.literal("未配置外部登录提供方。可使用本地登录。"),
        ConfigMessage::RemoveProvider => text.literal("移除提供方"),
        ConfigMessage::AddProvider => text.literal("添加身份提供方"),
        ConfigMessage::NewProvider => text.literal("新的登录提供方"),
        ConfigMessage::OnlySection => text.literal("只会更新此分区。"),
        ConfigMessage::Recovery => text.literal("恢复与部署默认值"),
        ConfigMessage::RecoveryLead => text.literal("如果已保存的设置导致无法启动，请在服务器环境中设置"),
        ConfigMessage::RecoveryTail => text.literal("，重启后再在此处更正或移除覆盖项。"),
        ConfigMessage::RemovingOverrides => {
            text.literal("移除覆盖项后，下次重启时将恢复文件和环境中的配置。实时品牌和策略会保留。");
        }
        ConfigMessage::RemoveOverrides => text.literal("移除所有部署覆盖项"),
        ConfigMessage::Optional => text.literal("（可选）"),
        ConfigMessage::OnePerLine => text.literal("每行一项。"),
        ConfigMessage::ProviderDefaults => text.literal("使用提供方默认值"),
        ConfigMessage::Configured => text.literal("已配置——留空则保持不变"),
        ConfigMessage::NotConfigured => text.literal("未配置"),
        ConfigMessage::SecretHint => text.literal(
            "留空则保留当前凭据。出错后不会重新显示新输入的凭据；更正表单时请重新输入。请保护好实例数据库及其备份。",
        ),
        ConfigMessage::ClearSecret => text.literal("清除此凭据"),
        ConfigMessage::BlankDefault => text.literal("留空则使用默认值。"),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.literal("保存“");
            text.argument(section);
            text.literal("”设置");
        }
    }
}
