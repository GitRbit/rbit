//! German (`de`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};
use crate::plural::Plural;

entries! { De:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("Setzen Sie zur Bestätigung das Häkchen."),
        WebhookMessage::Settings => text.literal("Einstellungen"),
        WebhookMessage::Webhooks => text.literal("Webhooks"),
        WebhookMessage::IntroSigned => text.literal("sendet einen signierten"),
        WebhookMessage::IntroPayload => text.literal(
            "mit einer JSON-Payload an jede URL, sobald abonnierte Ereignisse eintreten, und zwar in",
        ),
        WebhookMessage::IntroOrgTail => text.literal("oder einem ihrer Repositorys"),
        WebhookMessage::IntroTail => text.literal("."),
        WebhookMessage::AddWebhook => text.literal("Webhook hinzufügen"),
        WebhookMessage::NoWebhooks => text.literal("Keine Webhooks"),
        WebhookMessage::NoWebhooksText => text.literal(
            "Fügen Sie einen Webhook hinzu, um einen CI-Server, einen Chatbot oder einen anderen Dienst über Pushes, Releases und mehr zu benachrichtigen.",
        ),
        WebhookMessage::Inactive => text.literal("Inaktiv"),
        WebhookMessage::NoDeliveries => text.literal("Noch keine Zustellungen"),
        WebhookMessage::SendPing => text.literal("Ping senden"),
        WebhookMessage::NewWebhook => text.literal("Neuer Webhook"),
        WebhookMessage::PingLead => text.literal("Ein"),
        WebhookMessage::PingTail => text.literal("wird gesendet, sobald der Webhook gespeichert ist."),
        WebhookMessage::Saved => text.literal("Webhook gespeichert."),
        WebhookMessage::PingQueued => text.literal(
            "Ping eingereiht. Er erscheint gleich unter den letzten Zustellungen.",
        ),
        WebhookMessage::PayloadUrl => text.literal("Payload-URL"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("Payloads sind"),
        WebhookMessage::NoRedirects => text.literal(". Weiterleitungen werden nicht verfolgt."),
        WebhookMessage::Secret => text.literal("Secret"),
        WebhookMessage::Optional => text.literal("optional"),
        WebhookMessage::SecretConfigured => text.literal(
            "Konfiguriert – leer lassen, um es beizubehalten",
        ),
        WebhookMessage::SignsLead => text.literal("Signiert jede Payload:"),
        WebhookMessage::SignsAnd => text.literal("und"),
        WebhookMessage::SignsCarry => text.literal("enthalten"),
        WebhookMessage::SignsTail => text.literal(
            "und den HMAC-SHA256 des Bodys. Das Secret kann nicht erneut angezeigt werden.",
        ),
        WebhookMessage::RemoveSecret => text.literal("Secret entfernen"),
        WebhookMessage::Events => text.literal("Ereignisse"),
        WebhookMessage::Active => text.literal("Aktiv"),
        WebhookMessage::ActiveText => text.literal(
            "Ereignisse an diese URL zustellen. Inaktive Webhooks behalten ihre Einstellungen und ihr Protokoll.",
        ),
        WebhookMessage::SaveWebhook => text.literal("Webhook speichern"),
        WebhookMessage::RecentDeliveries => text.literal("Letzte Zustellungen"),
        WebhookMessage::NoDeliveriesText => text.literal(
            "Zustellungen erscheinen hier, sobald Ereignisse eintreten. Senden Sie einen Ping, um die URL zu testen.",
        ),
        WebhookMessage::DangerZone => text.literal("Gefahrenbereich"),
        WebhookMessage::DeleteTitle => text.literal("Diesen Webhook löschen"),
        WebhookMessage::DeleteConfirm => text.literal(
            "Mir ist bewusst, dass dieser Webhook gelöscht wird",
        ),
        WebhookMessage::DeleteWebhook => text.literal("Webhook löschen"),
        WebhookMessage::Webhook => text.literal("Webhook"),
        WebhookMessage::Redeliver => text.literal("Erneut zustellen"),
        WebhookMessage::Request => text.literal("Anfrage"),
        WebhookMessage::NotSent => text.literal("Noch nicht gesendet."),
        WebhookMessage::Response => text.literal("Antwort"),
        WebhookMessage::NoResponse => text.literal("Es wurde keine Antwort empfangen."),
        WebhookMessage::EmptyBody => text.literal("Der Body war leer."),
        WebhookMessage::Retrying => text.literal("Wird wiederholt"),
        WebhookMessage::Now => text.literal("jetzt"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("Dieses Repository hat die Höchstzahl von ");
            text.number(max);
            text.literal(" Webhooks erreicht. Löschen Sie einen, um einen weiteren hinzuzufügen.");
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("Diese Organisation hat die Höchstzahl von ");
            text.number(max);
            text.literal(" Webhooks erreicht. Löschen Sie einen, um einen weiteren hinzuzufügen.");
        }
        WebhookText::StopsDeliveries(host) => {
            text.literal("Beendet die Zustellungen an ");
            text.argument(host);
            text.literal(" und entfernt das Zustellungsprotokoll. Dies kann nicht rückgängig gemacht werden.");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("Wird wiederholt · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("Erstellt ");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) => {
            text.number(count);
            text.literal(" Versuch");
        }
        WebhookText::Attempts(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Versuch",
                _ => " Versuche",
            });
        }
        WebhookText::NextAttempt(when) => {
            text.literal("Nächster Versuch ");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("Webhooks · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("Neuer Webhook · ");
            text.argument(name);
        }
        WebhookText::EditTitle(name) => {
            text.literal("Webhook · ");
            text.argument(name);
        }
        WebhookText::DeliveryTitle(guid, name) => {
            text.literal("Zustellung ");
            text.argument(guid);
            text.literal(" · ");
            text.argument(name);
        }
    }
}
