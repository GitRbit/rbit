//! French (`fr`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};
use crate::plural::Plural;

entries! { Fr:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("Cochez la case pour confirmer."),
        WebhookMessage::Settings => text.literal("Paramètres"),
        WebhookMessage::Webhooks => text.literal("Webhooks"),
        WebhookMessage::IntroSigned => text.literal("envoie une requête"),
        WebhookMessage::IntroPayload => text.literal(
            "signée, avec une charge utile JSON, à chaque URL lorsque les événements auxquels elle est abonnée se \
             produisent dans",
        ),
        WebhookMessage::IntroOrgTail => text.literal("ou l’un de ses dépôts"),
        WebhookMessage::IntroTail => text.literal("."),
        WebhookMessage::AddWebhook => text.literal("Ajouter un webhook"),
        WebhookMessage::NoWebhooks => text.literal("Aucun webhook"),
        WebhookMessage::NoWebhooksText => text.literal(
            "Ajoutez un webhook pour prévenir un serveur de CI, un bot de discussion ou tout autre service des push, \
             des releases et plus encore.",
        ),
        WebhookMessage::Inactive => text.literal("Inactif"),
        WebhookMessage::NoDeliveries => text.literal("Aucune livraison pour l’instant"),
        WebhookMessage::SendPing => text.literal("Envoyer un ping"),
        WebhookMessage::NewWebhook => text.literal("Nouveau webhook"),
        WebhookMessage::PingLead => text.literal("Un"),
        WebhookMessage::PingTail => text.literal("est envoyé dès l’enregistrement du webhook."),
        WebhookMessage::Saved => text.literal("Webhook enregistré."),
        WebhookMessage::PingQueued => text.literal(
            "Ping en file d’attente. Il apparaîtra dans un instant parmi les livraisons récentes.",
        ),
        WebhookMessage::PayloadUrl => text.literal("URL de la charge utile"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("Les charges utiles sont envoyées en"),
        WebhookMessage::NoRedirects => text.literal(". Les redirections ne sont pas suivies."),
        WebhookMessage::Secret => text.literal("Secret"),
        WebhookMessage::Optional => text.literal("facultatif"),
        WebhookMessage::SecretConfigured => text.literal("Configuré — laissez vide pour le conserver"),
        WebhookMessage::SignsLead => text.literal("Signe chaque charge utile\u{a0}:"),
        WebhookMessage::SignsAnd => text.literal("et"),
        WebhookMessage::SignsCarry => text.literal("contiennent"),
        WebhookMessage::SignsTail => text.literal(
            "suivi du HMAC-SHA256 du corps. Il ne pourra plus être affiché.",
        ),
        WebhookMessage::RemoveSecret => text.literal("Retirer le secret"),
        WebhookMessage::Events => text.literal("Événements"),
        WebhookMessage::Active => text.literal("Actif"),
        WebhookMessage::ActiveText => text.literal(
            "Livrer les événements à cette URL. Les webhooks inactifs conservent leurs paramètres et leur journal.",
        ),
        WebhookMessage::SaveWebhook => text.literal("Enregistrer le webhook"),
        WebhookMessage::RecentDeliveries => text.literal("Livraisons récentes"),
        WebhookMessage::NoDeliveriesText => text.literal(
            "Les livraisons apparaissent ici au fil des événements. Envoyez un ping pour tester l’URL.",
        ),
        WebhookMessage::DangerZone => text.literal("Zone de danger"),
        WebhookMessage::DeleteTitle => text.literal("Supprimer ce webhook"),
        WebhookMessage::DeleteConfirm => text.literal("Je comprends que ce webhook sera supprimé"),
        WebhookMessage::DeleteWebhook => text.literal("Supprimer le webhook"),
        WebhookMessage::Webhook => text.literal("Webhook"),
        WebhookMessage::Redeliver => text.literal("Relivrer"),
        WebhookMessage::Request => text.literal("Requête"),
        WebhookMessage::NotSent => text.literal("Pas encore envoyée."),
        WebhookMessage::Response => text.literal("Réponse"),
        WebhookMessage::NoResponse => text.literal("Aucune réponse n’a été reçue."),
        WebhookMessage::EmptyBody => text.literal("Le corps était vide."),
        WebhookMessage::Retrying => text.literal("Nouvelle tentative"),
        WebhookMessage::Now => text.literal("maintenant"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("Ce dépôt a atteint le maximum de ");
            text.number(max);
            webhooks_tail(text, max);
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("Cette organisation a atteint le maximum de ");
            text.number(max);
            webhooks_tail(text, max);
        }
        WebhookText::StopsDeliveries(host) => {
            text.literal("Arrête les livraisons vers ");
            text.argument(host);
            text.literal(" et supprime le journal des livraisons. Cette action est irréversible.");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("Nouvelle tentative · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("Créé ");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) => {
            text.number(count);
            text.literal(" tentative");
        }
        WebhookText::Attempts(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " tentative",
                _ => " tentatives",
            });
        }
        WebhookText::NextAttempt(when) => {
            text.literal("Prochaine tentative ");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("Webhooks · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("Nouveau webhook · ");
            text.argument(name);
        }
        WebhookText::EditTitle(name) => {
            text.literal("Webhook · ");
            text.argument(name);
        }
        WebhookText::DeliveryTitle(guid, name) => {
            text.literal("Livraison ");
            text.argument(guid);
            text.literal(" · ");
            text.argument(name);
        }
    }
}

/// « webhooks. Supprimez-en un pour en ajouter un autre. »
fn webhooks_tail(text: &mut MessageWriter, max: u64) {
    text.literal(match text.plural(max) {
        Plural::One => " webhook. Supprimez-le pour en ajouter un autre.",
        _ => " webhooks. Supprimez-en un pour en ajouter un autre.",
    });
}
