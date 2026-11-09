//! German (`de`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::{Failure, PageMessage};
use crate::plural::Plural;

entries! { De:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(failure) => text.literal(match failure {
            Failure::NotFound => "Seite nicht gefunden",
            Failure::Forbidden => "Zugriff verweigert",
            Failure::BadRequest => "Ungültige Anfrage",
            Failure::SlowDown => "Bitte langsamer",
            Failure::Unavailable => "Vorübergehend nicht verfügbar",
            Failure::Internal => "Etwas ist schiefgelaufen",
        }),
        PageMessage::NotFoundText => text.literal(
            "Die gesuchte Seite existiert nicht, oder Sie haben keinen Zugriff darauf.",
        ),
        PageMessage::UnavailableText => text.literal(
            "Der Dienst ist vorübergehend nicht verfügbar. Versuchen Sie es in Kürze erneut.",
        ),
        PageMessage::InternalText => text.literal(
            "Ein unerwarteter Fehler ist aufgetreten. Er wurde protokolliert.",
        ),
        PageMessage::GoHome => text.literal("Zur Startseite"),
        PageMessage::Explore => text.literal("Entdecken"),
        PageMessage::Offline => text.literal("Sie sind offline"),
        PageMessage::OfflineText => text.literal(
            "Der Server war nicht erreichbar. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
        ),
        PageMessage::Retry => text.literal("Erneut versuchen"),
        PageMessage::Private => text.literal("Privat"),
        PageMessage::Archived => text.literal("Archiviert"),
        PageMessage::Stars(count) => {
            text.number(count.unsigned_abs());
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " Stern",
                _ => " Sterne",
            });
        }
        PageMessage::Pagination => text.literal("Seitennavigation"),
        PageMessage::Previous => text.literal("Zurück"),
        PageMessage::Next => text.literal("Weiter"),
        PageMessage::PageOf(page, pages) => {
            text.literal("Seite ");
            text.number(u64::from(page));
            text.literal(" von ");
            text.number(u64::from(pages));
        }
        PageMessage::EditProfile => text.literal("Profil bearbeiten"),
        PageMessage::Joined(date) => {
            text.literal("Mitglied seit ");
            text.argument(date);
        }
        PageMessage::Profile => text.literal("Profil"),
        PageMessage::Repositories => text.literal("Repositorys"),
        PageMessage::StarsTab => text.literal("Sterne"),
        PageMessage::NewRepository => text.literal("Neues Repository"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(" hat noch keine Repositorys mit einem Stern markiert.");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.argument(name);
            text.literal(" hat noch keine öffentlichen Repositorys.");
        }
    }
}
