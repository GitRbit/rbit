//! Spanish (`es`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::{Attempt, AuthMessage};
use crate::plural::Plural;

entries! { Es:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle | AuthMessage::SignIn => text.literal("Iniciar sesión"),
        AuthMessage::SignUpTitle | AuthMessage::SignUp => text.literal("Registrarse"),
        AuthMessage::SignInTo(site) => {
            text.literal("Inicia sesión en ");
            text.argument(site);
        }
        AuthMessage::SignInLede => text.literal("Usa tu nombre de usuario o tu dirección de correo electrónico."),
        AuthMessage::UsernameOrEmail => text.literal("Nombre de usuario o correo electrónico"),
        AuthMessage::Password => text.literal("Contraseña"),
        AuthMessage::ForgotPassword => text.literal("¿Olvidaste tu contraseña?"),
        AuthMessage::Or => text.literal("o"),
        AuthMessage::SignInWith(provider) => {
            text.literal("Iniciar sesión con ");
            text.argument(provider);
        }
        AuthMessage::NoAccount => text.literal("¿No tienes una cuenta?"),
        AuthMessage::CreateYourAccount => text.literal("Crea tu cuenta"),
        AuthMessage::HostYourCode(site) => {
            text.literal("Aloja tu código en ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::Username => text.literal("Nombre de usuario"),
        AuthMessage::UsernameHint => text.literal("Letras, dígitos, «-», «_» y «.»"),
        AuthMessage::Email => text.literal("Correo electrónico"),
        AuthMessage::MinimumLength(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.literal("Al menos ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " carácter.",
                _ => " caracteres.",
            });
        }
        AuthMessage::SetupToken => text.literal("Token de configuración"),
        AuthMessage::SetupTokenHint => {
            text.literal("Esta primera cuenta administra el servidor. Encontrarás el token en el registro del servidor.");
        }
        AuthMessage::CreateAccount => text.literal("Crear cuenta"),
        AuthMessage::RegistrationClosed => {
            text.literal("El registro está cerrado en este servidor. Pide una cuenta a un administrador.");
        }
        AuthMessage::HaveAccount => text.literal("¿Ya tienes una cuenta?"),
        AuthMessage::TwoFactorTitle => text.literal("Autenticación en dos pasos"),
        AuthMessage::PasskeyOrApp => {
            text.literal("Usa tu llave de acceso o introduce el código de 6 dígitos de tu aplicación de autenticación.");
        }
        AuthMessage::PasskeyOnly => {
            text.literal("Usa tu llave de acceso o tu llave de seguridad para terminar de iniciar sesión.");
        }
        AuthMessage::AppOnly => text.literal("Introduce el código de 6 dígitos de tu aplicación de autenticación."),
        AuthMessage::UsePasskey => text.literal("Usar una llave de acceso"),
        AuthMessage::PasskeyUnsupported => {
            text.literal("Este navegador no admite llaves de acceso. Introduce un código en su lugar.");
        }
        AuthMessage::AuthenticationCode => text.literal("Código de autenticación"),
        AuthMessage::LostDevice => {
            text.literal("¿Perdiste tu dispositivo? Introduce uno de tus códigos de recuperación en su lugar.");
        }
        AuthMessage::RecoveryCode => text.literal("Código de recuperación"),
        AuthMessage::LostPasskey => {
            text.literal("¿Perdiste tu llave de acceso? Introduce uno de tus códigos de recuperación.");
        }
        AuthMessage::VerifyCode => text.literal("Verificar código"),
        AuthMessage::NotYou => text.literal("¿No eres tú?"),
        AuthMessage::DifferentAccount => text.literal("Iniciar sesión con otra cuenta"),
        AuthMessage::ProviderLede(provider, site) => {
            text.literal("Iniciaste sesión con ");
            text.argument(provider);
            text.literal(". Elige un nombre de usuario en ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.literal("Verificada por ");
            text.argument(provider);
            text.literal(". Otra dirección debe confirmarse por correo electrónico.");
        }
        AuthMessage::ConfirmByEmail => text.literal("Recibirás un correo para confirmar esta dirección."),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.literal("Iniciarás sesión con ");
            text.argument(provider);
            text.literal(
                "; tu cuenta no tiene contraseña hasta que definas una con un enlace de restablecimiento. Git por \
                 HTTPS necesita un token de acceso personal.",
            );
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("y vincula ");
            text.argument(provider);
            text.literal(" en tu configuración de seguridad.");
        }
        AuthMessage::SignInFailed => text.literal("No se pudo iniciar sesión"),
        AuthMessage::BackToSignIn => text.literal("Volver a iniciar sesión"),
        AuthMessage::BackToSecuritySettings => text.literal("Volver a la configuración de seguridad"),
        AuthMessage::IncorrectCredentials => text.literal("Nombre de usuario o contraseña incorrectos."),
        AuthMessage::Suspended => text.literal("Esta cuenta está suspendida. Ponte en contacto con un administrador."),
        AuthMessage::SignInExpired => text.literal("Tu sesión caducó. Vuelve a iniciar sesión."),
        AuthMessage::TooManyAttempts(attempt, seconds) => {
            text.literal(match attempt {
                Attempt::SignIn => "Demasiados intentos de inicio de sesión. Vuelve a intentarlo en ",
                Attempt::SignUp => "Demasiados intentos de registro. Vuelve a intentarlo en ",
            });
            if seconds < 60 {
                text.number(seconds);
                text.literal(match text.plural(seconds) {
                    Plural::One => " segundo.",
                    _ => " segundos.",
                });
            } else {
                let minutes = seconds.div_ceil(60);
                text.number(minutes);
                text.literal(match text.plural(minutes) {
                    Plural::One => " minuto.",
                    _ => " minutos.",
                });
            }
        }
    }
}
