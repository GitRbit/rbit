//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings_nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings_nav::SettingsNavMessage;

entries! { PtBr:
    SettingsNavMessage => MessageWriter |text| {
        SettingsNavMessage::Settings => text.literal("Configurações"),
        SettingsNavMessage::Profile => text.literal("Perfil"),
        SettingsNavMessage::Account => text.literal("Conta"),
        SettingsNavMessage::Appearance => text.literal("Aparência"),
        SettingsNavMessage::Language => text.literal("Idioma"),
        SettingsNavMessage::Security => text.literal("Segurança"),
        SettingsNavMessage::SshKeys => text.literal("Chaves SSH"),
        SettingsNavMessage::AccessTokens => text.literal("Tokens de acesso"),
        SettingsNavMessage::Notifications => text.literal("Notificações"),
        SettingsNavMessage::Actions => text.literal("Actions"),
    }
}
