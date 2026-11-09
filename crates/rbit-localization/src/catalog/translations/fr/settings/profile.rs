//! French (`fr`) entries for `catalog/settings/profile.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::GallerySentence;
use crate::catalog::settings::profile::ProfileSentence;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};
use crate::plural::Plural;

entries! { Fr:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("Profil"),
        ProfileMessage::IntroLead => text.literal("Affiché sur"),
        ProfileMessage::IntroLink => text.literal("votre page de profil"),
        ProfileMessage::IntroTail => text.literal(
            "et à côté de votre activité. Tout ce qui figure ici est public et facultatif.",
        ),
        ProfileMessage::Updated => text.literal("Profil mis à jour."),
        ProfileMessage::DisplayName => text.literal("Nom affiché"),
        ProfileMessage::DisplayNameHint => text.literal(
            "Affiché à la place de votre nom d’utilisateur lorsque la place le permet.",
        ),
        ProfileMessage::Bio => text.literal("Bio"),
        ProfileMessage::BioHint => text.literal("Jusqu’à 300\u{a0}caractères."),
        ProfileMessage::Website => text.literal("Site web"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("Localisation"),
        ProfileMessage::Save => text.literal("Enregistrer le profil"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("Apparence"),
        AppearanceMessage::Intro => text.literal(
            "Votre thème est enregistré dans votre compte et vous suit sur tous les appareils où vous vous connectez. \
             Survolez un thème pour le prévisualiser\u{202f}; cliquez pour le conserver.",
        ),
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("Notifications"),
        NotificationsMessage::IntroLead => text.literal("Tout continue d’apparaître dans"),
        NotificationsMessage::IntroLink => text.literal("votre boîte de réception"),
        NotificationsMessage::IntroTail => text.literal(". Choisissez ce qui est aussi envoyé à votre adresse e-mail."),
        NotificationsMessage::Saved => text.literal("Préférences enregistrées."),
        NotificationsMessage::Unavailable => text.literal(
            "L’e-mail n’est pas disponible sur ce serveur\u{a0}: les notifications n’apparaissent donc que dans votre \
             boîte de réception.",
        ),
        NotificationsMessage::VerifyLead => text.literal("Vérifiez "),
        NotificationsMessage::VerifyMiddle => text.literal(" dans les "),
        NotificationsMessage::VerifyLink => text.literal("paramètres du compte"),
        NotificationsMessage::VerifyTail => text.literal(" pour recevoir les e-mails de notification."),
        NotificationsMessage::Email => text.literal("E-mail"),
        NotificationsMessage::SendTo => text.literal("Envoyer à"),
        NotificationsMessage::When => text.literal("Quand"),
        NotificationsMessage::DigestTime => text.literal("Heure du récapitulatif"),
        NotificationsMessage::DigestHint => text.literal(
            "Les récapitulatifs omettent les conversations que vous avez déjà lues ici. Chaque e-mail contient un lien \
             qui les désactive en un clic.",
        ),
        NotificationsMessage::Save => text.literal("Enregistrer les préférences"),
        NotificationsMessage::Off => text.literal("Désactivé"),
        NotificationsMessage::OffText => text.literal("Les notifications apparaissent uniquement dans votre boîte de réception."),
        NotificationsMessage::Participating => text.literal("Participation"),
        NotificationsMessage::ParticipatingText => text.literal(
            "Les conversations que vous avez ouvertes, commentées ou relues, et lorsque vous êtes mentionné, assigné ou \
             sollicité pour une revue.",
        ),
        NotificationsMessage::Everything => text.literal("Tout"),
        NotificationsMessage::EverythingText => text.literal(
            "Aussi les conversations auxquelles vous êtes abonné sans y participer.",
        ),
        NotificationsMessage::Immediately => text.literal("Immédiatement"),
        NotificationsMessage::ImmediatelyText => text.literal("Un e-mail par notification, dès qu’elle se produit."),
        NotificationsMessage::Daily => text.literal("Récapitulatif quotidien"),
        NotificationsMessage::DailyText => text.literal("Un e-mail par jour avec tout ce qui s’est passé depuis le précédent."),
        NotificationsMessage::Weekly => text.literal("Récapitulatif hebdomadaire"),
        NotificationsMessage::WeeklyText => text.literal(
            "Un e-mail chaque lundi avec tout ce qui s’est passé la semaine précédente.",
        ),
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("Par défaut (");
            text.argument(hour);
            text.literal(")");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("Afficher"),
        GalleryMessage::Search => text.literal("Rechercher des thèmes"),
        GalleryMessage::BuiltIn => text.literal("Intégrés"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("Aucun thème ne correspond à votre recherche."),
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::Licence => text.literal("Licence et crédits"),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.literal("Rechercher parmi ");
            text.number(total);
            text.literal(match text.plural(total) {
                Plural::One => " thème",
                _ => " thèmes",
            });
        }
    }
    GallerySentence => RichWriter |text| {
        GallerySentence::Credit => {
            text.literal("Les thèmes Tinted sont générés à partir des schémas ");
            text.slot(0);
            text.literal(" base16 et base24 (MIT)\u{202f}; survolez un thème pour voir son auteur. ");
            text.slot(1);
            text.literal(".");
        }
    }
    ProfileSentence => RichWriter |text| {
        ProfileSentence::ProfileIntro => {
            text.literal("Affiché sur ");
            text.slot(0);
            text.literal(" et à côté de votre activité. Tout ce qui figure ici est public et facultatif.");
        }
        ProfileSentence::NotificationsIntro => {
            text.literal("Tout continue d’apparaître dans ");
            text.slot(0);
            text.literal(". Choisissez ce qui est aussi envoyé à votre adresse e-mail.");
        }
    }
}
