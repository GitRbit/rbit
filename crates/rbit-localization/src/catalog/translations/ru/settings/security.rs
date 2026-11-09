//! Russian (`ru`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { Ru:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("Безопасность"),
        SecurityMessage::Intro => text.literal("Защитите учётную запись ключом доступа или приложением-аутентификатором и следите за тем, где выполнен вход."),
        SecurityMessage::TwoFactor => text.literal("Двухфакторная аутентификация"),
        SecurityMessage::On => text.literal("Включена"),
        SecurityMessage::Off => text.literal("Выключена"),
        SecurityMessage::EnabledLead => text.literal("При входе после пароля запрашивается ключ доступа или код. Для Git по HTTPS нужен"),
        SecurityMessage::TokenLink => text.literal("персональный токен доступа"),
        SecurityMessage::EnabledTail => text.literal("; ваш пароль там больше не работает."),
        SecurityMessage::Disabled => text.literal("Добавьте второй шаг при входе: ключ доступа, приложение-аутентификатор или и то и другое."),
        SecurityMessage::Passkeys => text.literal("Ключи доступа и ключи безопасности"),
        SecurityMessage::PasskeysText => text.literal("Подтверждайте вход отпечатком пальца, лицом, PIN-кодом устройства, телефоном или аппаратным ключом, например YubiKey."),
        SecurityMessage::Remove => text.literal("Удалить"),
        SecurityMessage::OnlyFactor => text.literal("Это ваш единственный второй фактор: его удаление отключит двухфакторную аутентификацию и удалит коды восстановления."),
        SecurityMessage::PasskeyGone => text.literal("Вы больше не сможете входить с этим ключом доступа."),
        SecurityMessage::Name => text.literal("Название"),
        SecurityMessage::PasskeyPlaceholder => text.literal("например, MacBook или YubiKey"),
        SecurityMessage::AddPasskey => text.literal("Добавить ключ доступа"),
        SecurityMessage::PasskeyUnsupported => text.literal("Для добавления ключа доступа нужен браузер с поддержкой ключей доступа и включённым JavaScript."),
        SecurityMessage::PasskeyUrlLead => text.literal("Для ключей доступа параметр"),
        SecurityMessage::PasskeyUrlTail => text.literal("должен содержать адрес этого сайта."),
        SecurityMessage::Authenticator => text.literal("Приложение-аутентификатор"),
        SecurityMessage::AuthenticatorText => text.literal("Используйте 6-значные коды из приложения, например 1Password, Aegis или Google Authenticator."),
        SecurityMessage::SetUp => text.literal("Настроить"),
        SecurityMessage::RemoveAuthenticator => text.literal("Удалить приложение-аутентификатор"),
        SecurityMessage::PasswordAlone => text.literal("Учётная запись будет защищена только паролем, а коды восстановления будут удалены."),
        SecurityMessage::PasskeysStay => text.literal("Ключи доступа останутся вашим вторым фактором."),
        SecurityMessage::RecoveryCodes => text.literal("Коды восстановления"),
        SecurityMessage::Regenerate => text.literal("Создать новые коды восстановления"),
        SecurityMessage::CodesStop => text.literal("Текущие коды восстановления перестанут работать."),
        SecurityMessage::GenerateCodes => text.literal("Создать новые коды"),
        SecurityMessage::SignInMethods => text.literal("Способы входа"),
        SecurityMessage::Password => text.literal("Пароль"),
        SecurityMessage::NoPassword => text.literal("Без пароля"),
        SecurityMessage::WithPassword => text.literal("Входите с паролем или через привязанную учётную запись."),
        SecurityMessage::LinkedOnlyLead => text.literal("У вашей учётной записи нет пароля: вы входите через привязанную учётную запись. Чтобы задать пароль, запросите"),
        SecurityMessage::ResetLink => text.literal("ссылку для сброса пароля"),
        SecurityMessage::LinkedOnlyMiddle => text.literal(". Для Git по HTTPS нужен"),
        SecurityMessage::LinkedOnlyTail => text.literal("."),
        SecurityMessage::NoLongerAvailable => text.literal("Больше недоступно"),
        SecurityMessage::Unlink => text.literal("Отвязать"),
        SecurityMessage::OnlyWay => text.literal("Единственный способ входа"),
        SecurityMessage::NotLinked => text.literal("Не привязано"),
        SecurityMessage::Link => text.literal("Привязать"),
        SecurityMessage::Sessions => text.literal("Сеансы"),
        SecurityMessage::SignOutOthers => text.literal("Выйти на всех остальных устройствах"),
        SecurityMessage::ThisDevice => text.literal("Это устройство"),
        SecurityMessage::SignOut => text.literal("Выйти"),
        SecurityMessage::RecentActivity => text.literal("Недавняя активность"),
        SecurityMessage::NoActivity => text.literal("Активности пока нет"),
        SecurityMessage::NoActivityText => text.literal("Здесь появятся входы и изменения безопасности вашей учётной записи."),
        SecurityMessage::SetupTitle => text.literal("Настройка приложения-аутентификатора"),
        SecurityMessage::SetupIntro => text.literal("Отсканируйте QR-код приложением-аутентификатором, затем введите показанный им 6-значный код."),
        SecurityMessage::CantScan => text.literal("Не получается отсканировать? Введите этот ключ"),
        SecurityMessage::CopyKey => text.literal("Скопировать ключ"),
        SecurityMessage::Copy => text.literal("Скопировать"),
        SecurityMessage::TimeBased => text.literal("На основе времени, 6 цифр, каждые 30 секунд."),
        SecurityMessage::ConfirmCode => text.literal("2. Подтвердите код"),
        SecurityMessage::AuthenticationCode => text.literal("Код аутентификации"),
        SecurityMessage::Cancel => text.literal("Отмена"),
        SecurityMessage::Enable => text.literal("Включить"),
        SecurityMessage::TwoFactorOn => text.literal("Двухфакторная аутентификация включена."),
        SecurityMessage::SaveCodes => text.literal("Сохраните эти коды сейчас"),
        SecurityMessage::CodesText => text.literal("Каждый код позволяет войти один раз, если вы потеряете ключи доступа или приложение-аутентификатор. Больше они показаны не будут, поэтому храните их в надёжном месте, например в менеджере паролей."),
        SecurityMessage::SavedCodes => text.literal("Я сохранил коды"),
        SecurityMessage::AppOrRecoveryCode => text.literal("Код аутентификации или восстановления"),
        SecurityMessage::RecoveryCode => text.literal("Код восстановления"),
        SecurityMessage::PasskeyInstead => text.literal("Или подтвердите одним из ключей доступа вместо кода."),
        SecurityMessage::AuthenticatorRemoved => text.literal("Приложение-аутентификатор удалено."),
        SecurityMessage::AuthenticatorAdded => text.literal("Приложение-аутентификатор добавлено."),
        SecurityMessage::PasskeyAdded => text.literal("Ключ доступа добавлен."),
        SecurityMessage::PasskeyRemoved => text.literal("Ключ доступа удалён."),
        SecurityMessage::SessionRevoked => text.literal("Выход из сеанса выполнен."),
        SecurityMessage::OtherSessionsRevoked => text.literal("Выполнен выход из всех остальных сеансов."),
        SecurityMessage::SetupCancelled => text.literal("Настройка двухфакторной аутентификации отменена."),
        SecurityMessage::IdentityLinked => text.literal("Учётная запись привязана. Теперь через неё можно входить."),
        SecurityMessage::IdentityUnlinked => text.literal("Учётная запись отвязана."),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. Добавьте ");
            text.argument(site);
            text.literal(" в приложение");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.literal("Отвязать ");
            text.argument(provider);
        }
        SecurityText::SignOutLabel(device) => {
            text.literal("Выйти на устройстве ");
            text.argument(device);
        }
        SecurityText::WithPasskey(action) => {
            text.argument(action);
            text.literal(" с помощью ключа доступа");
        }
    }
}
