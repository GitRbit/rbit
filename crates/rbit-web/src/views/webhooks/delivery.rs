//! `…/hooks/{id}/deliveries/{delivery}`: what was sent and what came back.
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Timestamp,
    webhook::{Delivery, DeliveryStatus, Webhook},
};
use rbit_localization::{
    Localized,
    settings::webhooks::{WebhookMessage, WebhookText},
};

use super::{HookFrame, StatusBadge, event_name};
use crate::views::layout::Page;

#[derive(Debug, Template, WebTemplate)]
#[template(path = "webhooks/delivery.html")]
pub struct DeliveryPage {
    pub page: Page,
    pub frame: HookFrame,
    pub hook_url: String,
    pub redeliver_url: String,
    pub guid: String,
    pub event: String,
    pub status: StatusBadge,
    pub attempts: u32,
    pub created: String,
    pub created_iso: String,
    pub next_attempt: Option<String>,
    pub duration: String,
    pub request_headers: Vec<(String, String)>,
    /// The payload, pretty-printed.
    pub payload: String,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    pub error: Option<String>,
}

impl DeliveryPage {
    pub fn new(page: Page, frame: HookFrame, hook: &Webhook, delivery: &Delivery) -> Self {
        let locale = page.locale;
        let hook_url = frame.hook_url(hook);
        let payload = serde_json::from_str::<serde_json::Value>(&delivery.payload)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .unwrap_or_else(|_| delivery.payload.clone());
        let next_attempt = (delivery.status == DeliveryStatus::Pending)
            .then_some(delivery.next_attempt_at)
            .flatten()
            .map(|at| if at <= Timestamp::now() { WebhookMessage::Now.text(&locale).to_string() } else { at.iso8601() });
        let title = WebhookText::DeliveryTitle(short_guid(&delivery.guid), &frame.name).text(&locale);
        Self {
            page: page.titled(title.as_str()),
            redeliver_url: format!("{hook_url}/deliveries/{}/redeliver", delivery.id.0),
            hook_url,
            guid: delivery.guid.clone(),
            event: event_name(delivery),
            status: StatusBadge::for_delivery(delivery, locale),
            attempts: delivery.attempts,
            created: delivery.created_at.relative(),
            created_iso: delivery.created_at.iso8601(),
            next_attempt,
            duration: delivery.duration_ms.map(|ms| format!("{ms} ms")).unwrap_or_default(),
            request_headers: delivery.request_headers.clone(),
            payload,
            response_status: delivery.response_status,
            response_headers: delivery.response_headers.clone(),
            response_body: delivery.response_body.clone().filter(|body| !body.is_empty()),
            error: delivery.error.clone(),
            frame,
        }
    }

    /// `3 attempts`, in the page's language.
    pub fn attempts_text(&self) -> Localized {
        let count = u64::from(self.attempts);
        let text = if count == 1 { WebhookText::OneAttempt(count) } else { WebhookText::Attempts(count) };
        text.text(&self.page.locale)
    }
}

fn short_guid(guid: &str) -> &str {
    guid.get(..8).unwrap_or(guid)
}
