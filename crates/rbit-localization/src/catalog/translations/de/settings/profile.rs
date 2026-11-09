//! German (`de`) entries for `catalog/settings/profile.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};

entries! { De:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("Profil"),
        ProfileMessage::IntroLead => text.literal("Wird auf"),
        ProfileMessage::IntroLink => text.literal("Ihrer Profilseite"),
        ProfileMessage::IntroTail => text.literal(
            "und neben Ihren Aktivitäten angezeigt. Alle Angaben hier sind öffentlich und freiwillig.",
        ),
        ProfileMessage::Updated => text.literal("Profil aktualisiert."),
        ProfileMessage::DisplayName => text.literal("Anzeigename"),
        ProfileMessage::DisplayNameHint => text.literal(
            "Wird anstelle Ihres Benutzernamens angezeigt, wo Platz ist.",
        ),
        ProfileMessage::Bio => text.literal("Über mich"),
        ProfileMessage::BioHint => text.literal("Bis zu 300 Zeichen."),
        ProfileMessage::Website => text.literal("Website"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("Standort"),
        ProfileMessage::Save => text.literal("Profil speichern"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("Darstellung"),
        AppearanceMessage::Intro => text.literal(
            "Ihr Design wird in Ihrem Konto gespeichert und begleitet Sie auf jedes Gerät, auf dem Sie sich anmelden. Fahren Sie mit der Maus über ein Design, um es in der Vorschau zu sehen; klicken Sie, um es zu übernehmen.",
        ),
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("Benachrichtigungen"),
        NotificationsMessage::IntroLead => text.literal("Alles erscheint weiterhin in"),
        NotificationsMessage::IntroLink => text.literal("Ihrem Posteingang"),
        NotificationsMessage::IntroTail => text.literal(
            ". Wählen Sie, was zusätzlich an Ihre E-Mail-Adresse gesendet wird.",
        ),
        NotificationsMessage::Saved => text.literal("Einstellungen gespeichert."),
        NotificationsMessage::Unavailable => text.literal(
            "E-Mail ist auf diesem Server nicht verfügbar, daher erscheinen Benachrichtigungen nur in Ihrem Posteingang.",
        ),
        NotificationsMessage::VerifyLead => text.literal("Bestätigen Sie"),
        NotificationsMessage::VerifyMiddle => text.literal("in den"),
        NotificationsMessage::VerifyLink => text.literal("Kontoeinstellungen"),
        NotificationsMessage::VerifyTail => text.literal(
            "um Benachrichtigungs-E-Mails zu erhalten.",
        ),
        NotificationsMessage::Email => text.literal("E-Mail"),
        NotificationsMessage::SendTo => text.literal("Senden an"),
        NotificationsMessage::When => text.literal("Wann"),
        NotificationsMessage::DigestTime => text.literal("Uhrzeit der Zusammenfassung"),
        NotificationsMessage::DigestHint => text.literal(
            "Zusammenfassungen lassen Threads weg, die Sie hier bereits gelesen haben. Jede E-Mail enthält einen Link, mit dem Sie sie mit einem Klick abbestellen.",
        ),
        NotificationsMessage::Save => text.literal("Einstellungen speichern"),
        NotificationsMessage::Off => text.literal("Aus"),
        NotificationsMessage::OffText => text.literal(
            "Benachrichtigungen erscheinen nur in Ihrem Posteingang.",
        ),
        NotificationsMessage::Participating => text.literal("Beteiligt"),
        NotificationsMessage::ParticipatingText => text.literal(
            "Threads, die Sie eröffnet, kommentiert oder geprüft haben, und wenn Sie erwähnt, zugewiesen oder um ein Review gebeten werden.",
        ),
        NotificationsMessage::Everything => text.literal("Alles"),
        NotificationsMessage::EverythingText => text.literal(
            "Auch Threads, die Sie abonniert haben, ohne sich zu beteiligen.",
        ),
        NotificationsMessage::Immediately => text.literal("Sofort"),
        NotificationsMessage::ImmediatelyText => text.literal(
            "Eine E-Mail pro Benachrichtigung, sobald sie eintritt.",
        ),
        NotificationsMessage::Daily => text.literal("Tägliche Zusammenfassung"),
        NotificationsMessage::DailyText => text.literal(
            "Eine E-Mail pro Tag mit allem seit der letzten.",
        ),
        NotificationsMessage::Weekly => text.literal("Wöchentliche Zusammenfassung"),
        NotificationsMessage::WeeklyText => text.literal(
            "Eine E-Mail jeden Montag mit allem aus der Vorwoche.",
        ),
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("Standard (");
            text.argument(hour);
            text.literal(")");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("Anzeigen"),
        GalleryMessage::Search => text.literal("Designs durchsuchen"),
        GalleryMessage::BuiltIn => text.literal("Integriert"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("Keine Designs entsprechen Ihrer Suche."),
        GalleryMessage::CreditLead => text.literal("Getönte Designs werden aus den base16- und base24-Schemata (MIT) von"),
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::CreditTail => text.literal(
            "erzeugt; fahren Sie mit der Maus über ein Design, um seinen Autor zu sehen.",
        ),
        GalleryMessage::Licence => text.literal("Lizenz und Danksagungen"),
        GalleryMessage::LicenceTail => text.literal("."),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.number(total);
            text.literal(" Designs durchsuchen");
        }
    }
}
