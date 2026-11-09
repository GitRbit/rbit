//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::WebhookSentence;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};

entries! { PtBr:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("Marque a caixa para confirmar."),
        WebhookMessage::Settings => text.literal("Configurações"),
        WebhookMessage::Webhooks => text.literal("Webhooks"),
        WebhookMessage::AddWebhook => text.literal("Adicionar webhook"),
        WebhookMessage::NoWebhooks => text.literal("Nenhum webhook"),
        WebhookMessage::NoWebhooksText => {
            text.literal(
                "Adicione um webhook para notificar um servidor de CI, um bot de chat ou qualquer outro serviço sobre \
                 pushes, releases e mais.",
            );
        }
        WebhookMessage::Inactive => text.literal("Inativo"),
        WebhookMessage::NoDeliveries => text.literal("Ainda não há entregas"),
        WebhookMessage::SendPing => text.literal("Enviar ping"),
        WebhookMessage::NewWebhook => text.literal("Novo webhook"),
        WebhookMessage::PingLead => text.literal("Um"),
        WebhookMessage::PingTail => text.literal("é enviado assim que o webhook é salvo."),
        WebhookMessage::Saved => text.literal("Webhook salvo."),
        WebhookMessage::PingQueued => {
            text.literal("Ping na fila. Ele aparecerá nas entregas recentes em instantes.");
        }
        WebhookMessage::PayloadUrl => text.literal("URL do payload"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("Os payloads são"),
        WebhookMessage::NoRedirects => text.literal(". Redirecionamentos não são seguidos."),
        WebhookMessage::Secret => text.literal("Segredo"),
        WebhookMessage::Optional => text.literal("opcional"),
        WebhookMessage::SecretConfigured => text.literal("Configurado — deixe em branco para manter"),
        WebhookMessage::SignsLead => text.literal("Assina cada payload:"),
        WebhookMessage::SignsAnd => text.literal("e"),
        WebhookMessage::SignsCarry => text.literal("contêm"),
        WebhookMessage::SignsTail => {
            text.literal("e o HMAC-SHA256 do corpo. O segredo não pode ser exibido novamente.");
        }
        WebhookMessage::RemoveSecret => text.literal("Remover o segredo"),
        WebhookMessage::Events => text.literal("Eventos"),
        WebhookMessage::Active => text.literal("Ativo"),
        WebhookMessage::ActiveText => {
            text.literal(
                "Entregar eventos para esta URL. Webhooks inativos mantêm as configurações e o log.",
            );
        }
        WebhookMessage::SaveWebhook => text.literal("Salvar webhook"),
        WebhookMessage::RecentDeliveries => text.literal("Entregas recentes"),
        WebhookMessage::NoDeliveriesText => {
            text.literal("As entregas aparecem aqui conforme os eventos acontecem. Envie um ping para testar a URL.");
        }
        WebhookMessage::DangerZone => text.literal("Zona de perigo"),
        WebhookMessage::DeleteTitle => text.literal("Excluir este webhook"),
        WebhookMessage::DeleteConfirm => text.literal("Entendo que este webhook será excluído"),
        WebhookMessage::DeleteWebhook => text.literal("Excluir webhook"),
        WebhookMessage::Webhook => text.literal("Webhook"),
        WebhookMessage::Redeliver => text.literal("Entregar novamente"),
        WebhookMessage::Request => text.literal("Requisição"),
        WebhookMessage::NotSent => text.literal("Ainda não enviado."),
        WebhookMessage::Response => text.literal("Resposta"),
        WebhookMessage::NoResponse => text.literal("Nenhuma resposta foi recebida."),
        WebhookMessage::EmptyBody => text.literal("O corpo estava vazio."),
        WebhookMessage::Retrying => text.literal("Tentando novamente"),
        WebhookMessage::Now => text.literal("agora"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("Este repositório já tem o máximo de ");
            text.number(max);
            text.literal(" webhooks. Exclua um para adicionar outro.");
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("Esta organização já tem o máximo de ");
            text.number(max);
            text.literal(" webhooks. Exclua um para adicionar outro.");
        }
        WebhookText::StopsDeliveries(host) => {
            text.literal("Interrompe as entregas para ");
            text.argument(host);
            text.literal(" e remove o log de entregas. Isso não pode ser desfeito.");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("Tentando novamente · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("Criado ");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) => {
            text.number(count);
            text.literal(" tentativa");
        }
        WebhookText::Attempts(count) => {
            text.number(count);
            text.literal(" tentativas");
        }
        WebhookText::NextAttempt(when) => {
            text.literal("Próxima tentativa ");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("Webhooks · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("Novo webhook · ");
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
            text.literal(" envia um ");
            text.slot(1);
            text.literal(" assinado com um payload JSON para cada URL quando os eventos selecionados nela acontecem em ");
            text.slot(2);
            if organization {
                text.literal(" ou em qualquer um dos repositórios dela");
            }
            text.literal(".");
        }
    }
}
