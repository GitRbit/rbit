//! Webhook settings pages and the delivery log.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.
//!
//! The same pages serve repository hooks (`/{owner}/{repo}/settings/hooks`, inside the
//! repository chrome) and organisation hooks (`/{org}/settings/hooks`, inside the
//! organisation chrome); [`HookFrame`] carries whichever header applies.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Timestamp,
    webhook::{Delivery, DeliveryStatus, LastDelivery, Webhook, WebhookEvent},
};
use rbit_services::webhooks::MAX_HOOKS_PER_OWNER;

use crate::views::{
    components::{FormError, Pagination, SettingsCard},
    layout::Page,
    org::OrgHeader,
    repo::RepoHeader,
};

/// Cards this workstream adds to the repository settings page.
pub fn repo_settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    vec![SettingsCard {
        url: format!("{}/hooks", header.settings_url),
        icon: "webhook",
        title: "Webhooks",
        description: "Notify other services when code is pushed, releases are published and more.",
    }]
}

/// Cards this workstream adds to the organisation settings page; `settings_url` is
/// `/{org}/settings`.
pub fn org_settings_cards(settings_url: &str) -> Vec<SettingsCard> {
    vec![SettingsCard {
        url: format!("{settings_url}/hooks"),
        icon: "webhook",
        title: "Webhooks",
        description: "Notify other services about events in every repository of this organization.",
    }]
}

/// The chrome around a webhook page: a repository's or an organisation's.
#[derive(Debug, Clone)]
pub struct HookFrame {
    pub repo: Option<RepoHeader>,
    pub org: Option<OrgHeader>,
    /// `owner/name` or the organisation's name.
    pub name: String,
    /// `/{owner}/{repo}/settings` or `/{org}/settings`.
    pub settings_url: String,
    /// `…/settings/hooks`.
    pub hooks_url: String,
    // Actions: integration (I).
    /// The instance has Actions: its events are offered.
    pub actions: bool,
}

impl HookFrame {
    pub fn for_repository(header: RepoHeader) -> Self {
        let name = format!("{}/{}", header.owner, header.name);
        let settings_url = header.settings_url.clone();
        Self { hooks_url: format!("{settings_url}/hooks"), settings_url, name, repo: Some(header), org: None, actions: false }
    }

    pub fn for_organization(header: OrgHeader) -> Self {
        let settings_url = header.settings_url.clone();
        Self {
            hooks_url: format!("{settings_url}/hooks"),
            settings_url,
            name: header.name.clone(),
            repo: None,
            org: Some(header),
            actions: false,
        }
    }

    // Actions: integration (I).
    /// Offers the Actions events when `on` (the instance has Actions).
    #[must_use]
    pub fn with_actions(mut self, on: bool) -> Self {
        self.actions = on;
        self
    }

    /// "repository" or "organization", for sentences.
    pub fn kind(&self) -> &'static str {
        if self.repo.is_some() { "repository" } else { "organization" }
    }

    fn hook_url(&self, hook: &Webhook) -> String {
        format!("{}/{}", self.hooks_url, hook.id.0)
    }
}

/// A status badge: colour only says whether the delivery worked.
#[derive(Debug, Clone)]
pub struct StatusBadge {
    pub label: String,
    /// Extra classes for `.badge`.
    pub class: &'static str,
}

impl StatusBadge {
    fn new(status: DeliveryStatus, response_status: Option<u16>) -> Self {
        let class = match status {
            DeliveryStatus::Succeeded => "border-success/40 text-success",
            DeliveryStatus::Failed => "border-danger/40 text-danger",
            DeliveryStatus::Pending | DeliveryStatus::Delivering => "",
        };
        let label = match (status, response_status) {
            (DeliveryStatus::Succeeded | DeliveryStatus::Failed, Some(code)) => format!("{} · {code}", status.label()),
            _ => status.label().to_owned(),
        };
        Self { label, class }
    }

    fn for_last(last: &LastDelivery) -> Self {
        Self::new(last.status, last.response_status)
    }

    /// Like [`Self::new`], but a pending delivery that already failed says it is retrying.
    fn for_delivery(delivery: &Delivery) -> Self {
        let mut badge = Self::new(delivery.status, delivery.response_status);
        if delivery.status == DeliveryStatus::Pending && delivery.attempts > 0 {
            badge.label = match delivery.response_status {
                Some(code) => format!("Retrying · {code}"),
                None => "Retrying".to_owned(),
            };
        }
        badge
    }
}

// ------------------------------------------------------------------ list

/// One hook in the list.
#[derive(Debug, Clone)]
pub struct HookRow {
    pub url: String,
    pub host: String,
    pub events: String,
    pub active: bool,
    pub last: Option<StatusBadge>,
    pub last_at: String,
    pub last_iso: String,
    pub edit_url: String,
}

impl HookRow {
    fn new(frame: &HookFrame, hook: &Webhook) -> Self {
        Self {
            url: hook.url.as_str().to_owned(),
            host: hook.url.host().to_owned(),
            events: event_labels(&hook.events),
            active: hook.active,
            last: hook.last_delivery.as_ref().map(StatusBadge::for_last),
            last_at: hook.last_delivery.map(|last| last.at.relative()).unwrap_or_default(),
            last_iso: hook.last_delivery.map(|last| last.at.iso8601()).unwrap_or_default(),
            edit_url: frame.hook_url(hook),
        }
    }
}

fn event_labels(events: &[WebhookEvent]) -> String {
    events.iter().map(|event| event.label()).collect::<Vec<_>>().join(", ")
}

/// `…/settings/hooks`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "webhooks/list.html")]
pub struct WebhooksPage {
    pub page: Page,
    pub frame: HookFrame,
    pub hooks: Vec<HookRow>,
    pub new_url: String,
    pub can_add: bool,
    pub max_hooks: i64,
}

impl WebhooksPage {
    pub fn new(page: Page, frame: HookFrame, hooks: &[Webhook]) -> Self {
        Self {
            page: page.titled(format!("Webhooks · {}", frame.name)),
            hooks: hooks.iter().map(|hook| HookRow::new(&frame, hook)).collect(),
            new_url: format!("{}/new", frame.hooks_url),
            can_add: i64::try_from(hooks.len()).unwrap_or(i64::MAX) < MAX_HOOKS_PER_OWNER,
            max_hooks: MAX_HOOKS_PER_OWNER,
            frame,
        }
    }
}

// ------------------------------------------------------------------ form

/// One event checkbox.
#[derive(Debug, Clone)]
pub struct EventChoice {
    /// Form field name: `event_push`, …
    pub field: String,
    pub label: &'static str,
    pub description: &'static str,
    pub checked: bool,
}

/// The create/edit form's values.
#[derive(Debug, Clone)]
pub struct HookForm {
    pub url: String,
    pub events: Vec<EventChoice>,
    pub active: bool,
    /// The stored hook has a secret (shown as "configured", never the value).
    pub has_secret: bool,
}

impl HookForm {
    /// A new hook: pushes only, active. `actions` offers the Actions events.
    pub fn blank(actions: bool) -> Self {
        Self::with(String::new(), &[WebhookEvent::Push], true, false, actions)
    }

    pub fn for_hook(hook: &Webhook, actions: bool) -> Self {
        Self::with(hook.url.as_str().to_owned(), &hook.events, hook.active, hook.has_secret, actions)
    }

    pub fn with(url: String, events: &[WebhookEvent], active: bool, has_secret: bool, actions: bool) -> Self {
        let events = WebhookEvent::subscribable(actions)
            .map(|event| EventChoice {
                field: event_field(event),
                label: event.label(),
                description: event.description(),
                checked: events.contains(&event),
            })
            .collect();
        Self { url, events, active, has_secret }
    }
}

/// The form field of an event checkbox.
pub fn event_field(event: WebhookEvent) -> String {
    format!("event_{}", event.as_str())
}

/// One row of the recent deliveries list.
#[derive(Debug, Clone)]
pub struct DeliveryRow {
    pub guid: String,
    pub event: String,
    pub status: StatusBadge,
    pub when: String,
    pub when_iso: String,
    pub duration: String,
    pub url: String,
}

impl DeliveryRow {
    fn new(hook_url: &str, delivery: &Delivery) -> Self {
        let at = delivery.last_attempt_at.unwrap_or(delivery.created_at);
        Self {
            guid: delivery.guid.clone(),
            event: event_name(delivery),
            status: StatusBadge::for_delivery(delivery),
            when: at.relative(),
            when_iso: at.iso8601(),
            duration: delivery.duration_ms.map(|ms| format!("{ms} ms")).unwrap_or_default(),
            url: format!("{hook_url}/deliveries/{}", delivery.id.0),
        }
    }
}

/// `push` or `repository.created`.
fn event_name(delivery: &Delivery) -> String {
    match &delivery.action {
        Some(action) => format!("{}.{action}", delivery.event.as_str()),
        None => delivery.event.as_str().to_owned(),
    }
}

/// `…/hooks/new` and `…/hooks/{id}`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "webhooks/form.html")]
pub struct WebhookFormPage {
    pub page: Page,
    pub frame: HookFrame,
    pub form: HookForm,
    pub error: FormError,
    pub delete_error: FormError,
    /// Where the form posts.
    pub action_url: String,
    /// `Some` when editing an existing hook.
    pub hook: Option<HookActions>,
    pub saved: bool,
    pub pinged: bool,
    pub deliveries: Vec<DeliveryRow>,
    pub pager: Pagination,
}

/// Links and buttons of an existing hook.
#[derive(Debug, Clone)]
pub struct HookActions {
    pub host: String,
    pub ping_url: String,
    pub delete_url: String,
}

impl WebhookFormPage {
    pub fn new_hook(page: Page, frame: HookFrame) -> Self {
        Self {
            page: page.titled(format!("New webhook · {}", frame.name)),
            action_url: frame.hooks_url.clone(),
            form: HookForm::blank(frame.actions),
            frame,
            error: FormError::default(),
            delete_error: FormError::default(),
            hook: None,
            saved: false,
            pinged: false,
            deliveries: Vec::new(),
            pager: Pagination::default(),
        }
    }

    /// The edit page of `hook` with one page of its deliveries (`page_count` pages).
    pub fn edit(page: Page, frame: HookFrame, hook: &Webhook, deliveries: &[Delivery], pager: Pagination) -> Self {
        let hook_url = frame.hook_url(hook);
        Self {
            page: page.titled(format!("Webhook · {}", frame.name)),
            action_url: hook_url.clone(),
            form: HookForm::for_hook(hook, frame.actions),
            error: FormError::default(),
            delete_error: FormError::default(),
            hook: Some(HookActions {
                host: hook.url.host().to_owned(),
                ping_url: format!("{hook_url}/ping"),
                delete_url: format!("{hook_url}/delete"),
            }),
            saved: false,
            pinged: false,
            deliveries: deliveries.iter().map(|delivery| DeliveryRow::new(&hook_url, delivery)).collect(),
            pager,
            frame,
        }
    }

    /// Re-renders with what was submitted and why it was refused.
    #[must_use]
    pub fn rejected(mut self, form: HookForm, error: FormError) -> Self {
        self.form = HookForm { has_secret: self.form.has_secret, ..form };
        self.error = error;
        self
    }

    #[must_use]
    pub fn delete_rejected(mut self, error: FormError) -> Self {
        self.delete_error = error;
        self
    }

    #[must_use]
    pub fn notices(mut self, saved: bool, pinged: bool) -> Self {
        self.saved = saved;
        self.pinged = pinged;
        self
    }
}

// ------------------------------------------------------------------ one delivery

/// `…/hooks/{id}/deliveries/{delivery}`: what was sent and what came back.
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
        let hook_url = frame.hook_url(hook);
        let payload = serde_json::from_str::<serde_json::Value>(&delivery.payload)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .unwrap_or_else(|_| delivery.payload.clone());
        let next_attempt = (delivery.status == DeliveryStatus::Pending)
            .then_some(delivery.next_attempt_at)
            .flatten()
            .map(|at| if at <= Timestamp::now() { "now".to_owned() } else { at.iso8601() });
        Self {
            page: page.titled(format!("Delivery {} · {}", short_guid(&delivery.guid), frame.name)),
            redeliver_url: format!("{hook_url}/deliveries/{}/redeliver", delivery.id.0),
            hook_url,
            guid: delivery.guid.clone(),
            event: event_name(delivery),
            status: StatusBadge::for_delivery(delivery),
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
}

fn short_guid(guid: &str) -> &str {
    guid.get(..8).unwrap_or(guid)
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        AccountId, RepoId,
        webhook::{DeliveryId, WebhookId, WebhookOwner, WebhookUrl},
    };

    use super::*;

    fn header() -> RepoHeader {
        RepoHeader {
            owner: "ada".to_owned(),
            name: "demo".to_owned(),
            home_url: "/ada/demo".to_owned(),
            settings_url: "/ada/demo/settings".to_owned(),
            ..RepoHeader::default()
        }
    }

    fn hook(last: Option<LastDelivery>) -> Webhook {
        Webhook {
            id: WebhookId(4),
            owner: WebhookOwner::Repository(RepoId(1)),
            url: WebhookUrl::parse("https://ci.example.com/hook?token=abc").unwrap(),
            events: vec![WebhookEvent::Push, WebhookEvent::Release],
            active: true,
            has_secret: true,
            created_by: Some(AccountId(1)),
            created_at: Timestamp(1_792_022_400),
            updated_at: Timestamp(1_792_022_400),
            last_delivery: last,
        }
    }

    fn delivery(status: DeliveryStatus, response_status: Option<u16>) -> Delivery {
        Delivery {
            id: DeliveryId(9),
            hook_id: WebhookId(4),
            guid: "0f8fad5b-d9cb-469f-a165-70867728950e".to_owned(),
            event: WebhookEvent::Repository,
            action: Some("created".to_owned()),
            payload: r#"{"action":"created"}"#.to_owned(),
            status,
            attempts: 2,
            next_attempt_at: None,
            last_attempt_at: Some(Timestamp(1_792_022_400)),
            request_headers: vec![("X-Rbit-Event".to_owned(), "repository".to_owned())],
            response_status,
            response_headers: Vec::new(),
            response_body: Some(String::new()),
            duration_ms: Some(42),
            error: None,
            created_at: Timestamp(1_792_022_400),
        }
    }

    #[test]
    fn cards_link_to_the_hooks_pages() {
        assert_eq!(repo_settings_cards(&header())[0].url, "/ada/demo/settings/hooks");
        assert_eq!(org_settings_cards("/acme/settings")[0].url, "/acme/settings/hooks");
        assert_eq!(repo_settings_cards(&header())[0].icon, "webhook");
    }

    #[test]
    fn rows_describe_hooks_and_their_last_delivery() {
        let frame = HookFrame::for_repository(header());
        assert_eq!((frame.name.as_str(), frame.hooks_url.as_str(), frame.kind()), ("ada/demo", "/ada/demo/settings/hooks", "repository"));
        let last = LastDelivery { status: DeliveryStatus::Failed, response_status: Some(500), at: Timestamp(1_792_022_400) };
        let row = HookRow::new(&frame, &hook(Some(last)));
        assert_eq!(row.host, "ci.example.com");
        assert_eq!(row.events, "Pushes, Releases");
        assert_eq!(row.edit_url, "/ada/demo/settings/hooks/4");
        let badge = row.last.unwrap();
        assert_eq!((badge.label.as_str(), badge.class), ("Failed · 500", "border-danger/40 text-danger"));
        assert!(HookRow::new(&frame, &hook(None)).last.is_none());
    }

    #[test]
    fn forms_preselect_events_and_never_carry_the_secret() {
        let blank = HookForm::blank(false);
        let checked: Vec<_> = blank.events.iter().filter(|choice| choice.checked).map(|choice| choice.field.as_str()).collect();
        assert_eq!(checked, ["event_push"]);
        assert!(blank.active && !blank.has_secret);

        let edit = HookForm::for_hook(&hook(None), false);
        let checked: Vec<_> = edit.events.iter().filter(|choice| choice.checked).map(|choice| choice.label).collect();
        assert_eq!(checked, ["Pushes", "Releases"]);
        assert!(edit.has_secret);
        assert_eq!(edit.events.len(), WebhookEvent::SUBSCRIBABLE.len());
    }

    // Actions: integration (I).
    #[test]
    fn actions_events_are_offered_only_with_actions() {
        let fields = |form: HookForm| form.events.into_iter().map(|choice| choice.field).collect::<Vec<_>>();
        assert!(!fields(HookForm::blank(false)).iter().any(|field| field.starts_with("event_workflow")));
        let on = fields(HookForm::blank(true));
        assert!(on.ends_with(&["event_workflow_run".to_owned(), "event_workflow_job".to_owned()]));
    }

    #[test]
    fn delivery_pages_pretty_print_payloads() {
        let frame = HookFrame::for_repository(header());
        let page = DeliveryPage::new(page(), frame, &hook(None), &delivery(DeliveryStatus::Succeeded, Some(204)));
        assert_eq!(page.event, "repository.created");
        assert_eq!(page.payload, "{\n  \"action\": \"created\"\n}");
        assert_eq!(page.status.label, "Delivered · 204");
        assert_eq!(page.redeliver_url, "/ada/demo/settings/hooks/4/deliveries/9/redeliver");
        assert_eq!(page.response_body, None, "an empty body is not shown");
        assert_eq!(page.next_attempt, None);
        assert_eq!(DeliveryRow::new("/x", &delivery(DeliveryStatus::Pending, None)).status.label, "Retrying", "two attempts were made");
        let fresh = Delivery { attempts: 0, ..delivery(DeliveryStatus::Pending, None) };
        assert_eq!(DeliveryRow::new("/x", &fresh).status.label, "Pending");
        assert_eq!(DeliveryRow::new("/x", &delivery(DeliveryStatus::Pending, Some(502))).status.label, "Retrying · 502");
    }

    fn page() -> Page {
        Page {
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".to_owned(),
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }
}
