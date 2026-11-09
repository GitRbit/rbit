//! Russian (`ru`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::Attempt;
use crate::catalog::shell::auth::AuthMessage;
use crate::plural::Plural;

entries! { Ru:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle => text.literal("Вход"),
        AuthMessage::SignUpTitle => text.literal("Регистрация"),
        AuthMessage::SignInTo(site) => {
            text.literal("Вход в ");
            text.argument(site);
        }
        AuthMessage::SignInLede => text.literal("Используйте имя пользователя или адрес электронной почты."),
        AuthMessage::UsernameOrEmail => text.literal("Имя пользователя или эл. почта"),
        AuthMessage::Password => text.literal("Пароль"),
        AuthMessage::ForgotPassword => text.literal("Забыли пароль?"),
        AuthMessage::SignIn => text.literal("Войти"),
        AuthMessage::Or => text.literal("или"),
        AuthMessage::SignInWith(provider) => {
            text.literal("Войти через ");
            text.argument(provider);
        }
        AuthMessage::NoAccount => text.literal("Нет учётной записи?"),
        AuthMessage::SignUp => text.literal("Зарегистрироваться"),
        AuthMessage::CreateYourAccount => text.literal("Создайте учётную запись"),
        AuthMessage::HostYourCode(site) => {
            text.literal("Размещайте свой код на ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::Username => text.literal("Имя пользователя"),
        AuthMessage::UsernameHint => text.literal("Буквы, цифры, «-», «_» и «.»"),
        AuthMessage::Email => text.literal("Электронная почта"),
        AuthMessage::MinimumLength(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.literal("Не менее ");
            text.number(count);
            text.literal(match text.plural(count) { Plural::One => " символа.", _ => " символов." });
        }
        AuthMessage::SetupToken => text.literal("Токен настройки"),
        AuthMessage::SetupTokenHint => text.literal("Эта первая учётная запись управляет сервером. Токен указан в журнале сервера."),
        AuthMessage::CreateAccount => text.literal("Создать учётную запись"),
        AuthMessage::RegistrationClosed => text.literal("Регистрация на этом сервере закрыта. Попросите учётную запись у администратора."),
        AuthMessage::HaveAccount => text.literal("Уже есть учётная запись?"),
        AuthMessage::TwoFactorTitle => text.literal("Двухфакторная аутентификация"),
        AuthMessage::PasskeyOrApp => text.literal("Используйте ключ доступа или введите 6-значный код из приложения-аутентификатора."),
        AuthMessage::PasskeyOnly => text.literal("Чтобы завершить вход, используйте ключ доступа или ключ безопасности."),
        AuthMessage::AppOnly => text.literal("Введите 6-значный код из приложения-аутентификатора."),
        AuthMessage::UsePasskey => text.literal("Использовать ключ доступа"),
        AuthMessage::PasskeyUnsupported => text.literal("Этот браузер не поддерживает ключи доступа. Введите код."),
        AuthMessage::AuthenticationCode => text.literal("Код аутентификации"),
        AuthMessage::LostDevice => text.literal("Потеряли устройство? Введите один из кодов восстановления."),
        AuthMessage::RecoveryCode => text.literal("Код восстановления"),
        AuthMessage::LostPasskey => text.literal("Потеряли ключ доступа? Введите один из кодов восстановления."),
        AuthMessage::VerifyCode => text.literal("Проверить код"),
        AuthMessage::NotYou => text.literal("Не вы?"),
        AuthMessage::DifferentAccount => text.literal("Войти с другой учётной записью"),
        AuthMessage::ProviderLede(provider, site) => {
            text.literal("Вы вошли через ");
            text.argument(provider);
            text.literal(". Выберите имя пользователя на ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.literal("Подтверждено через ");
            text.argument(provider);
            text.literal(". Другой адрес нужно подтвердить по электронной почте.");
        }
        AuthMessage::ConfirmByEmail => text.literal("Вы получите письмо для подтверждения этого адреса."),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.literal("Вы будете входить через ");
            text.argument(provider);
            text.literal("; у учётной записи не будет пароля, пока вы не зададите его по ссылке для сброса. Для Git по HTTPS нужен персональный токен доступа.");
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("и привязать ");
            text.argument(provider);
            text.literal(" в настройках безопасности.");
        }
        AuthMessage::SignInFailed => text.literal("Не удалось войти"),
        AuthMessage::BackToSignIn => text.literal("Вернуться ко входу"),
        AuthMessage::BackToSecuritySettings => text.literal("Вернуться к настройкам безопасности"),
        AuthMessage::IncorrectCredentials => text.literal("Неверное имя пользователя или пароль."),
        AuthMessage::Suspended => text.literal("Эта учётная запись приостановлена. Обратитесь к администратору."),
        AuthMessage::SignInExpired => text.literal("Сеанс входа истёк. Войдите снова."),
        AuthMessage::TooManyAttempts(attempt, seconds) => {
            text.literal(match attempt {
                Attempt::SignIn => "Слишком много попыток входа. Повторите через ",
                Attempt::SignUp => "Слишком много попыток регистрации. Повторите через ",
            });
            let (count, forms) = if seconds < 60 {
                (seconds, [" секунду.", " секунды.", " секунд."])
            } else {
                (seconds.div_ceil(60), [" минуту.", " минуты.", " минут."])
            };
            text.number(count);
            text.literal(match text.plural(count) { Plural::One => forms[0], Plural::Few => forms[1], _ => forms[2] });
        }
    }
}
