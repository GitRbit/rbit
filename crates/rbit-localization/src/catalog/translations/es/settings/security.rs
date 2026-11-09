//! Spanish (`es`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { Es:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("Seguridad"),
        SecurityMessage::Intro => text.literal(
            "Protege tu cuenta con una llave de acceso o una aplicación de autenticación, y controla dónde tienes la \
             sesión iniciada.",
        ),
        SecurityMessage::TwoFactor => text.literal("Autenticación en dos pasos"),
        SecurityMessage::On => text.literal("Activada"),
        SecurityMessage::Off => text.literal("Desactivada"),
        SecurityMessage::EnabledLead => text.literal(
            "Al iniciar sesión se pide una llave de acceso o un código después de la contraseña. Git por HTTPS \
             necesita un",
        ),
        SecurityMessage::TokenLink => text.literal("token de acceso personal"),
        SecurityMessage::EnabledTail => text.literal("; tu contraseña ya no funciona ahí."),
        SecurityMessage::Disabled => text.literal(
            "Añade un segundo paso al inicio de sesión: una llave de acceso, una aplicación de autenticación o ambas.",
        ),
        SecurityMessage::Passkeys => text.literal("Llaves de acceso y llaves de seguridad"),
        SecurityMessage::PasskeysText => text.literal(
            "Confirma los inicios de sesión con tu huella, tu cara, el PIN del dispositivo, tu teléfono o una llave \
             física como una YubiKey.",
        ),
        SecurityMessage::Remove => text.literal("Quitar"),
        SecurityMessage::OnlyFactor => text.literal(
            "Es tu único segundo factor: si lo quitas, se desactiva la autenticación en dos pasos y se eliminan tus \
             códigos de recuperación.",
        ),
        SecurityMessage::PasskeyGone => text.literal("Ya no podrás iniciar sesión con esta llave de acceso."),
        SecurityMessage::Name => text.literal("Nombre"),
        SecurityMessage::PasskeyPlaceholder => text.literal("p. ej., MacBook o YubiKey"),
        SecurityMessage::AddPasskey => text.literal("Añadir llave de acceso"),
        SecurityMessage::PasskeyUnsupported => text.literal(
            "Para añadir una llave de acceso hace falta un navegador compatible con llaves de acceso y JavaScript \
             activado.",
        ),
        SecurityMessage::PasskeyUrlLead => text.literal("Las llaves de acceso necesitan que"),
        SecurityMessage::PasskeyUrlTail => text.literal("tenga como valor la dirección de este sitio."),
        SecurityMessage::Authenticator => text.literal("Aplicación de autenticación"),
        SecurityMessage::AuthenticatorText => text.literal(
            "Usa códigos de 6 dígitos de una aplicación como 1Password, Aegis o Google Authenticator.",
        ),
        SecurityMessage::SetUp => text.literal("Configurar"),
        SecurityMessage::RemoveAuthenticator => text.literal("Quitar la aplicación de autenticación"),
        SecurityMessage::PasswordAlone => text.literal(
            "Tu cuenta quedará protegida solo por tu contraseña y se eliminarán tus códigos de recuperación.",
        ),
        SecurityMessage::PasskeysStay => {
            text.literal("Tus llaves de acceso siguen activas como segundo factor.");
        }
        SecurityMessage::RecoveryCodes => text.literal("Códigos de recuperación"),
        SecurityMessage::Regenerate => text.literal("Generar nuevos códigos de recuperación"),
        SecurityMessage::CodesStop => text.literal("Tus códigos de recuperación actuales dejan de funcionar."),
        SecurityMessage::GenerateCodes => text.literal("Generar códigos nuevos"),
        SecurityMessage::SignInMethods => text.literal("Métodos de inicio de sesión"),
        SecurityMessage::Password => text.literal("Contraseña"),
        SecurityMessage::NoPassword => text.literal("Sin contraseña"),
        SecurityMessage::WithPassword => {
            text.literal("Inicia sesión con tu contraseña o con una cuenta vinculada.");
        }
        SecurityMessage::LinkedOnlyLead => text.literal(
            "Tu cuenta no tiene contraseña: inicias sesión con una cuenta vinculada. Para definir una, solicita un",
        ),
        SecurityMessage::ResetLink => text.literal("enlace de restablecimiento de contraseña"),
        SecurityMessage::LinkedOnlyMiddle => text.literal(". Git por HTTPS necesita un"),
        SecurityMessage::LinkedOnlyTail => text.literal("."),
        SecurityMessage::NoLongerAvailable => text.literal("Ya no está disponible"),
        SecurityMessage::Unlink => text.literal("Desvincular"),
        SecurityMessage::OnlyWay => text.literal("Única forma de iniciar sesión"),
        SecurityMessage::NotLinked => text.literal("Sin vincular"),
        SecurityMessage::Link => text.literal("Vincular"),
        SecurityMessage::Sessions => text.literal("Sesiones"),
        SecurityMessage::SignOutOthers => text.literal("Cerrar sesión en los demás dispositivos"),
        SecurityMessage::ThisDevice => text.literal("Este dispositivo"),
        SecurityMessage::SignOut => text.literal("Cerrar sesión"),
        SecurityMessage::RecentActivity => text.literal("Actividad reciente"),
        SecurityMessage::NoActivity => text.literal("Aún no hay actividad"),
        SecurityMessage::NoActivityText => text.literal(
            "Aquí aparecerán los inicios de sesión y los cambios de seguridad de tu cuenta.",
        ),
        SecurityMessage::SetupTitle => text.literal("Configurar una aplicación de autenticación"),
        SecurityMessage::SetupIntro => text.literal(
            "Escanea el código QR con tu aplicación de autenticación e introduce el código de 6 dígitos que muestra.",
        ),
        SecurityMessage::CantScan => text.literal("¿No puedes escanearlo? Introduce esta clave en su lugar"),
        SecurityMessage::CopyKey => text.literal("Copiar clave"),
        SecurityMessage::Copy => text.literal("Copiar"),
        SecurityMessage::TimeBased => text.literal("Basado en el tiempo, 6 dígitos, cada 30 segundos."),
        SecurityMessage::ConfirmCode => text.literal("2. Confirma un código"),
        SecurityMessage::AuthenticationCode => text.literal("Código de autenticación"),
        SecurityMessage::Cancel => text.literal("Cancelar"),
        SecurityMessage::Enable => text.literal("Activar"),
        SecurityMessage::TwoFactorOn => text.literal("La autenticación en dos pasos está activada."),
        SecurityMessage::SaveCodes => text.literal("Guarda ahora estos códigos"),
        SecurityMessage::CodesText => text.literal(
            "Cada código te permite iniciar sesión una vez si pierdes tus llaves de acceso o tu aplicación de \
             autenticación. No se volverán a mostrar, así que guárdalos en un lugar seguro, como un gestor de \
             contraseñas.",
        ),
        SecurityMessage::SavedCodes => text.literal("Ya guardé mis códigos"),
        SecurityMessage::AppOrRecoveryCode => text.literal("Código de autenticación o de recuperación"),
        SecurityMessage::RecoveryCode => text.literal("Código de recuperación"),
        SecurityMessage::PasskeyInstead => text.literal(
            "O confirma con una de tus llaves de acceso en lugar de un código.",
        ),
        SecurityMessage::AuthenticatorRemoved => text.literal("Se quitó la aplicación de autenticación."),
        SecurityMessage::AuthenticatorAdded => text.literal("Se añadió la aplicación de autenticación."),
        SecurityMessage::PasskeyAdded => text.literal("Se añadió la llave de acceso."),
        SecurityMessage::PasskeyRemoved => text.literal("Se quitó la llave de acceso."),
        SecurityMessage::SessionRevoked => text.literal("Se cerró la sesión."),
        SecurityMessage::OtherSessionsRevoked => text.literal("Se cerraron todas las demás sesiones."),
        SecurityMessage::SetupCancelled => {
            text.literal("Se canceló la configuración de la autenticación en dos pasos.");
        }
        SecurityMessage::IdentityLinked => {
            text.literal("Cuenta vinculada. Ya puedes iniciar sesión con ella.");
        }
        SecurityMessage::IdentityUnlinked => text.literal("Cuenta desvinculada."),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. Añade ");
            text.argument(site);
            text.literal(" a tu aplicación");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.literal("Desvincular ");
            text.argument(provider);
        }
        SecurityText::SignOutLabel(device) => {
            text.literal("Cerrar la sesión de ");
            text.argument(device);
        }
        SecurityText::WithPasskey(action) => {
            text.argument(action);
            text.literal(" con una llave de acceso");
        }
    }
}
