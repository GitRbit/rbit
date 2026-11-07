//! Shared navigation text; static English/Spanish labels borrow compiled literals.
use super::{Catalog, LocalizedText};
use crate::Locale;
use std::fmt;

#[derive(Clone, Copy, Debug)]
pub enum ChromeMessage<'a> {
    Skip,
    Main,
    Explore,
    SwitchTheme,
    ChooseTheme,
    Notifications,
    Unread,
    NewRepository,
    AccountMenu,
    Issues,
    Pulls,
    Dashboard,
    Profile,
    NewOrganization,
    Settings,
    Administration,
    SignOut,
    SignIn,
    SignUp,
    Menu,
    CreateAccount,
    Themes,
    LightMode,
    DarkMode,
    PoweredBy,
    UpdateAvailable,
    TimingHelp,
    RenderedIn,
    Footer,
    Support,
    Terms,
    Privacy,
    Status,
    Home { site: &'a str },
}
impl ChromeMessage<'_> {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Skip => "chrome.skip",
            Self::Main => "chrome.main",
            Self::Explore => "chrome.explore",
            Self::SwitchTheme => "chrome.switch-theme",
            Self::ChooseTheme => "chrome.choose-theme",
            Self::Notifications => "chrome.notifications",
            Self::Unread => "chrome.unread",
            Self::NewRepository => "chrome.new-repository",
            Self::AccountMenu => "chrome.account-menu",
            Self::Issues => "chrome.issues",
            Self::Pulls => "chrome.pulls",
            Self::Dashboard => "chrome.dashboard",
            Self::Profile => "chrome.profile",
            Self::NewOrganization => "chrome.new-organization",
            Self::Settings => "chrome.settings",
            Self::Administration => "chrome.administration",
            Self::SignOut => "chrome.sign-out",
            Self::SignIn => "chrome.sign-in",
            Self::SignUp => "chrome.sign-up",
            Self::Menu => "chrome.menu",
            Self::CreateAccount => "chrome.create-account",
            Self::Themes => "chrome.themes",
            Self::LightMode => "chrome.light-mode",
            Self::DarkMode => "chrome.dark-mode",
            Self::PoweredBy => "chrome.powered-by",
            Self::UpdateAvailable => "chrome.update",
            Self::TimingHelp => "chrome.timing-help",
            Self::RenderedIn => "chrome.rendered-in",
            Self::Footer => "chrome.footer",
            Self::Support => "chrome.support",
            Self::Terms => "chrome.terms",
            Self::Privacy => "chrome.privacy",
            Self::Status => "chrome.status",
            Self::Home { .. } => "chrome.home",
        }
    }
}
impl Catalog {
    pub fn chrome(self, message: ChromeMessage<'_>) -> LocalizedText {
        let (english, spanish) = match message {
            ChromeMessage::Skip => ("Skip to content", "Saltar al contenido"),
            ChromeMessage::Main => ("Main", "Principal"),
            ChromeMessage::Explore => ("Explore", "Explorar"),
            ChromeMessage::SwitchTheme => ("Switch between light and dark", "Cambiar entre claro y oscuro"),
            ChromeMessage::ChooseTheme => ("Choose a theme", "Elegir un tema"),
            ChromeMessage::Notifications => ("Notifications", "Notificaciones"),
            ChromeMessage::Unread => ("You have unread notifications", "Tienes notificaciones sin leer"),
            ChromeMessage::NewRepository => ("New repository", "Nuevo repositorio"),
            ChromeMessage::AccountMenu => ("Account menu", "Menú de la cuenta"),
            ChromeMessage::Issues => ("Issues", "Incidencias"),
            ChromeMessage::Pulls => ("Pull requests", "Solicitudes de cambios"),
            ChromeMessage::Dashboard => ("Dashboard", "Panel"),
            ChromeMessage::Profile => ("Profile", "Perfil"),
            ChromeMessage::NewOrganization => ("New organization", "Nueva organización"),
            ChromeMessage::Settings => ("Settings", "Configuración"),
            ChromeMessage::Administration => ("Site administration", "Administración del sitio"),
            ChromeMessage::SignOut => ("Sign out", "Cerrar sesión"),
            ChromeMessage::SignIn => ("Sign in", "Iniciar sesión"),
            ChromeMessage::SignUp => ("Sign up", "Registrarse"),
            ChromeMessage::Menu => ("Menu", "Menú"),
            ChromeMessage::CreateAccount => ("Create an account", "Crear una cuenta"),
            ChromeMessage::Themes => ("Themes", "Temas"),
            ChromeMessage::LightMode => ("Light mode", "Modo claro"),
            ChromeMessage::DarkMode => ("Dark mode", "Modo oscuro"),
            ChromeMessage::PoweredBy => ("Powered by", "Funciona con"),
            ChromeMessage::UpdateAvailable => ("Update available", "Actualización disponible"),
            ChromeMessage::TimingHelp => {
                ("Server time for the last page or update", "Tiempo del servidor para la última página o actualización")
            }
            ChromeMessage::RenderedIn => ("rendered in", "generado en"),
            ChromeMessage::Footer => ("Footer", "Pie de página"),
            ChromeMessage::Support => ("Support", "Ayuda"),
            ChromeMessage::Terms => ("Terms", "Condiciones"),
            ChromeMessage::Privacy => ("Privacy", "Privacidad"),
            ChromeMessage::Status => ("Status", "Estado"),
            ChromeMessage::Home { site } => {
                let mut value = String::new();
                if self.locale == Locale::Spanish {
                    value.push_str("Inicio de ");
                    value.push_str(site);
                } else {
                    self.argument(&mut value, site);
                    self.literal(&mut value, " home");
                }
                return self.finish(value);
            }
        };
        self.literal_text(if self.locale == Locale::Spanish { spanish } else { english })
    }

    pub const fn chrome_display(self, message: ChromeMessage<'_>) -> ChromeDisplay<'_> {
        ChromeDisplay { catalog: self, message }
    }
}

/// Dynamic home labels borrow the site name and format directly in real catalogs.
#[derive(Clone, Copy, Debug)]
pub struct ChromeDisplay<'a> {
    catalog: Catalog,
    message: ChromeMessage<'a>,
}
impl fmt::Display for ChromeDisplay<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let ChromeMessage::Home { site } = self.message {
            match self.catalog.locale {
                Locale::English => return write!(output, "{site} home"),
                Locale::Spanish => return write!(output, "Inicio de {site}"),
                _ => {}
            }
        }
        fmt::Display::fmt(&self.catalog.chrome(self.message), output)
    }
}
