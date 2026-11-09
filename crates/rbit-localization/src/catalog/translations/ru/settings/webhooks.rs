//! Russian (`ru`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::WebhookSentence;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};
use crate::plural::Plural;

entries! { Ru:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("Поставьте флажок для подтверждения."),
        WebhookMessage::Settings => text.literal("Настройки"),
        WebhookMessage::Webhooks => text.literal("Вебхуки"),
        WebhookMessage::AddWebhook => text.literal("Добавить вебхук"),
        WebhookMessage::NoWebhooks => text.literal("Вебхуков нет"),
        WebhookMessage::NoWebhooksText => text.literal("Добавьте вебхук, чтобы уведомлять CI-сервер, чат-бота или любой другой сервис об отправках, релизах и других событиях."),
        WebhookMessage::Inactive => text.literal("Неактивен"),
        WebhookMessage::NoDeliveries => text.literal("Доставок пока нет"),
        WebhookMessage::SendPing => text.literal("Отправить ping"),
        WebhookMessage::NewWebhook => text.literal("Новый вебхук"),
        WebhookMessage::PingLead => text.literal("Запрос"),
        WebhookMessage::PingTail => text.literal("отправляется сразу после сохранения вебхука."),
        WebhookMessage::Saved => text.literal("Вебхук сохранён."),
        WebhookMessage::PingQueued => text.literal("Ping поставлен в очередь. Он скоро появится в списке недавних доставок."),
        WebhookMessage::PayloadUrl => text.literal("URL для данных"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("Данные отправляются в формате"),
        WebhookMessage::NoRedirects => text.literal(". Перенаправления не выполняются."),
        WebhookMessage::Secret => text.literal("Секрет"),
        WebhookMessage::Optional => text.literal("необязательно"),
        WebhookMessage::SecretConfigured => text.literal("Задан — оставьте пустым, чтобы сохранить"),
        WebhookMessage::SignsLead => text.literal("Подписывает все данные: заголовки"),
        WebhookMessage::SignsAnd => text.literal("и"),
        WebhookMessage::SignsCarry => text.literal("содержат"),
        WebhookMessage::SignsTail => text.literal("и HMAC-SHA256 тела. Повторно показать секрет нельзя."),
        WebhookMessage::RemoveSecret => text.literal("Удалить секрет"),
        WebhookMessage::Events => text.literal("События"),
        WebhookMessage::Active => text.literal("Активен"),
        WebhookMessage::ActiveText => text.literal("Доставлять события на этот URL. Неактивные вебхуки сохраняют настройки и журнал."),
        WebhookMessage::SaveWebhook => text.literal("Сохранить вебхук"),
        WebhookMessage::RecentDeliveries => text.literal("Недавние доставки"),
        WebhookMessage::NoDeliveriesText => text.literal("Доставки появляются здесь по мере событий. Отправьте ping, чтобы проверить URL."),
        WebhookMessage::DangerZone => text.literal("Опасная зона"),
        WebhookMessage::DeleteTitle => text.literal("Удалить этот вебхук"),
        WebhookMessage::DeleteConfirm => text.literal("Я понимаю, что этот вебхук будет удалён"),
        WebhookMessage::DeleteWebhook => text.literal("Удалить вебхук"),
        WebhookMessage::Webhook => text.literal("Вебхук"),
        WebhookMessage::Redeliver => text.literal("Доставить повторно"),
        WebhookMessage::Request => text.literal("Запрос"),
        WebhookMessage::NotSent => text.literal("Ещё не отправлен."),
        WebhookMessage::Response => text.literal("Ответ"),
        WebhookMessage::NoResponse => text.literal("Ответ не получен."),
        WebhookMessage::EmptyBody => text.literal("Тело было пустым."),
        WebhookMessage::Retrying => text.literal("Повтор"),
        WebhookMessage::Now => text.literal("сейчас"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("В этом репозитории уже максимальное число вебхуков — ");
            text.number(max);
            text.literal(". Удалите один, чтобы добавить другой.");
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("В этой организации уже максимальное число вебхуков — ");
            text.number(max);
            text.literal(". Удалите один, чтобы добавить другой.");
        }
        WebhookText::StopsDeliveries(host) => {
            text.literal("Доставка на ");
            text.argument(host);
            text.literal(" прекратится, а журнал доставок будет удалён. Это действие нельзя отменить.");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("Повтор · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("Создан ");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "попытка",
                Plural::Few => "попытки",
                _ => "попыток",
            });
        }
        WebhookText::Attempts(count) => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "попытка",
                Plural::Few => "попытки",
                _ => "попыток",
            });
        }
        WebhookText::NextAttempt(when) => {
            text.literal("Следующая попытка ");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("Вебхуки · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("Новый вебхук · ");
            text.argument(name);
        }
        WebhookText::EditTitle(name) => {
            text.literal("Вебхук · ");
            text.argument(name);
        }
        WebhookText::DeliveryTitle(guid, name) => {
            text.literal("Доставка ");
            text.argument(guid);
            text.literal(" · ");
            text.argument(name);
        }
    }
    WebhookSentence => RichWriter |text| {
        WebhookSentence::Intro { organization } => {
            text.slot(0);
            text.literal(" отправляет подписанный ");
            text.slot(1);
            text.literal(" запрос с данными JSON на каждый URL, когда происходят выбранные для него события в ");
            text.slot(2);
            if organization {
                text.literal(" или любом из её репозиториев");
            }
            text.literal(".");
        }
    }
}
