//! Russian (`ru`) entries for `catalog/settings/config.rs`.
//!
//! The configuration schema text (`ConfigText`) is too large for one file: its arms live
//! in the sibling modules, each declared with `config_arms!`, and the table here asks
//! them in turn.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

/// Declares one sibling module's `ConfigText` arms as `write`, returning `false` for a
/// message the module has no arm for.
macro_rules! config_arms {
    (ConfigText<'a> => MessageWriter |$text:ident| { $($arms:tt)* }) => {
        #[allow(
            clippy::match_same_arms,
            clippy::too_many_lines,
            reason = "the schema repeats some help texts across fields"
        )]
        pub(super) fn write(message: ConfigText<'_>, $text: &mut MessageWriter) -> bool {
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
mod schema;

entries! { Ru:
    ConfigText<'a> => MessageWriter |text| {
        message => return fields_a::write(message, text) || fields_b::write(message, text) || schema::write(message, text),
    }
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("Конфигурация"),
        ConfigMessage::Intro => text.literal("Управляйте своим сервером с помощью настроек, сгруппированных по функциям."),
        ConfigMessage::RestartServer => text.literal("Перезапустить сервер"),
        ConfigMessage::Saved => text.literal("Настройки сохранены. Перезапустите сервер, чтобы применить изменения развёртывания."),
        ConfigMessage::RestartRequired => text.literal("Нужен перезапуск"),
        ConfigMessage::EnvironmentWins => text.literal("Переменные окружения имеют приоритет."),
        ConfigMessage::LivePolicies => text.literal("Оформление и политики — применяются сразу"),
        ConfigMessage::SettingsSection => text.literal("Раздел настроек"),
        ConfigMessage::OpenSection => text.literal("Открыть раздел"),
        ConfigMessage::Sections => text.literal("Разделы конфигурации"),
        ConfigMessage::LiveOverrides => text.literal("Сохранённые текущие политики оформления и регистрации переопределяют эти значения развёртывания по умолчанию."),
        ConfigMessage::IdentityProviders => text.literal("Провайдеры удостоверений"),
        ConfigMessage::ProvidersHint => text.literal("Сначала добавьте и удалите карточки провайдеров, затем введите новые учётные данные и сохраните. Изменения провайдеров остаются в этой форме, пока вы не сохраните раздел."),
        ConfigMessage::CallbackLead => text.literal("Зарегистрируйте у каждого провайдера URL обратного вызова: ваш публичный URL, а за ним"),
        ConfigMessage::CallbackTail => text.literal(", где PROVIDER_ID заменён на ID ниже. Для OpenID Connect укажите URL издателя, для GitHub Enterprise — URL Enterprise."),
        ConfigMessage::NoProviders => text.literal("Внешние провайдеры входа не настроены. Доступен локальный вход."),
        ConfigMessage::RemoveProvider => text.literal("Удалить провайдера"),
        ConfigMessage::AddProvider => text.literal("Добавить провайдера удостоверений"),
        ConfigMessage::NewProvider => text.literal("Новый провайдер входа"),
        ConfigMessage::OnlySection => text.literal("Будет обновлён только этот раздел."),
        ConfigMessage::Recovery => text.literal("Восстановление и значения развёртывания по умолчанию"),
        ConfigMessage::RecoveryLead => text.literal("Если сохранённые настройки мешают запуску, задайте"),
        ConfigMessage::RecoveryTail => text.literal("в окружении сервера, перезапустите его, а затем исправьте или удалите переопределения здесь."),
        ConfigMessage::RemovingOverrides => text.literal("Удаление переопределений восстановит конфигурацию из файла и окружения при следующем перезапуске. Текущие оформление и политики сохранятся."),
        ConfigMessage::RemoveOverrides => text.literal("Удалить все переопределения развёртывания"),
        ConfigMessage::Optional => text.literal("(необязательно)"),
        ConfigMessage::OnePerLine => text.literal("По одному значению на строку."),
        ConfigMessage::ProviderDefaults => text.literal("Использовать значения провайдера по умолчанию"),
        ConfigMessage::Configured => text.literal("Задано — оставьте пустым, чтобы сохранить"),
        ConfigMessage::NotConfigured => text.literal("Не задано"),
        ConfigMessage::SecretHint => text.literal("Оставьте пустым, чтобы сохранить текущие учётные данные. После ошибки новые учётные данные не показываются повторно; введите их снова при исправлении формы. Защищайте базу данных сервера и её резервные копии."),
        ConfigMessage::ClearSecret => text.literal("Удалить эти учётные данные"),
        ConfigMessage::BlankDefault => text.literal("Оставьте пустым, чтобы использовать значение по умолчанию."),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.literal("Сохранить настройки раздела «");
            text.argument(section);
            text.literal("»");
        }
    }
}
