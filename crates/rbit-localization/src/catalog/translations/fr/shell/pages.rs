//! French (`fr`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::{Failure, PageMessage};
use crate::plural::Plural;

entries! { Fr:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(failure) => text.literal(match failure {
            Failure::NotFound => "Page introuvable",
            Failure::Forbidden => "Accès refusé",
            Failure::BadRequest => "Requête incorrecte",
            Failure::SlowDown => "Ralentissez",
            Failure::Unavailable => "Temporairement indisponible",
            Failure::Internal => "Une erreur s’est produite",
        }),
        PageMessage::NotFoundText => text.literal("La page que vous cherchez n’existe pas ou vous n’y avez pas accès."),
        PageMessage::UnavailableText => text.literal("Le service est temporairement indisponible. Réessayez dans quelques instants."),
        PageMessage::InternalText => text.literal("Une erreur inattendue s’est produite. Elle a été journalisée."),
        PageMessage::GoHome => text.literal("Retour à l’accueil"),
        PageMessage::Explore => text.literal("Explorer"),
        PageMessage::Offline => text.literal("Vous êtes hors ligne"),
        PageMessage::OfflineText => text.literal("Impossible de joindre le serveur. Vérifiez votre connexion et réessayez."),
        PageMessage::Retry => text.literal("Réessayer"),
        PageMessage::Private => text.literal("Privé"),
        PageMessage::Archived => text.literal("Archivé"),
        PageMessage::Stars(count) => {
            let count = count.unsigned_abs();
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " étoile",
                _ => " étoiles",
            });
        }
        PageMessage::Pagination => text.literal("Pagination"),
        PageMessage::Previous => text.literal("Précédent"),
        PageMessage::Next => text.literal("Suivant"),
        PageMessage::PageOf(page, pages) => {
            text.literal("Page ");
            text.number(u64::from(page));
            text.literal(" sur ");
            text.number(u64::from(pages));
        }
        PageMessage::EditProfile => text.literal("Modifier le profil"),
        PageMessage::Joined(date) => {
            text.literal("Inscrit le ");
            text.argument(date);
        }
        PageMessage::Profile => text.literal("Profil"),
        PageMessage::Repositories => text.literal("Dépôts"),
        PageMessage::StarsTab => text.literal("Étoiles"),
        PageMessage::NewRepository => text.literal("Nouveau dépôt"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(" n’a encore ajouté d’étoile à aucun dépôt.");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.argument(name);
            text.literal(" n’a encore aucun dépôt public.");
        }
    }
}
