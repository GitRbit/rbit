//! Spanish (`es`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::WebhookSentence;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};
use crate::plural::Plural;

entries! { Es:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("Marca la casilla para confirmar."),
        WebhookMessage::Settings => text.literal("Configuración"),
        WebhookMessage::Webhooks => text.literal("Webhooks"),
        WebhookMessage::AddWebhook => text.literal("Añadir webhook"),
        WebhookMessage::NoWebhooks => text.literal("No hay webhooks"),
        WebhookMessage::NoWebhooksText => text.literal(
            "Añade un webhook para avisar a un servidor de CI, a un bot de chat o a cualquier otro servicio de los \
             envíos, las versiones y mucho más.",
        ),
        WebhookMessage::Inactive => text.literal("Inactivo"),
        WebhookMessage::NoDeliveries => text.literal("Aún no hay entregas"),
        WebhookMessage::SendPing => text.literal("Enviar ping"),
        WebhookMessage::NewWebhook => text.literal("Nuevo webhook"),
        WebhookMessage::PingLead => text.literal("Se envía un"),
        WebhookMessage::PingTail => text.literal("en cuanto se guarda el webhook."),
        WebhookMessage::Saved => text.literal("Webhook guardado."),
        WebhookMessage::PingQueued => text.literal(
            "Ping en cola. Aparecerá en un momento en las entregas recientes.",
        ),
        WebhookMessage::PayloadUrl => text.literal("URL de la carga"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("Las cargas son"),
        WebhookMessage::NoRedirects => text.literal(". No se siguen las redirecciones."),
        WebhookMessage::Secret => text.literal("Secreto"),
        WebhookMessage::Optional => text.literal("opcional"),
        WebhookMessage::SecretConfigured => text.literal("Configurado: déjalo vacío para conservarlo"),
        WebhookMessage::SignsLead => text.literal("Firma cada carga:"),
        WebhookMessage::SignsAnd => text.literal("y"),
        WebhookMessage::SignsCarry => text.literal("llevan"),
        WebhookMessage::SignsTail => text.literal(
            "y el HMAC-SHA256 del cuerpo. No se puede volver a mostrar.",
        ),
        WebhookMessage::RemoveSecret => text.literal("Quitar el secreto"),
        WebhookMessage::Events => text.literal("Eventos"),
        WebhookMessage::Active => text.literal("Activo"),
        WebhookMessage::ActiveText => text.literal(
            "Entregar los eventos a esta URL. Los webhooks inactivos conservan su configuración y su registro.",
        ),
        WebhookMessage::SaveWebhook => text.literal("Guardar webhook"),
        WebhookMessage::RecentDeliveries => text.literal("Entregas recientes"),
        WebhookMessage::NoDeliveriesText => text.literal(
            "Las entregas aparecen aquí a medida que ocurren los eventos. Envía un ping para probar la URL.",
        ),
        WebhookMessage::DangerZone => text.literal("Zona de peligro"),
        WebhookMessage::DeleteTitle => text.literal("Eliminar este webhook"),
        WebhookMessage::DeleteConfirm => text.literal("Entiendo que este webhook se eliminará"),
        WebhookMessage::DeleteWebhook => text.literal("Eliminar webhook"),
        WebhookMessage::Webhook => text.literal("Webhook"),
        WebhookMessage::Redeliver => text.literal("Volver a entregar"),
        WebhookMessage::Request => text.literal("Solicitud"),
        WebhookMessage::NotSent => text.literal("Aún no se ha enviado."),
        WebhookMessage::Response => text.literal("Respuesta"),
        WebhookMessage::NoResponse => text.literal("No se recibió ninguna respuesta."),
        WebhookMessage::EmptyBody => text.literal("El cuerpo estaba vacío."),
        WebhookMessage::Retrying => text.literal("Reintentando"),
        WebhookMessage::Now => text.literal("ahora"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("Este repositorio tiene el máximo de ");
            text.number(max);
            text.literal(" webhooks. Elimina uno para añadir otro.");
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("Esta organización tiene el máximo de ");
            text.number(max);
            text.literal(" webhooks. Elimina uno para añadir otro.");
        }
        WebhookText::StopsDeliveries(host) => {
            text.literal("Detiene las entregas a ");
            text.argument(host);
            text.literal(" y elimina el registro de entregas. No se puede deshacer.");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("Reintentando · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("Creado ");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) => {
            text.number(count);
            text.literal(" intento");
        }
        WebhookText::Attempts(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " intento",
                _ => " intentos",
            });
        }
        WebhookText::NextAttempt(when) => {
            text.literal("Próximo intento ");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("Webhooks · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("Nuevo webhook · ");
            text.argument(name);
        }
        WebhookText::EditTitle(name) => {
            text.literal("Webhook · ");
            text.argument(name);
        }
        WebhookText::DeliveryTitle(guid, name) => {
            text.literal("Entrega ");
            text.argument(guid);
            text.literal(" · ");
            text.argument(name);
        }
    }
    WebhookSentence => RichWriter |text| {
        WebhookSentence::Intro { organization } => {
            text.slot(0);
            text.literal(" envía una solicitud ");
            text.slot(1);
            text.literal(" firmada con una carga JSON a cada URL cuando ocurren los eventos a los que está suscrita en ");
            text.slot(2);
            if organization {
                text.literal(" o en cualquiera de sus repositorios");
            }
            text.literal(".");
        }
    }
}
