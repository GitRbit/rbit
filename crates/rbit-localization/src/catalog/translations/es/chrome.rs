//! Spanish (`es`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::{ChromeMessage, ChromeSentence};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

entries! { Es:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("Saltar al contenido"),
        ChromeMessage::Main => text.literal("Principal"),
        ChromeMessage::Explore => text.literal("Explorar"),
        ChromeMessage::Language => text.literal("Idioma"),
        ChromeMessage::SwitchTheme => text.literal("Cambiar entre claro y oscuro"),
        ChromeMessage::ChooseTheme => text.literal("Elegir un tema"),
        ChromeMessage::Notifications => text.literal("Notificaciones"),
        ChromeMessage::Unread => text.literal("Tienes notificaciones sin leer"),
        ChromeMessage::NewRepository => text.literal("Nuevo repositorio"),
        ChromeMessage::AccountMenu => text.literal("Menú de la cuenta"),
        ChromeMessage::Issues => text.literal("Incidencias"),
        ChromeMessage::Pulls => text.literal("Solicitudes de cambios"),
        ChromeMessage::Dashboard => text.literal("Panel"),
        ChromeMessage::Profile => text.literal("Perfil"),
        ChromeMessage::NewOrganization => text.literal("Nueva organización"),
        ChromeMessage::Settings => text.literal("Configuración"),
        ChromeMessage::Administration => text.literal("Administración del sitio"),
        ChromeMessage::SignOut => text.literal("Cerrar sesión"),
        ChromeMessage::SignIn => text.literal("Iniciar sesión"),
        ChromeMessage::SignUp => text.literal("Registrarse"),
        ChromeMessage::Menu => text.literal("Menú"),
        ChromeMessage::CreateAccount => text.literal("Crear una cuenta"),
        ChromeMessage::Themes => text.literal("Temas"),
        ChromeMessage::LightMode => text.literal("Modo claro"),
        ChromeMessage::DarkMode => text.literal("Modo oscuro"),
        ChromeMessage::UpdateAvailable => text.literal("Actualización disponible"),
        ChromeMessage::TimingHelp => text.literal("Tiempo del servidor para la última página o actualización"),
        ChromeMessage::Footer => text.literal("Pie de página"),
        ChromeMessage::Support => text.literal("Ayuda"),
        ChromeMessage::Terms => text.literal("Condiciones"),
        ChromeMessage::Privacy => text.literal("Privacidad"),
        ChromeMessage::Status => text.literal("Estado"),
        ChromeMessage::Home { site } => {
            text.literal("Inicio de ");
            text.argument(site);
        }
    }
    ChromeSentence => RichWriter |text| {
        ChromeSentence::RenderedIn => {
            text.literal("generado en ");
            text.slot(0);
        }
        ChromeSentence::PoweredBy => {
            text.literal("Funciona con ");
            text.slot(0);
        }
    }
}
