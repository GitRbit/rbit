//! Spanish (`es`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};
use crate::plural::Plural;

entries! { Es:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("Actualiza tu contraseña"),
        AccountMessage::Title => text.literal("Cuenta"),
        AccountMessage::Intro => text.literal("Cómo inicias sesión. Los cambios piden tu contraseña actual."),
        AccountMessage::Username => text.literal("Nombre de usuario"),
        AccountMessage::UsernameUpdated => text.literal("Nombre de usuario actualizado."),
        AccountMessage::UsernameHint => text.literal(
            "Los enlaces al perfil y los remotos de Git que usan tu nombre anterior siguen funcionando hasta que \
             alguien ocupe ese nombre. Actualiza tus enlaces y remotos. Inicia sesión con tu nuevo nombre de usuario \
             o tu correo electrónico.",
        ),
        AccountMessage::CurrentPassword => text.literal("Contraseña actual"),
        AccountMessage::ChangeUsername => text.literal("Cambiar el nombre de usuario"),
        AccountMessage::VerificationSent => {
            text.literal("Se envió el correo de verificación. Revisa tu bandeja de entrada.");
        }
        AccountMessage::Verified => text.literal("Verificada"),
        AccountMessage::Unverified => text.literal("Sin verificar"),
        AccountMessage::VerifiedText => text.literal("Confirmaste que esta dirección es tuya."),
        AccountMessage::PendingText => {
            text.literal("Confirma la dirección con el enlace que te enviamos por correo.");
        }
        AccountMessage::UnavailableText => text.literal(
            "El correo electrónico no está disponible en este servidor, así que la dirección no se puede verificar.",
        ),
        AccountMessage::Resend => text.literal("Reenviar el correo de verificación"),
        AccountMessage::EmailAddress => text.literal("Dirección de correo electrónico"),
        AccountMessage::EmailUpdated => text.literal("Dirección de correo electrónico actualizada."),
        AccountMessage::Email => text.literal("Correo electrónico"),
        AccountMessage::EmailHintLead => text.literal(
            "Puedes iniciar sesión con esta dirección o con tu nombre de usuario. Actualmente",
        ),
        AccountMessage::EmailHintTail => text.literal("."),
        AccountMessage::UpdateEmail => text.literal("Actualizar el correo electrónico"),
        AccountMessage::Password => text.literal("Contraseña"),
        AccountMessage::PasswordChanged => text.literal("Contraseña cambiada."),
        AccountMessage::NewPassword => text.literal("Contraseña nueva"),
        AccountMessage::ConfirmPassword => text.literal("Confirmar contraseña nueva"),
        AccountMessage::ChangePassword => text.literal("Cambiar contraseña"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal(
                "Un administrador creó tu cuenta con una contraseña inicial. Elige una contraseña nueva antes de \
                 continuar a ",
            );
            text.argument(site);
            text.literal(".");
        }
        AccountText::PasswordHint(length) => {
            text.literal("Al menos ");
            text.number(length);
            text.literal(match text.plural(length) {
                Plural::One => " carácter.",
                _ => " caracteres.",
            });
            text.literal(" Unas cuantas palabras sin relación forman una contraseña segura y fácil de recordar.");
        }
    }
}
