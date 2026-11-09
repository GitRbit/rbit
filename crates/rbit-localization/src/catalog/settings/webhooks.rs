//! Repository and organization webhooks: the list, the form and the delivery log.

labels! {
    /// Fixed labels of the webhook pages.
    WebhookMessage, "webhooks" {
        TickToConfirm = "tick-to-confirm" => "Tick the box to confirm.",
        Settings = "settings" => "Settings",
        Webhooks = "title" => "Webhooks",
        AddWebhook = "add" => "Add webhook",
        NoWebhooks = "empty-title" => "No webhooks",
        NoWebhooksText = "empty-text" => "Add a webhook to notify a CI server, a chat bot or any other service about pushes, releases and more.",
        Inactive = "inactive" => "Inactive",
        NoDeliveries = "no-deliveries" => "No deliveries yet",
        SendPing = "send-ping" => "Send ping",
        NewWebhook = "new" => "New webhook",
        PingLead = "ping-lead" => "A",
        PingTail = "ping-tail" => "is sent as soon as the webhook is saved.",
        Saved = "saved" => "Webhook saved.",
        PingQueued = "ping-queued" => "Ping queued. It appears under recent deliveries in a moment.",
        PayloadUrl = "payload-url" => "Payload URL",
        UrlPlaceholder = "url-placeholder" => "https://example.com/hooks/rbit",
        PayloadsAre = "payloads-are" => "Payloads are",
        NoRedirects = "no-redirects" => ". Redirects are not followed.",
        Secret = "secret" => "Secret",
        Optional = "optional" => "optional",
        SecretConfigured = "secret-configured" => "Configured — leave blank to keep it",
        SignsLead = "signs-lead" => "Signs every payload:",
        SignsAnd = "signs-and" => "and",
        SignsCarry = "signs-carry" => "carry",
        SignsTail = "signs-tail" => "and the HMAC-SHA256 of the body. It cannot be shown again.",
        RemoveSecret = "remove-secret" => "Remove the secret",
        Events = "events" => "Events",
        Active = "active" => "Active",
        ActiveText = "active-text" => "Deliver events to this URL. Inactive webhooks keep their settings and log.",
        SaveWebhook = "save" => "Save webhook",
        RecentDeliveries = "recent-deliveries" => "Recent deliveries",
        NoDeliveriesText = "no-deliveries-text" => "Deliveries appear here as events happen. Send a ping to try the URL.",
        DangerZone = "danger-zone" => "Danger zone",
        DeleteTitle = "delete-title" => "Delete this webhook",
        DeleteConfirm = "delete-confirm" => "I understand that this webhook will be deleted",
        DeleteWebhook = "delete" => "Delete webhook",
        Webhook = "webhook" => "Webhook",
        Redeliver = "redeliver" => "Redeliver",
        Request = "request" => "Request",
        NotSent = "not-sent" => "Not sent yet.",
        Response = "response" => "Response",
        NoResponse = "no-response" => "No response was received.",
        EmptyBody = "empty-body" => "The body was empty.",
        Retrying = "retrying" => "Retrying",
        Now = "now" => "now",
    }
}

messages! {
    /// Webhook text with arguments.
    WebhookText<'a>, "webhooks" {
        MaxRepository(max: u64) = "max-repository" => ["This repository has the maximum of ", (max), " webhooks. Delete one to add another."],
        MaxOrganization(max: u64) = "max-organization" => ["This organization has the maximum of ", (max), " webhooks. Delete one to add another."],
        StopsDeliveries(host: &'a str) = "delete-text" => ["Stops deliveries to ", {host}, " and removes the delivery log. It cannot be undone."],
        RetryingCode(code: u64) = "retrying-code" => ["Retrying · ", (code)],
        Created(when: &'a str) = "created" => ["Created ", {when}],
        OneAttempt(count: u64) = "one-attempt" => [(count), " attempt"],
        Attempts(count: u64) = "attempts" => [(count), " attempts"],
        NextAttempt(when: &'a str) = "next-attempt" => ["Next attempt ", {when}],
        ListTitle(name: &'a str) = "list-title" => ["Webhooks · ", {name}],
        NewTitle(name: &'a str) = "new-title" => ["New webhook · ", {name}],
        EditTitle(name: &'a str) = "edit-title" => ["Webhook · ", {name}],
        DeliveryTitle(guid: &'a str, name: &'a str) = "delivery-title" => ["Delivery ", {guid}, " · ", {name}],
    }
}

/// Webhook page sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookSentence {
    /// Under the webhooks title; `organization` on an organization's list. Slot 0: the site name; 1: the HTTP method; 2: the repository or organization.
    Intro { organization: bool },
}

impl WebhookSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<crate::Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for WebhookSentence {
    fn id(self) -> &'static str {
        match self {
            Self::Intro { .. } => "webhooks.intro",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::Intro { organization } => {
                text.slot(0);
                text.literal(" sends a signed ");
                text.slot(1);
                text.literal(" with a JSON payload to each URL when the events it subscribes to happen in ");
                text.slot(2);
                if organization {
                    text.literal(" or any of its repositories");
                }
                text.literal(".");
            }
        }
    }

    fn translate(self, locale: crate::Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
