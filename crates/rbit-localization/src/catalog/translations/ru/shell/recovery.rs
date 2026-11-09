//! Russian (`ru`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;
use crate::catalog::shell::recovery::RecoverySentence;

entries! { Ru:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("Сброс пароля"),
        RecoveryMessage::ForgotLede => text.literal("Введите адрес электронной почты вашей учётной записи, и мы отправим ссылку для выбора нового пароля."),
        RecoveryMessage::ForgotWithoutEmail => text.literal("Электронная почта на этом сервере недоступна, поэтому пароль нельзя сбросить по почте. Попросите администратора задать вам новый пароль."),
        RecoveryMessage::CheckEmail => text.literal("Проверьте почту"),
        RecoveryMessage::LinkSent => text.literal("Если этот адрес принадлежит учётной записи, мы отправили на него ссылку для выбора нового пароля. Ссылка одноразовая и действует один час."),
        RecoveryMessage::TryAgain => text.literal("попробуйте ещё раз"),
        RecoveryMessage::SendResetLink => text.literal("Отправить ссылку для сброса"),
        RecoveryMessage::RequestNewLink => text.literal("Запросить новую ссылку"),
        RecoveryMessage::NewLinkWithoutEmail => text.literal("Электронная почта на этом сервере недоступна, поэтому новую ссылку отправить нельзя. Попросите администратора задать вам новый пароль."),
        RecoveryMessage::ChooseNewPassword => text.literal("Выберите новый пароль"),
        RecoveryMessage::ResetFor(username) => {
            text.literal("Учётная запись: ");
            text.argument(username);
            text.literal(". Теперь вы будете входить с этим паролем.");
        }
        RecoveryMessage::NewPassword => text.literal("Новый пароль"),
        RecoveryMessage::ConfirmNewPassword => text.literal("Повторите новый пароль"),
        RecoveryMessage::ChangePassword => text.literal("Сменить пароль"),
        RecoveryMessage::BackToSignIn => text.literal("Вернуться ко входу"),
        RecoveryMessage::PasswordChanged => text.literal("Пароль изменён"),
        RecoveryMessage::PasswordChangedText => text.literal("Новый пароль установлен. Используйте его при следующем входе."),
        RecoveryMessage::LinkExpired => text.literal("Срок действия ссылки истёк"),
        RecoveryMessage::ResetLinkExpiredText => text.literal("Ссылки для сброса пароля одноразовые и действуют один час. Запросите новую, чтобы продолжить."),
        RecoveryMessage::AccountSuspended => text.literal("Эта учётная запись приостановлена"),
        RecoveryMessage::AccountSuspendedText => text.literal("Её пароль нельзя сбросить. Обратитесь к администратору."),
        RecoveryMessage::EmailVerified => text.literal("Адрес электронной почты подтверждён"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.literal("Спасибо за подтверждение адреса ");
            text.argument(address);
            text.literal(".");
        }
        RecoveryMessage::VerifyLinkExpiredText => text.literal("Ссылки для подтверждения одноразовые, действуют два дня и перестают работать при смене адреса. Войдите и отправьте новую из настроек учётной записи."),
        RecoveryMessage::GoToAccountSettings => text.literal("Перейти к настройкам учётной записи"),
        RecoveryMessage::SignIn => text.literal("Войти"),
    }
    RecoverySentence => RichWriter |text| {
        RecoverySentence::NothingArrived => {
            text.literal("Письмо не пришло? Проверьте папку «Спам» или ");
            text.slot(0);
            text.literal(".");
        }
        RecoverySentence::RememberedSignIn => {
            text.literal("Вспомнили пароль? ");
            text.slot(0);
        }
    }
}
