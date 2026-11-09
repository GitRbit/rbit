//! The administration audit log at `/admin/audit`.
use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_localization::{
    Locale, Localized,
    settings::{
        admin::{AdminMessage, AdminText},
        admin_pages::AdminPageMessage,
    },
};
use rbit_services::{
    admin::Paged,
    audit::{AuditCategory, AuditEvent},
};

use super::{AdminLink, AdminSection};
use crate::views::{
    layout::Page,
    when::{date, relative},
};

const AUDIT_URL: &str = AdminSection::AuditLog.url();

/// `/admin/audit?q=…&category=…&page=…`, omitting defaults.
pub fn audit_url(search: &str, category: Option<AuditCategory>, page: u32) -> String {
    let mut params = Vec::new();
    if !search.is_empty() {
        params.push(format!("q={}", utf8_percent_encode(search, NON_ALPHANUMERIC)));
    }
    if let Some(category) = category {
        params.push(format!("category={}", category.as_str()));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if params.is_empty() { AUDIT_URL.to_owned() } else { format!("{AUDIT_URL}?{}", params.join("&")) }
}

/// One `<option>` of the category filter.
#[derive(Debug, Clone)]
pub struct CategoryOption {
    pub value: &'static str,
    pub label: String,
    pub selected: bool,
}

/// Search, filter and pagination state (the fields `admin/components.html`'s `pager` reads).
#[derive(Debug, Clone)]
pub struct AuditListing {
    pub search: String,
    pub categories: Vec<CategoryOption>,
    /// e.g. `1–30 of 112`, or `No events`.
    summary: Localized,
    pub previous_url: Option<String>,
    pub next_url: Option<String>,
    pub is_filtered: bool,
}

impl AuditListing {
    /// The result count, as `admin/components.html`'s `pager` shows it.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
    pub fn summary(&self, _locale: &Locale) -> Localized {
        self.summary.clone()
    }

    pub fn has_pages(&self) -> bool {
        self.previous_url.is_some() || self.next_url.is_some()
    }
}

/// One entry of the log.
#[derive(Debug, Clone)]
pub struct AuditRow {
    pub label: &'static str,
    pub category: &'static str,
    pub is_failure: bool,
    pub actor: Option<String>,
    /// The actor is only a login someone tried (failed sign-in), not a verified account.
    pub actor_unverified: bool,
    pub target: Option<String>,
    pub ip_address: Option<String>,
    pub detail: String,
    /// e.g. `Oct 15, 2026`, with the relative time as a tooltip.
    pub date: Localized,
    pub relative: Localized,
    pub iso: String,
}

impl AuditRow {
    pub fn new(event: &AuditEvent, locale: Locale) -> Self {
        Self {
            label: event.action.label(),
            category: event.action.category().label(),
            is_failure: event.action.is_failure(),
            actor: event.actor_name.clone(),
            actor_unverified: event.actor_id.is_none(),
            target: event.target_name.clone(),
            ip_address: event.ip_address.clone(),
            detail: event.detail.clone(),
            date: date(event.created_at, locale),
            relative: relative(event.created_at, locale),
            iso: event.created_at.iso8601(),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/audit.html")]
pub struct AuditPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub listing: AuditListing,
    pub events: Vec<AuditRow>,
}

impl AuditPage {
    pub fn new(page: Page, search: &str, category: Option<AuditCategory>, paged: &Paged<AuditEvent>) -> Self {
        let locale = page.locale;
        let summary = if paged.total == 0 {
            AdminPageMessage::NoEvents.text(&locale)
        } else {
            let (first, last, total) = (paged.first_index().to_string(), paged.last_index().to_string(), paged.total.to_string());
            AdminText::Summary(&first, &last, &total).text(&locale)
        };
        let all = AdminPageMessage::AllEvents.text(&locale).to_string();
        let categories = std::iter::once(CategoryOption { value: "", label: all, selected: category.is_none() })
            .chain(AuditCategory::ALL.into_iter().map(|option| CategoryOption {
                value: option.as_str(),
                label: option.label().to_owned(),
                selected: category == Some(option),
            }))
            .collect();
        Self {
            page: super::titled(page, AdminMessage::AuditLog),
            admin_nav: AdminSection::AuditLog.nav(),
            listing: AuditListing {
                search: search.to_owned(),
                categories,
                summary,
                previous_url: paged.has_previous().then(|| audit_url(search, category, paged.page - 1)),
                next_url: paged.has_next().then(|| audit_url(search, category, paged.page + 1)),
                is_filtered: !search.is_empty() || category.is_some(),
            },
            events: paged.items.iter().map(|event| AuditRow::new(event, locale)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::Timestamp;
    use rbit_services::audit::AuditAction;

    use super::*;

    fn page() -> Page {
        crate::views::test_support::page(AUDIT_URL)
    }

    fn event(id: i64, action: AuditAction, actor: Option<(i64, &str)>) -> AuditEvent {
        AuditEvent {
            id,
            created_at: Timestamp(1_792_022_400),
            action,
            actor_id: actor.map(|(id, _)| rbit_core::AccountId(id)),
            actor_name: actor.map(|(_, name)| name.to_owned()),
            target_name: Some("mallory".to_owned()),
            ip_address: Some("198.51.100.23".to_owned()),
            detail: String::new(),
        }
    }

    #[test]
    fn audit_urls_keep_filters() {
        assert_eq!(audit_url("", None, 1), "/admin/audit");
        assert_eq!(audit_url("a b", Some(AuditCategory::TwoFactor), 3), "/admin/audit?q=a%20b&category=two_factor&page=3");
    }

    #[test]
    fn audit_page_renders_events_filters_and_paging() {
        let mut failed = event(1, AuditAction::SignInFailed, Some((0, "ghost")));
        failed.actor_id = None;
        let paged =
            Paged { items: vec![event(2, AuditAction::UserSuspended, Some((1, "root"))), failed], total: 32, page: 2, per_page: 30 };
        let html = AuditPage::new(page(), "mall", Some(AuditCategory::Administration), &paged).render().unwrap();

        assert!(html.contains("Suspended a user"));
        assert!(html.contains(r#"href="/root""#), "verified actors link to their profile");
        assert!(html.contains("as <span class=\"font-mono\" dir=\"ltr\">ghost</span>"), "attempted logins are not links");
        assert!(html.contains("198.51.100.23"));
        assert!(html.contains(r#"<option value="administration" selected>"#));
        assert!(html.contains(r#"value="mall""#));
        assert!(html.contains("31–32 of 32"));
        assert!(html.contains("rel=\"prev\""));
        assert!(html.contains("q=mall") && html.contains("category=administration"), "paging keeps the filters");
        assert!(html.contains(r#"href="/admin/audit""#), "clear filters");
        assert!(html.contains("text-danger"), "failures stand out");
        assert!(html.contains(r#"aria-current="page""#));
    }

    #[test]
    fn empty_audit_page() {
        let paged = Paged { items: Vec::new(), total: 0, page: 1, per_page: 30 };
        let html = AuditPage::new(page(), "", None, &paged).render().unwrap();
        assert!(html.contains("No events"));
        assert!(!html.contains("rel=\"next\""));
    }
}
