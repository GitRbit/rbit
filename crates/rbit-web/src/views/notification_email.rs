//! Email notification pages: Settings → Notifications and the one-click unsubscribe page.
//!
//! Milestone 3 workstream N (email notifications) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::config::{EmailDelivery, EmailPreference, MAX_DIGEST_HOUR};
use rbit_services::notification_email::EmailSettings;

use crate::views::{
    components::FormError,
    layout::Page,
    settings::{SettingsLink, SettingsSection},
};

/// One choice of the preference form.
#[derive(Debug, Clone)]
pub struct PreferenceOption {
    pub value: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub checked: bool,
}

impl PreferenceOption {
    fn all(selected: EmailPreference) -> Vec<Self> {
        EmailPreference::ALL
            .into_iter()
            .map(|preference| {
                let (label, description) = match preference {
                    EmailPreference::Off => ("Off", "Notifications appear in your inbox only."),
                    EmailPreference::Participating => (
                        "Participating",
                        "Threads you opened, commented on or reviewed, and when you are mentioned, assigned or asked for a review.",
                    ),
                    EmailPreference::All => ("Everything", "Also threads you subscribed to without taking part."),
                };
                Self { value: preference.as_str(), label, description, checked: preference == selected }
            })
            .collect()
    }
}

/// One choice of the delivery form.
#[derive(Debug, Clone)]
pub struct DeliveryOption {
    pub value: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub checked: bool,
}

impl DeliveryOption {
    fn all(selected: EmailDelivery) -> Vec<Self> {
        EmailDelivery::ALL
            .into_iter()
            .map(|delivery| {
                let (label, description) = match delivery {
                    EmailDelivery::Immediate => ("Immediately", "One email for each notification, as it happens."),
                    EmailDelivery::Daily => ("Daily digest", "One email a day with everything since the last one."),
                    EmailDelivery::Weekly => ("Weekly digest", "One email every Monday with everything from the week before."),
                };
                Self { value: delivery.as_str(), label, description, checked: delivery == selected }
            })
            .collect()
    }
}

/// One entry of the digest time menu.
#[derive(Debug, Clone)]
pub struct HourOption {
    /// The form value: empty for the instance default, otherwise the hour.
    pub value: String,
    pub label: String,
    pub selected: bool,
}

impl HourOption {
    fn all(chosen: Option<u8>, default: u8) -> Vec<Self> {
        let default = Self { value: String::new(), label: format!("Default ({})", hour_label(default)), selected: chosen.is_none() };
        std::iter::once(default)
            .chain((0..=MAX_DIGEST_HOUR).map(|hour| Self {
                value: hour.to_string(),
                label: hour_label(hour),
                selected: chosen == Some(hour),
            }))
            .collect()
    }
}

/// `08:00 UTC`.
pub fn hour_label(hour: u8) -> String {
    format!("{hour:02}:00 UTC")
}

/// `/settings/notifications`: which notifications are also emailed.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/notifications.html")]
pub struct NotificationSettingsPage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub options: Vec<PreferenceOption>,
    pub deliveries: Vec<DeliveryOption>,
    pub hours: Vec<HourOption>,
    /// The address notifications go to.
    pub address: String,
    pub verified: bool,
    /// Whether the server sends notification emails at all.
    pub available: bool,
    /// Set after a successful save (`?saved=1`).
    pub saved: bool,
    pub error: FormError,
}

impl NotificationSettingsPage {
    pub fn new(page: Page, settings: &EmailSettings) -> Self {
        Self {
            page: page.titled(crate::i18n::text("core", "title-notifications")),
            settings_nav: SettingsSection::Notifications.nav(),
            options: PreferenceOption::all(settings.preference),
            deliveries: DeliveryOption::all(settings.delivery),
            hours: HourOption::all(settings.digest_hour, settings.default_digest_hour),
            address: settings.address.to_string(),
            verified: settings.verified,
            available: settings.available,
            saved: false,
            error: FormError::default(),
        }
    }

    #[must_use]
    pub fn saved(mut self, saved: bool) -> Self {
        self.saved = saved;
        self
    }

    #[must_use]
    pub fn with_error(mut self, error: FormError) -> Self {
        self.error = error;
        self
    }
}

/// What the unsubscribe page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsubscribeState {
    /// A valid link, before confirming (`GET`): link scanners must not unsubscribe anyone.
    Confirm { username: String },
    /// Notification emails are off now.
    Done { username: String },
    /// Malformed, forged or of a deleted account.
    Invalid,
}

/// `/email/unsubscribe/{token}`: the target of the unsubscribe link in every notification email.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "email/unsubscribe.html")]
pub struct UnsubscribePage {
    pub page: Page,
    pub state: UnsubscribeState,
    /// Where the confirmation form posts (the link itself).
    pub action: String,
    pub settings_url: &'static str,
}

impl UnsubscribePage {
    pub fn new(page: Page, state: UnsubscribeState, action: String) -> Self {
        let title = match state {
            UnsubscribeState::Confirm { .. } => "Stop notification emails",
            UnsubscribeState::Done { .. } => "Notification emails are off",
            UnsubscribeState::Invalid => "This link is not valid",
        };
        Self { page: page.titled(title), state, action, settings_url: SettingsSection::Notifications.url() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        Page {
            locale: rbit_messages::LocaleSnapshot::english(0),
            has_i18n: false,
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".to_owned(),
            plugin_revision: 0,
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/settings/notifications".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    fn settings(preference: EmailPreference, verified: bool, available: bool) -> EmailSettings {
        EmailSettings {
            preference,
            chosen: true,
            delivery: EmailDelivery::Immediate,
            digest_hour: None,
            default_digest_hour: 8,
            address: "alice@example.com".parse().unwrap(),
            verified,
            available,
        }
    }

    #[test]
    fn settings_page_checks_the_current_choice() {
        let html = NotificationSettingsPage::new(page(), &settings(EmailPreference::All, true, true)).render().unwrap();
        assert!(html.contains(r#"action="/settings/notifications""#));
        assert!(html.contains(r#"value="all" class="mt-1 accent-(--ink)" checked"#), "{html}");
        assert!(!html.contains(r#"value="off" class="mt-1 accent-(--ink)" checked"#));
        assert!(html.contains("alice@example.com"));
        assert!(!html.contains("not available on this server"));
        assert!(html.contains(r#"aria-current="page">Notifications"#), "the navigation marks the section");

        let html = NotificationSettingsPage::new(page(), &settings(EmailPreference::Participating, false, true)).render().unwrap();
        assert!(html.contains("Verify"), "unverified addresses are explained");
        assert!(html.contains(r#"href="/settings/account""#));

        assert!(html.contains(r#"value="immediate" class="mt-1 accent-(--ink)" checked"#), "immediate is the default");
        assert!(html.contains(r#"<option value="" selected>Default (08:00 UTC)</option>"#), "{html}");

        let weekly = EmailSettings { delivery: EmailDelivery::Weekly, digest_hour: Some(17), ..settings(EmailPreference::All, true, true) };
        let html = NotificationSettingsPage::new(page(), &weekly).render().unwrap();
        assert!(html.contains(r#"value="weekly" class="mt-1 accent-(--ink)" checked"#));
        assert!(!html.contains(r#"value="immediate" class="mt-1 accent-(--ink)" checked"#));
        assert!(html.contains(r#"<option value="17" selected>17:00 UTC</option>"#));
        assert!(html.contains(r#"<option value="0">00:00 UTC</option>"#));

        let html = NotificationSettingsPage::new(page(), &settings(EmailPreference::Off, true, false)).saved(true).render().unwrap();
        assert!(html.contains("not available on this server"));
        assert!(html.contains("Preferences saved."));
    }

    #[test]
    fn unsubscribe_page_renders_each_state() {
        let confirm = UnsubscribeState::Confirm { username: "bob<".to_owned() };
        let html = UnsubscribePage::new(page(), confirm, "/email/unsubscribe/2.ab".to_owned()).render().unwrap();
        assert!(html.contains(r#"<form method="post" action="/email/unsubscribe/2.ab""#));
        assert!(html.contains("bob&#60;") || html.contains("bob&lt;"), "the name is escaped");

        let done = UnsubscribeState::Done { username: "bob".to_owned() };
        let html = UnsubscribePage::new(page(), done, String::new()).render().unwrap();
        assert!(html.contains("Notification emails are off"));
        assert!(html.contains(r#"href="/settings/notifications""#));
        assert!(!html.contains("Turn off notification emails</button>"), "nothing left to confirm");

        let html = UnsubscribePage::new(page(), UnsubscribeState::Invalid, String::new()).render().unwrap();
        assert!(html.contains("This link is not valid"));
    }
}
