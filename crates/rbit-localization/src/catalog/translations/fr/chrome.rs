//! French (`fr`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::{ChromeMessage, ChromeSentence};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

entries! { Fr:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("Aller au contenu"),
        ChromeMessage::Main => text.literal("Navigation principale"),
        ChromeMessage::Explore => text.literal("Explorer"),
        ChromeMessage::Language => text.literal("Langue"),
        ChromeMessage::SwitchTheme => text.literal("Basculer entre clair et sombre"),
        ChromeMessage::ChooseTheme => text.literal("Choisir un thème"),
        ChromeMessage::Notifications => text.literal("Notifications"),
        ChromeMessage::Unread => text.literal("Vous avez des notifications non lues"),
        ChromeMessage::NewRepository => text.literal("Nouveau dépôt"),
        ChromeMessage::AccountMenu => text.literal("Menu du compte"),
        ChromeMessage::Issues => text.literal("Tickets"),
        ChromeMessage::Pulls => text.literal("Pull requests"),
        ChromeMessage::Dashboard => text.literal("Tableau de bord"),
        ChromeMessage::Profile => text.literal("Profil"),
        ChromeMessage::NewOrganization => text.literal("Nouvelle organisation"),
        ChromeMessage::Settings => text.literal("Paramètres"),
        ChromeMessage::Administration => text.literal("Administration du site"),
        ChromeMessage::SignOut => text.literal("Se déconnecter"),
        ChromeMessage::SignIn => text.literal("Se connecter"),
        ChromeMessage::SignUp => text.literal("S’inscrire"),
        ChromeMessage::Menu => text.literal("Menu"),
        ChromeMessage::CreateAccount => text.literal("Créer un compte"),
        ChromeMessage::Themes => text.literal("Thèmes"),
        ChromeMessage::LightMode => text.literal("Mode clair"),
        ChromeMessage::DarkMode => text.literal("Mode sombre"),
        ChromeMessage::UpdateAvailable => text.literal("Mise à jour disponible"),
        ChromeMessage::TimingHelp => text.literal("Temps serveur de la dernière page ou mise à jour"),
        ChromeMessage::RenderedIn => text.literal("généré en"),
        ChromeMessage::Footer => text.literal("Pied de page"),
        ChromeMessage::Support => text.literal("Assistance"),
        ChromeMessage::Terms => text.literal("Conditions"),
        ChromeMessage::Privacy => text.literal("Confidentialité"),
        ChromeMessage::Status => text.literal("État du service"),
        ChromeMessage::Home { site } => {
            text.literal("Accueil de ");
            text.argument(site);
        }
    }
    ChromeSentence => RichWriter |text| {
        ChromeSentence::PoweredBy => {
            text.literal("Propulsé par ");
            text.slot(0);
        }
    }
}
