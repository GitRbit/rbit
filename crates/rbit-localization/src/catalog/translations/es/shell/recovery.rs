//! Spanish (`es`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;
use crate::catalog::shell::recovery::RecoverySentence;

entries! { Es:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("Restablece tu contraseña"),
        RecoveryMessage::ForgotLede => text.literal(
            "Introduce la dirección de correo electrónico de tu cuenta y te enviaremos un enlace para elegir una \
             contraseña nueva.",
        ),
        RecoveryMessage::ForgotWithoutEmail => text.literal(
            "El correo electrónico no está disponible en este servidor, así que las contraseñas no se pueden \
             restablecer por correo. Pide a un administrador que te asigne una contraseña nueva.",
        ),
        RecoveryMessage::CheckEmail => text.literal("Revisa tu correo"),
        RecoveryMessage::LinkSent => text.literal(
            "Si alguna cuenta usa esa dirección, le enviamos un enlace para elegir una contraseña nueva. El enlace \
             funciona una sola vez y caduca en una hora.",
        ),
        RecoveryMessage::TryAgain => text.literal("vuelve a intentarlo"),
        RecoveryMessage::SendResetLink => text.literal("Enviar enlace de restablecimiento"),
        RecoveryMessage::RequestNewLink => text.literal("Solicitar un enlace nuevo"),
        RecoveryMessage::NewLinkWithoutEmail => text.literal(
            "El correo electrónico no está disponible en este servidor, así que no se puede enviar un enlace nuevo. \
             Pide a un administrador que te asigne una contraseña nueva.",
        ),
        RecoveryMessage::ChooseNewPassword => text.literal("Elige una contraseña nueva"),
        RecoveryMessage::ResetFor(username) => {
            text.literal("Para ");
            text.argument(username);
            text.literal(". A partir de ahora iniciarás sesión con ella.");
        }
        RecoveryMessage::NewPassword => text.literal("Contraseña nueva"),
        RecoveryMessage::ConfirmNewPassword => text.literal("Confirmar contraseña nueva"),
        RecoveryMessage::ChangePassword => text.literal("Cambiar contraseña"),
        RecoveryMessage::BackToSignIn => text.literal("Volver a iniciar sesión"),
        RecoveryMessage::PasswordChanged => text.literal("Contraseña cambiada"),
        RecoveryMessage::PasswordChangedText => {
            text.literal("Tu contraseña nueva está lista. Úsala la próxima vez que inicies sesión.");
        }
        RecoveryMessage::LinkExpired => text.literal("Este enlace caducó"),
        RecoveryMessage::ResetLinkExpiredText => text.literal(
            "Los enlaces de restablecimiento de contraseña funcionan una sola vez y caducan al cabo de una hora. \
             Solicita uno nuevo para continuar.",
        ),
        RecoveryMessage::AccountSuspended => text.literal("Esta cuenta está suspendida"),
        RecoveryMessage::AccountSuspendedText => {
            text.literal("Su contraseña no se puede restablecer. Ponte en contacto con un administrador.");
        }
        RecoveryMessage::EmailVerified => text.literal("Dirección de correo electrónico verificada"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.literal("Gracias por confirmar ");
            text.argument(address);
            text.literal(".");
        }
        RecoveryMessage::VerifyLinkExpiredText => text.literal(
            "Los enlaces de verificación funcionan una sola vez, caducan al cabo de dos días y dejan de funcionar si \
             la dirección cambia. Inicia sesión y envía uno nuevo desde la configuración de tu cuenta.",
        ),
        RecoveryMessage::GoToAccountSettings => text.literal("Ir a la configuración de la cuenta"),
        RecoveryMessage::SignIn => text.literal("Iniciar sesión"),
    }
    RecoverySentence => RichWriter |text| {
        RecoverySentence::NothingArrived => {
            text.literal("¿No llegó nada? Revisa tu carpeta de spam o ");
            text.slot(0);
            text.literal(".");
        }
        RecoverySentence::RememberedSignIn => {
            text.literal("¿La recordaste? ");
            text.slot(0);
        }
    }
}
