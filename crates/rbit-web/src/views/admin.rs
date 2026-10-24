//! Administration panel pages (`/admin/…`) and their view models.

pub mod configuration;

use std::{collections::HashSet, time::Duration};

use crate::views::notification_email::hour_label;
use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{AccountId, InstanceSettings, OrgCreationPolicy, OwnedRepository, RegistrationMode, User, Visibility};
use rbit_services::admin::{Dashboard, OrgRecord, Paged, UserRecord};

use crate::views::{
    components::{Avatar, FormError},
    layout::Page,
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
}

impl AdminSection {
    const ALL: [Self; 7] =
        [Self::Dashboard, Self::Users, Self::Organizations, Self::Repositories, Self::Settings, Self::Configuration, Self::AuditLog];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Users => "Users",
            Self::Organizations => "Organizations",
            Self::Repositories => "Repositories",
            Self::Settings => "Settings",
            Self::AuditLog => "Audit log",
            Self::Configuration => "Configuration",
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
        }
    }

    /// The navigation with `self` marked as the current page.
    pub fn nav(self) -> Vec<AdminLink> {
        Self::ALL.iter().map(|&section| AdminLink { label: section.label(), url: section.url(), is_active: section == self }).collect()
    }
}

#[derive(Debug, Clone)]
pub struct AdminLink {
    pub label: &'static str,
    pub url: &'static str,
    pub is_active: bool,
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
    /// e.g. `1–30 of 112`, or `No results`.
    pub summary: String,
    pub previous_url: Option<String>,
    pub next_url: Option<String>,
}

impl Listing {
    pub fn new<T>(section: AdminSection, search: &str, paged: &Paged<T>) -> Self {
        let base = section.url();
        let summary = if paged.total == 0 {
            "No results".to_owned()
        } else {
            format!("{}–{} of {}", paged.first_index(), paged.last_index(), paged.total)
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

    pub fn has_pages(&self) -> bool {
        self.previous_url.is_some() || self.next_url.is_some()
    }
}

// ------------------------------------------------------------------ dashboard

/// A number on the dashboard.
#[derive(Debug, Clone)]
pub struct StatTile {
    pub label: &'static str,
    pub value: String,
    pub detail: String,
    pub url: &'static str,
}

/// A label/value line in a definition list.
#[derive(Debug, Clone)]
pub struct InfoRow {
    pub label: &'static str,
    pub value: String,
    /// Render the value in the monospace face (versions, paths).
    pub mono: bool,
}

impl InfoRow {
    fn text(label: &'static str, value: impl Into<String>) -> Self {
        Self { label, value: value.into(), mono: false }
    }

    fn mono(label: &'static str, value: impl Into<String>) -> Self {
        Self { label, value: value.into(), mono: true }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/dashboard.html")]
pub struct DashboardPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub tiles: Vec<StatTile>,
    pub system: Vec<InfoRow>,
    pub settings: Vec<InfoRow>,
}

impl DashboardPage {
    pub fn new(page: Page, dashboard: &Dashboard) -> Self {
        let counts = &dashboard.counts;
        let system = &dashboard.system;
        let tiles = vec![
            StatTile {
                label: "Users",
                value: count(counts.users),
                detail: format!("{} admin{} · {} suspended", count(counts.admins), plural(counts.admins), count(counts.suspended)),
                url: AdminSection::Users.url(),
            },
            StatTile {
                label: "Organizations",
                value: count(counts.organizations),
                detail: "Shared namespaces".to_owned(),
                url: AdminSection::Organizations.url(),
            },
            StatTile {
                label: "Repositories",
                value: count(counts.public_repositories + counts.private_repositories),
                detail: format!("{} public · {} private", count(counts.public_repositories), count(counts.private_repositories)),
                url: AdminSection::Repositories.url(),
            },
            StatTile {
                label: "Access tokens",
                value: count(counts.access_tokens),
                detail: "Across all users".to_owned(),
                url: AdminSection::Users.url(),
            },
        ];
        let system_rows = vec![
            InfoRow::mono("rbit", system.rbit_version),
            InfoRow::mono("SQLite", system.sqlite_version.clone()),
            InfoRow::mono("Git", system.git_version.clone().unwrap_or_else(|| "unavailable".to_owned())),
            InfoRow::text("Uptime", uptime(system.uptime)),
            InfoRow::mono("Data directory", system.data_dir.display().to_string()),
            InfoRow::text("Disk usage", system.data_dir_bytes.map_or_else(|| "unavailable".to_owned(), bytes)),
        ];
        Self {
            page: page.titled("Site administration"),
            admin_nav: AdminSection::Dashboard.nav(),
            tiles,
            system: system_rows,
            settings: settings_summary(&dashboard.settings),
        }
    }
}

fn settings_summary(settings: &InstanceSettings) -> Vec<InfoRow> {
    vec![
        InfoRow::text("Site name", settings.site_name.clone()),
        InfoRow::text(
            "Registration",
            match settings.registration {
                RegistrationMode::Open => "Open to everyone",
                RegistrationMode::Closed => "Closed (administrators create accounts)",
            },
        ),
        InfoRow::text(
            "Organizations",
            match settings.org_creation {
                OrgCreationPolicy::AnyUser => "Any user can create",
                OrgCreationPolicy::AdminsOnly => "Only administrators can create",
            },
        ),
        InfoRow::text(
            "New repositories",
            match settings.default_visibility {
                Visibility::Public => "Public by default",
                Visibility::Private => "Private by default",
            },
        ),
        InfoRow::text("Email digests", format!("Sent at {} unless users choose", hour_label(settings.digest_hour))),
    ]
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
    pub joined: String,
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
    pub fn new(record: &UserRecord, viewer: AccountId) -> Self {
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
            repositories: count(record.repository_count),
            joined: user.account.created_at.date(),
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

    pub const fn message(self) -> &'static str {
        match self {
            Self::ResetSent => "Password reset email sent.",
            Self::PasswordSet => "Password updated.",
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
        Self {
            page: page.titled("Users · Administration"),
            admin_nav: AdminSection::Users.nav(),
            error: FormError::default(),
            notice: None,
            listing: Listing::new(AdminSection::Users, search, paged),
            users: paged.items.iter().map(|record| UserRow::new(record, viewer.id())).collect(),
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
        let user = UserRow::new(record, viewer.id());
        Self {
            page: page.titled(format!("Set password for {} · Administration", user.name)),
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
        let user = UserRow::new(record, viewer.id());
        Self { page: page.titled(format!("Delete {} · Administration", user.name)), admin_nav: AdminSection::Users.nav(), error, user }
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
    pub created: String,
    pub created_iso: String,
}

impl From<&OrgRecord> for OrgRow {
    fn from(record: &OrgRecord) -> Self {
        let account = &record.account;
        Self {
            name: account.name.to_string(),
            label: account.label().to_owned(),
            has_display_name: account.display_name.as_deref().is_some_and(|name| !name.is_empty()),
            url: format!("/{}", account.name),
            avatar: Avatar::for_account(account),
            members: count(record.member_count),
            repositories: count(record.repository_count),
            created: account.created_at.date(),
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
        Self {
            page: page.titled("Organizations · Administration"),
            admin_nav: AdminSection::Organizations.nav(),
            listing: Listing::new(AdminSection::Organizations, search, paged),
            orgs: paged.items.iter().map(OrgRow::from).collect(),
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
    pub updated: String,
    pub updated_iso: String,
    pub delete_url: String,
}

impl From<&OwnedRepository> for RepoRow {
    fn from(item: &OwnedRepository) -> Self {
        Self {
            owner: item.owner.name.to_string(),
            name: item.repo.name.to_string(),
            full_name: item.full_name(),
            url: format!("/{}/{}", item.owner.name, item.repo.name),
            owner_url: format!("/{}", item.owner.name),
            description: item.repo.description.clone().filter(|d| !d.is_empty()),
            is_private: !item.repo.visibility.is_public(),
            updated: item.repo.updated_at.relative(),
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
        Self {
            page: page.titled("Repositories · Administration"),
            admin_nav: AdminSection::Repositories.nav(),
            listing: Listing::new(AdminSection::Repositories, search, paged),
            repos: paged.items.iter().map(RepoRow::from).collect(),
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
        let repo = RepoRow::from(repo);
        Self {
            page: page.titled(format!("Delete {} · Administration", repo.full_name)),
            admin_nav: AdminSection::Repositories.nav(),
            error,
            repo,
        }
    }
}

// ------------------------------------------------------------------ instance settings

/// The settings form's values (current settings, or as submitted when re-rendered).
#[derive(Debug, Clone)]
pub struct SettingsForm {
    pub site_name: String,
    pub branding: rbit_core::config::BrandingConfig,
    pub registration_open: bool,
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
            page: page.titled("Settings · Administration"),
            admin_nav: AdminSection::Settings.nav(),
            error,
            form,
            saved,
            site_name_max_len: rbit_services::instance::SITE_NAME_MAX_LEN,
        }
    }
}

// ------------------------------------------------------------------ formatting

/// `1234567` → `1,234,567`.
pub fn count(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0 {
        grouped.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

const fn plural(value: i64) -> &'static str {
    if value == 1 { "" } else { "s" }
}

/// Binary units with one decimal: `0 B`, `512 B`, `1.5 KiB`, `3.2 GiB`.
pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["KiB", "MiB", "GiB", "TiB", "PiB"];
    if value < 1024 {
        return format!("{value} B");
    }
    let mut unit = 0;
    let mut scaled = value;
    while scaled >= 1024 * 1024 && unit < UNITS.len() - 1 {
        scaled /= 1024;
        unit += 1;
    }
    // `scaled` is now in [1024, 1024²): divide once more with one decimal of precision.
    let tenths = scaled * 10 / 1024;
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

/// The two most significant units: `45 seconds`, `12 minutes`, `3 hours 5 minutes`, `2 days 4 hours`.
pub fn uptime(duration: Duration) -> String {
    const UNITS: [(u64, &str); 4] = [(86_400, "day"), (3_600, "hour"), (60, "minute"), (1, "second")];
    let mut remaining = duration.as_secs();
    let mut parts = Vec::new();
    for (size, name) in UNITS {
        let amount = remaining / size;
        if amount > 0 || (parts.is_empty() && size == 1) {
            parts.push(format!("{amount} {name}{}", if amount == 1 { "" } else { "s" }));
            remaining %= size;
        }
        if parts.len() == 2 || (!parts.is_empty() && amount == 0) {
            break;
        }
    }
    parts.join(" ")
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
    fn formats_counts() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1_000), "1,000");
        assert_eq!(count(1_234_567), "1,234,567");
        assert_eq!(count(-1_234), "-1,234");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(5 * 1024 * 1024 + 300 * 1024), "5.2 MiB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }

    #[test]
    fn formats_uptime() {
        assert_eq!(uptime(Duration::ZERO), "0 seconds");
        assert_eq!(uptime(Duration::from_secs(45)), "45 seconds");
        assert_eq!(uptime(Duration::from_secs(61)), "1 minute 1 second");
        assert_eq!(uptime(Duration::from_secs(3 * 3600 + 5 * 60 + 7)), "3 hours 5 minutes");
        assert_eq!(uptime(Duration::from_secs(2 * 86_400 + 30)), "2 days");
        assert_eq!(uptime(Duration::from_hours(28)), "1 day 4 hours");
    }

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
        Page {
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".to_owned(),
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/admin/users".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
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
        let html =
            DashboardPage { page, admin_nav: AdminSection::Dashboard.nav(), tiles: Vec::new(), system: Vec::new(), settings: Vec::new() }
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
        assert!(settings_summary(&settings).iter().any(|row| row.value.contains("06:00 UTC")));
    }

    #[test]
    fn nav_marks_active_section() {
        let nav = AdminSection::Users.nav();
        assert_eq!(nav.len(), AdminSection::ALL.len());
        assert_eq!(nav.iter().filter(|link| link.is_active).count(), 1);
        assert_eq!(nav.iter().find(|link| link.is_active).map(|link| link.url), Some("/admin/users"));
    }
}
