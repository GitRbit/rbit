//! Administration panel pages (`/admin/…`) and their view models.

pub mod audit;
pub mod configuration;
pub mod dashboard;
pub mod monitoring;
pub mod restart;

use std::collections::HashSet;

use crate::views::notification_email::hour_label;
use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{AccountId, InstanceSettings, OrgCreationPolicy, OwnedRepository, RegistrationMode, User, Visibility};
use rbit_localization::{
    Catalog, Locale, Localized, MessageFamily,
    settings::{
        admin::{AdminMessage, AdminText},
        admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage},
    },
};
use rbit_services::admin::{OrgRecord, Paged, UserRecord};

use crate::views::{
    components::{Avatar, FormError},
    layout::Page,
    when::{count, date, relative},
};

// ------------------------------------------------------------------ shell

/// An administration area, listed in the left-hand navigation of `admin/layout.html`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminSection {
    Dashboard,
    Users,
    Organizations,
    Repositories,
    Settings,
    AuditLog,
    Configuration,
    // Actions: interface and secrets (U).
    /// Only on an instance with Actions: see [`AdminLink::needs_actions`].
    Runners,
}

impl AdminSection {
    const ALL: [Self; 8] = [
        Self::Dashboard,
        Self::Users,
        Self::Organizations,
        Self::Repositories,
        Self::Settings,
        Self::Configuration,
        Self::AuditLog,
        Self::Runners,
    ];

    pub const fn label(self) -> AdminMessage {
        match self {
            Self::Dashboard => AdminMessage::Dashboard,
            Self::Users => AdminMessage::Users,
            Self::Organizations => AdminMessage::Organizations,
            Self::Repositories => AdminMessage::Repositories,
            Self::Settings => AdminMessage::Settings,
            Self::AuditLog => AdminMessage::AuditLog,
            Self::Configuration => AdminMessage::Configuration,
            Self::Runners => AdminMessage::Runners,
        }
    }

    pub const fn url(self) -> &'static str {
        match self {
            Self::Dashboard => "/admin",
            Self::Users => "/admin/users",
            Self::Organizations => "/admin/organizations",
            Self::Repositories => "/admin/repositories",
            Self::Settings => "/admin/settings",
            Self::AuditLog => "/admin/audit",
            Self::Configuration => "/admin/configuration",
            Self::Runners => "/admin/runners",
        }
    }

    /// The navigation with `self` marked as the current page.
    pub fn nav(self) -> Vec<AdminLink> {
        Self::ALL
            .iter()
            .map(|&section| AdminLink {
                label: section.label(),
                url: section.url(),
                is_active: section == self,
                needs_actions: section == Self::Runners,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AdminLink {
    pub label: AdminMessage,
    pub url: &'static str,
    pub is_active: bool,
    // Actions: interface and secrets (U).
    /// Shown only on an instance with Actions ([`Page::has_actions`]); `admin/layout.html`
    /// leaves the link out otherwise.
    pub needs_actions: bool,
}

/// `base?q=…&page=…`, omitting defaults.
pub fn listing_url(base: &str, search: &str, page: u32) -> String {
    let mut params = Vec::new();
    if !search.is_empty() {
        params.push(format!("q={}", utf8_percent_encode(search, NON_ALPHANUMERIC)));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if params.is_empty() { base.to_owned() } else { format!("{base}?{}", params.join("&")) }
}

/// Search box and pagination state shared by every listing.
#[derive(Debug, Clone)]
pub struct Listing {
    /// Where the search form submits (the listing's own URL).
    pub action: &'static str,
    pub search: String,
    pub page: u32,
    /// e.g. `1–30 of 112`; empty when there are no results.
    summary: [String; 3],
    pub previous_url: Option<String>,
    pub next_url: Option<String>,
}

impl Listing {
    pub fn new<T>(section: AdminSection, search: &str, paged: &Paged<T>) -> Self {
        let base = section.url();
        let summary = if paged.total == 0 {
            [String::new(), String::new(), String::new()]
        } else {
            [paged.first_index().to_string(), paged.last_index().to_string(), paged.total.to_string()]
        };
        Self {
            action: base,
            search: search.to_owned(),
            page: paged.page,
            summary,
            previous_url: paged.has_previous().then(|| listing_url(base, search, paged.page - 1)),
            next_url: paged.has_next().then(|| listing_url(base, search, paged.page + 1)),
        }
    }

    /// The URL of this exact page (used to come back after an action).
    pub fn current_url(&self) -> String {
        listing_url(self.action, &self.search, self.page)
    }

    /// `1–30 of 112`, or `No results`.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
    pub fn summary(&self, locale: &Locale) -> Localized {
        let [first, last, total] = &self.summary;
        if total.is_empty() { AdminMessage::NoResults.text(locale) } else { AdminText::Summary(first, last, total).text(locale) }
    }

    pub fn has_pages(&self) -> bool {
        self.previous_url.is_some() || self.next_url.is_some()
    }
}

// ------------------------------------------------------------------ users

/// One user in the users table.
#[allow(clippy::struct_excessive_bools)] // Independent display flags, not a state machine.
#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: AccountId,
    pub name: String,
    pub label: String,
    pub has_display_name: bool,
    pub email: String,
    pub profile_url: String,
    pub avatar: Avatar,
    pub is_admin: bool,
    pub is_suspended: bool,
    /// The user has confirmed their email address.
    pub is_verified: bool,
    /// The row is the viewing administrator (self-targeted actions are hidden).
    pub is_self: bool,
    pub repositories: String,
    pub joined: Localized,
    pub joined_iso: String,
    pub promote_url: String,
    pub demote_url: String,
    pub suspend_url: String,
    pub unsuspend_url: String,
    pub delete_url: String,
    /// `POST`: emails the user a password-reset link.
    pub send_reset_url: String,
    /// `GET`/`POST`: sets the user's password directly.
    pub password_url: String,
}

impl UserRow {
    pub fn new(record: &UserRecord, viewer: AccountId, locale: Locale) -> Self {
        let user = &record.user;
        let base = format!("/admin/users/{}", user.id().0);
        Self {
            id: user.id(),
            name: user.name().to_string(),
            label: user.account.label().to_owned(),
            has_display_name: user.account.display_name.as_deref().is_some_and(|name| !name.is_empty()),
            email: user.email.as_str().to_owned(),
            profile_url: format!("/{}", user.name()),
            avatar: Avatar::for_account(&user.account),
            is_admin: user.is_admin,
            is_suspended: record.is_suspended(),
            is_verified: false,
            is_self: user.id() == viewer,
            repositories: count(locale, record.repository_count),
            joined: date(user.account.created_at, locale),
            joined_iso: user.account.created_at.iso8601(),
            promote_url: format!("{base}/promote"),
            demote_url: format!("{base}/demote"),
            suspend_url: format!("{base}/suspend"),
            unsuspend_url: format!("{base}/unsuspend"),
            delete_url: format!("{base}/delete"),
            send_reset_url: format!("{base}/password-reset"),
            password_url: format!("{base}/password"),
        }
    }
}

/// A confirmation shown above the users table after an action (`?notice=…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsersNotice {
    ResetSent,
    PasswordSet,
}

impl UsersNotice {
    pub const fn key(self) -> &'static str {
        match self {
            Self::ResetSent => "reset-sent",
            Self::PasswordSet => "password-set",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::ResetSent, Self::PasswordSet].into_iter().find(|notice| notice.key() == value)
    }

    pub const fn message(self) -> AdminPageMessage {
        match self {
            Self::ResetSent => AdminPageMessage::ResetSent,
            Self::PasswordSet => AdminPageMessage::PasswordSet,
        }
    }

    /// `listing` (a URL from [`listing_url`]) with this notice appended.
    pub fn url(self, listing: &str) -> String {
        let separator = if listing.contains('?') { '&' } else { '?' };
        format!("{listing}{separator}notice={}", self.key())
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/users.html")]
pub struct UsersPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub error: FormError,
    pub notice: Option<UsersNotice>,
    pub listing: Listing,
    pub users: Vec<UserRow>,
    /// Whether "Send password reset" can be offered.
    pub email_available: bool,
}

impl UsersPage {
    pub fn new(page: Page, viewer: &User, search: &str, paged: &Paged<UserRecord>) -> Self {
        let locale = page.locale;
        Self {
            page: titled(page, AdminMessage::Users),
            admin_nav: AdminSection::Users.nav(),
            error: FormError::default(),
            notice: None,
            listing: Listing::new(AdminSection::Users, search, paged),
            users: paged.items.iter().map(|record| UserRow::new(record, viewer.id(), locale)).collect(),
            email_available: false,
        }
    }

    /// Marks the users in `verified`, and records whether reset emails can be sent.
    #[must_use]
    pub fn with_email(mut self, verified: &HashSet<AccountId>, email_available: bool) -> Self {
        for row in &mut self.users {
            row.is_verified = verified.contains(&row.id);
        }
        self.email_available = email_available;
        self
    }

    #[must_use]
    pub fn with_error(mut self, error: FormError) -> Self {
        self.error = error;
        self
    }

    #[must_use]
    pub fn with_notice(mut self, notice: Option<UsersNotice>) -> Self {
        self.notice = notice;
        self
    }
}

/// `/admin/users/{id}/password`: sets a user's password directly.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/set_password.html")]
pub struct SetPasswordPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub error: FormError,
    pub user: UserRow,
    pub min_password_length: usize,
    pub email_available: bool,
}

impl SetPasswordPage {
    pub fn new(page: Page, viewer: &User, record: &UserRecord, min_password_length: usize, email_available: bool) -> Self {
        let user = UserRow::new(record, viewer.id(), page.locale);
        Self {
            page: {
                let title = AdminText::SetPasswordFor(&user.name).text(&page.locale);
                page.titled(title.as_str())
            },
            admin_nav: AdminSection::Users.nav(),
            error: FormError::default(),
            user,
            min_password_length,
            email_available,
        }
    }

    #[must_use]
    pub fn with_error(mut self, error: FormError) -> Self {
        self.error = error;
        self
    }

    /// The new password's length hint.
    pub fn password_length(&self) -> Localized {
        AdminText::PasswordLength(u64::try_from(self.min_password_length).unwrap_or(u64::MAX)).text(&self.page.locale)
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/new_user.html")]
pub struct NewUserPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub error: FormError,
    pub username: String,
    pub email: String,
    pub is_admin: bool,
    pub min_password_length: usize,
    pub registration_open: bool,
}

impl NewUserPage {
    /// The initial password's length hint.
    pub fn password_hint(&self) -> Localized {
        AdminPageText::ShareSecurely(u64::try_from(self.min_password_length).unwrap_or(u64::MAX)).text(&self.page.locale)
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/delete_user.html")]
pub struct DeleteUserPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub error: FormError,
    pub user: UserRow,
}

impl DeleteUserPage {
    pub fn new(page: Page, viewer: &User, record: &UserRecord, error: FormError) -> Self {
        let user = UserRow::new(record, viewer.id(), page.locale);
        let title = AdminText::DeletePageTitle(&user.name).text(&page.locale);
        Self { page: page.titled(title.as_str()), admin_nav: AdminSection::Users.nav(), error, user }
    }
}

// ------------------------------------------------------------------ organisations

#[derive(Debug, Clone)]
pub struct OrgRow {
    pub name: String,
    pub label: String,
    pub has_display_name: bool,
    pub url: String,
    pub avatar: Avatar,
    pub members: String,
    pub repositories: String,
    pub created: Localized,
    pub created_iso: String,
}

impl OrgRow {
    fn new(record: &OrgRecord, locale: Locale) -> Self {
        let account = &record.account;
        Self {
            name: account.name.to_string(),
            label: account.label().to_owned(),
            has_display_name: account.display_name.as_deref().is_some_and(|name| !name.is_empty()),
            url: format!("/{}", account.name),
            avatar: Avatar::for_account(account),
            members: count(locale, record.member_count),
            repositories: count(locale, record.repository_count),
            created: date(account.created_at, locale),
            created_iso: account.created_at.iso8601(),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/organizations.html")]
pub struct OrganizationsPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub listing: Listing,
    pub orgs: Vec<OrgRow>,
}

impl OrganizationsPage {
    pub fn new(page: Page, search: &str, paged: &Paged<OrgRecord>) -> Self {
        let locale = page.locale;
        Self {
            page: titled(page, AdminMessage::Organizations),
            admin_nav: AdminSection::Organizations.nav(),
            listing: Listing::new(AdminSection::Organizations, search, paged),
            orgs: paged.items.iter().map(|record| OrgRow::new(record, locale)).collect(),
        }
    }
}

// ------------------------------------------------------------------ repositories

#[derive(Debug, Clone)]
pub struct RepoRow {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub url: String,
    pub owner_url: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub is_archived: bool,
    pub updated: Localized,
    pub updated_iso: String,
    pub delete_url: String,
}

impl RepoRow {
    fn new(item: &OwnedRepository, locale: Locale) -> Self {
        Self {
            owner: item.owner.name.to_string(),
            name: item.repo.name.to_string(),
            full_name: item.full_name(),
            url: format!("/{}/{}", item.owner.name, item.repo.name),
            owner_url: format!("/{}", item.owner.name),
            description: item.repo.description.clone().filter(|d| !d.is_empty()),
            is_private: !item.repo.visibility.is_public(),
            is_archived: item.repo.is_archived(),
            updated: relative(item.repo.updated_at, locale),
            updated_iso: item.repo.updated_at.iso8601(),
            delete_url: format!("/admin/repositories/{}/delete", item.repo.id.0),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/repositories.html")]
pub struct RepositoriesPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub listing: Listing,
    pub repos: Vec<RepoRow>,
}

impl RepositoriesPage {
    pub fn new(page: Page, search: &str, paged: &Paged<OwnedRepository>) -> Self {
        let locale = page.locale;
        Self {
            page: titled(page, AdminMessage::Repositories),
            admin_nav: AdminSection::Repositories.nav(),
            listing: Listing::new(AdminSection::Repositories, search, paged),
            repos: paged.items.iter().map(|item| RepoRow::new(item, locale)).collect(),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/delete_repository.html")]
pub struct DeleteRepositoryPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub error: FormError,
    pub repo: RepoRow,
}

impl DeleteRepositoryPage {
    pub fn new(page: Page, repo: &OwnedRepository, error: FormError) -> Self {
        let repo = RepoRow::new(repo, page.locale);
        Self {
            page: {
                let title = AdminText::DeletePageTitle(&repo.full_name).text(&page.locale);
                page.titled(title.as_str())
            },
            admin_nav: AdminSection::Repositories.nav(),
            error,
            repo,
        }
    }
}

// ------------------------------------------------------------------ instance settings

/// The settings form's values (current settings, or as submitted when re-rendered).
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools, reason = "one per on/off choice of the form")]
pub struct SettingsForm {
    pub site_name: String,
    pub branding: rbit_core::config::BrandingConfig,
    pub registration_open: bool,
    pub explore_enabled: bool,
    pub admins_only_orgs: bool,
    pub private_by_default: bool,
    /// `None` when the submitted value was not an hour (nothing is preselected).
    pub digest_hour: Option<u8>,
}

impl SettingsForm {
    /// The digest time menu: every hour, the current one selected.
    pub fn digest_hours(&self) -> Vec<(u8, String, bool)> {
        (0..=rbit_core::config::MAX_DIGEST_HOUR).map(|hour| (hour, hour_label(hour), self.digest_hour == Some(hour))).collect()
    }
}

impl From<&InstanceSettings> for SettingsForm {
    fn from(settings: &InstanceSettings) -> Self {
        Self {
            site_name: settings.site_name.clone(),
            branding: settings.branding.clone(),
            registration_open: settings.registration == RegistrationMode::Open,
            explore_enabled: settings.explore_enabled,
            admins_only_orgs: settings.org_creation == OrgCreationPolicy::AdminsOnly,
            private_by_default: settings.default_visibility == Visibility::Private,
            digest_hour: Some(settings.digest_hour),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/settings.html")]
pub struct SettingsPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub error: FormError,
    pub form: SettingsForm,
    pub saved: bool,
    pub site_name_max_len: usize,
}

impl SettingsPage {
    pub fn new(page: Page, form: SettingsForm, error: FormError, saved: bool) -> Self {
        Self {
            page: titled(page, AdminMessage::Settings),
            admin_nav: AdminSection::Settings.nav(),
            error,
            form,
            saved,
            site_name_max_len: rbit_services::instance::SITE_NAME_MAX_LEN,
        }
    }
}

// ------------------------------------------------------------------ formatting

/// `Users · Administration`, in the page's language.
pub fn titled<M: MessageFamily>(page: Page, section: M) -> Page {
    let section = Catalog::new(page.locale).localized(section);
    let title = AdminText::InAdministration(section.as_str()).text(&page.locale);
    page.titled(title.as_str())
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/configuration.html")]
pub struct ConfigurationPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub form: configuration::ConfigurationForm,
    pub error: FormError,
    pub saved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_urls_encode_search_and_skip_defaults() {
        assert_eq!(listing_url("/admin/users", "", 1), "/admin/users");
        assert_eq!(listing_url("/admin/users", "a b&c", 2), "/admin/users?q=a%20b%26c&page=2");
    }

    fn user(id: i64, name: &str, is_admin: bool) -> User {
        User {
            account: rbit_core::Account {
                id: AccountId(id),
                kind: rbit_core::AccountKind::User,
                name: name.parse().unwrap(),
                display_name: None,
                bio: None,
                created_at: rbit_core::Timestamp(1_792_022_400),
            },
            email: format!("{name}@example.com").parse().unwrap(),
            is_admin,
        }
    }

    fn record(user: User, suspended: bool) -> UserRecord {
        UserRecord { user, suspended_at: suspended.then_some(rbit_core::Timestamp(1)), repository_count: 0 }
    }

    fn page() -> Page {
        crate::views::test_support::page("/admin/users")
    }

    #[test]
    fn dashboard_renders_available_release_and_configuration_link() {
        let mut page = page();
        page.updates = rbit_services::updates::UpdateStatus {
            revision: 1,
            state: rbit_services::updates::UpdateState::Available,
            release: Some(rbit_services::updates::AvailableRelease {
                version: "99.0.0".into(),
                url: "https://github.com/GitRbit/rbit/releases/tag/v99.0.0".into(),
            }),
        };
        let snapshot = crate::monitoring::Snapshot { uptime: std::time::Duration::ZERO, samples: Vec::new() };
        let html = dashboard::DashboardPage {
            page,
            admin_nav: AdminSection::Dashboard.nav(),
            monitoring: monitoring::MonitoringPanel::new(&snapshot, Locale::English),
            restart_available: false,
            tiles: Vec::new(),
            system: Vec::new(),
            settings: Vec::new(),
        }
        .render()
        .unwrap();
        assert!(html.contains("A newer stable release is available."));
        assert!(html.contains("View rbit 99.0.0 release notes"));
        assert!(html.contains("https://github.com/GitRbit/rbit/releases/tag/v99.0.0"));
        assert!(html.contains("/admin/configuration"));
    }

    #[test]
    fn users_page_shows_verification_and_password_actions() {
        let admin = user(1, "root", true);
        let paged = Paged {
            items: vec![record(admin.clone(), false), record(user(2, "alice", false), false), record(user(3, "bob", false), true)],
            total: 3,
            page: 1,
            per_page: 30,
        };
        let verified = HashSet::from([AccountId(2)]);

        let html = UsersPage::new(page(), &admin, "", &paged)
            .with_email(&verified, true)
            .with_notice(Some(UsersNotice::ResetSent))
            .render()
            .unwrap();
        assert_eq!(html.matches(">Verified<").count(), 1);
        assert!(html.contains("Password reset email sent."));
        assert!(html.contains(r#"action="/admin/users/2/password-reset""#));
        assert!(!html.contains(r#"action="/admin/users/3/password-reset""#), "not offered for suspended users");
        assert!(html.contains(r#"href="/admin/users/2/password""#) && html.contains(r#"href="/admin/users/3/password""#));
        assert!(!html.contains("/admin/users/1/password"), "no self-targeted actions");

        let html = UsersPage::new(page(), &admin, "", &paged).with_email(&verified, false).render().unwrap();
        assert!(!html.contains("password-reset"), "no reset emails without email");
        assert!(html.contains(r#"href="/admin/users/2/password""#));
    }

    #[test]
    fn users_notices_round_trip() {
        for notice in [UsersNotice::ResetSent, UsersNotice::PasswordSet] {
            assert_eq!(UsersNotice::parse(notice.key()), Some(notice));
        }
        assert_eq!(UsersNotice::parse("other"), None);
        assert_eq!(UsersNotice::PasswordSet.url("/admin/users"), "/admin/users?notice=password-set");
        assert_eq!(UsersNotice::ResetSent.url("/admin/users?q=alice"), "/admin/users?q=alice&notice=reset-sent");
    }

    #[test]
    fn set_password_page_renders() {
        let admin = user(1, "root", true);
        let html = SetPasswordPage::new(page(), &admin, &record(user(2, "alice", false), false), 12, true)
            .with_error(FormError::new("The new passwords do not match."))
            .render()
            .unwrap();
        assert!(html.contains(r#"action="/admin/users/2/password""#));
        assert!(html.contains(r#"minlength="12""#));
        assert!(html.contains("The new passwords do not match."));
        assert!(html.contains("send a password reset"));

        let html = SetPasswordPage::new(page(), &admin, &record(user(2, "alice", false), false), 12, false).render().unwrap();
        assert!(!html.contains("send a password reset"));
    }

    #[test]
    fn settings_page_preselects_the_digest_hour() {
        let settings = InstanceSettings { digest_hour: 6, ..InstanceSettings::defaults(&rbit_core::Config::default()) };
        let html = SettingsPage::new(page(), SettingsForm::from(&settings), FormError::default(), false).render().unwrap();
        assert!(html.contains(r#"name="digest_hour""#));
        assert!(html.contains(r#"<option value="6" selected>06:00 UTC</option>"#), "{html}");
        assert_eq!(html.matches(" selected>").count(), 1);
        assert!(dashboard::settings_summary(&settings, Locale::English).iter().any(|row| row.value.contains("06:00 UTC")));
    }

    #[test]
    fn nav_marks_active_section() {
        let nav = AdminSection::Users.nav();
        assert_eq!(nav.len(), AdminSection::ALL.len());
        assert_eq!(nav.iter().filter(|link| link.is_active).count(), 1);
        assert_eq!(nav.iter().find(|link| link.is_active).map(|link| link.url), Some("/admin/users"));
    }
}
