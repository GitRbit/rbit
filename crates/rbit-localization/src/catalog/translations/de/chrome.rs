//! German (`de`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::{ChromeMessage, ChromeSentence};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

entries! { De:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("Zum Inhalt springen"),
        ChromeMessage::Main => text.literal("Hauptnavigation"),
        ChromeMessage::Explore => text.literal("Entdecken"),
        ChromeMessage::Language => text.literal("Sprache"),
        ChromeMessage::SwitchTheme => text.literal("Zwischen hell und dunkel wechseln"),
        ChromeMessage::ChooseTheme => text.literal("Design auswählen"),
        ChromeMessage::Notifications => text.literal("Benachrichtigungen"),
        ChromeMessage::Unread => text.literal("Sie haben ungelesene Benachrichtigungen"),
        ChromeMessage::NewRepository => text.literal("Neues Repository"),
        ChromeMessage::AccountMenu => text.literal("Kontomenü"),
        ChromeMessage::Issues => text.literal("Issues"),
        ChromeMessage::Pulls => text.literal("Pull-Requests"),
        ChromeMessage::Dashboard => text.literal("Dashboard"),
        ChromeMessage::Profile => text.literal("Profil"),
        ChromeMessage::NewOrganization => text.literal("Neue Organisation"),
        ChromeMessage::Settings => text.literal("Einstellungen"),
        ChromeMessage::Administration => text.literal("Website-Administration"),
        ChromeMessage::SignOut => text.literal("Abmelden"),
        ChromeMessage::SignIn => text.literal("Anmelden"),
        ChromeMessage::SignUp => text.literal("Registrieren"),
        ChromeMessage::Menu => text.literal("Menü"),
        ChromeMessage::CreateAccount => text.literal("Konto erstellen"),
        ChromeMessage::Themes => text.literal("Designs"),
        ChromeMessage::LightMode => text.literal("Heller Modus"),
        ChromeMessage::DarkMode => text.literal("Dunkler Modus"),
        ChromeMessage::UpdateAvailable => text.literal("Update verfügbar"),
        ChromeMessage::TimingHelp => text.literal("Serverzeit für die letzte Seite oder Aktualisierung"),
        ChromeMessage::Footer => text.literal("Fußzeile"),
        ChromeMessage::Support => text.literal("Support"),
        ChromeMessage::Terms => text.literal("Nutzungsbedingungen"),
        ChromeMessage::Privacy => text.literal("Datenschutz"),
        ChromeMessage::Status => text.literal("Status"),
        ChromeMessage::Home { site } => {
            text.literal("Startseite von ");
            text.argument(site);
        }
    }
    ChromeSentence => RichWriter |text| {
        ChromeSentence::RenderedIn => {
            text.literal("erstellt in ");
            text.slot(0);
        }
        ChromeSentence::PoweredBy => {
            text.literal("Betrieben mit ");
            text.slot(0);
        }
    }
}
