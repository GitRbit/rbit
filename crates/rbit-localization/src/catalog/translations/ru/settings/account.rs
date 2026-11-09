//! Russian (`ru`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};
use crate::plural::Plural;

entries! { Ru:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("Обновите пароль"),
        AccountMessage::Title => text.literal("Учётная запись"),
        AccountMessage::Intro => text.literal("Как вы входите. Для изменений нужен текущий пароль."),
        AccountMessage::Username => text.literal("Имя пользователя"),
        AccountMessage::UsernameUpdated => text.literal("Имя пользователя обновлено."),
        AccountMessage::UsernameHint => text.literal("Ссылки на профиль и удалённые репозитории Git со старым именем продолжат работать, пока это имя кто-нибудь не займёт. Обновите ссылки и адреса удалённых репозиториев. Входите с новым именем пользователя или адресом электронной почты."),
        AccountMessage::CurrentPassword => text.literal("Текущий пароль"),
        AccountMessage::ChangeUsername => text.literal("Сменить имя пользователя"),
        AccountMessage::VerificationSent => text.literal("Письмо для подтверждения отправлено. Проверьте почту."),
        AccountMessage::Verified => text.literal("Подтверждён"),
        AccountMessage::Unverified => text.literal("Не подтверждён"),
        AccountMessage::VerifiedText => text.literal("Вы подтвердили, что этот адрес принадлежит вам."),
        AccountMessage::PendingText => text.literal("Подтвердите адрес по ссылке из отправленного вам письма."),
        AccountMessage::UnavailableText => text.literal("Электронная почта на этом сервере недоступна, поэтому адрес нельзя подтвердить."),
        AccountMessage::Resend => text.literal("Отправить письмо для подтверждения повторно"),
        AccountMessage::EmailAddress => text.literal("Адрес электронной почты"),
        AccountMessage::EmailUpdated => text.literal("Адрес электронной почты обновлён."),
        AccountMessage::Email => text.literal("Электронная почта"),
        AccountMessage::EmailHintLead => text.literal("Входить можно с этим адресом или с именем пользователя. Текущий адрес:"),
        AccountMessage::EmailHintTail => text.literal("."),
        AccountMessage::UpdateEmail => text.literal("Обновить адрес"),
        AccountMessage::Password => text.literal("Пароль"),
        AccountMessage::PasswordChanged => text.literal("Пароль изменён."),
        AccountMessage::NewPassword => text.literal("Новый пароль"),
        AccountMessage::ConfirmPassword => text.literal("Повторите новый пароль"),
        AccountMessage::ChangePassword => text.literal("Сменить пароль"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal("Администратор создал вашу учётную запись с начальным паролем. Выберите новый пароль, прежде чем продолжить работу с ");
            text.argument(site);
            text.literal(".");
        }
        AccountText::PasswordHint(length) => {
            text.literal("Не менее ");
            text.number(length);
            text.literal(" ");
            text.literal(match text.plural(length) {
                Plural::One => "символа",
                _ => "символов",
            });
            text.literal(". Несколько не связанных между собой слов — надёжный и запоминающийся пароль.");
        }
    }
}
