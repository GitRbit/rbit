//! The administration dashboard: counts, system details and instance settings.
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{InstanceSettings, OrgCreationPolicy, RegistrationMode, Visibility};
use rbit_localization::{
    Locale, Localized,
    settings::admin::{AdminMessage, AdminText, DashboardMessage},
};
use rbit_services::admin::Dashboard;

use super::{AdminLink, AdminSection, monitoring};
use crate::views::{
    layout::Page,
    notification_email::hour_label,
    when::{bytes, count, uptime},
};

/// A number on the dashboard.
#[derive(Debug, Clone)]
pub struct StatTile {
    pub label: DashboardMessage,
    pub value: String,
    pub detail: Localized,
    pub url: &'static str,
}

/// A label/value line in a definition list.
#[derive(Debug, Clone)]
pub struct InfoRow {
    pub label: Localized,
    pub value: String,
    /// Render the value in the monospace face (versions, paths).
    pub mono: bool,
}

impl InfoRow {
    fn text(label: Localized, value: impl Into<String>) -> Self {
        Self { label, value: value.into(), mono: false }
    }

    fn mono(label: Localized, value: impl Into<String>) -> Self {
        Self { label, value: value.into(), mono: true }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/dashboard.html")]
pub struct DashboardPage {
    pub monitoring: monitoring::MonitoringPanel,
    pub restart_available: bool,
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub tiles: Vec<StatTile>,
    pub system: Vec<InfoRow>,
    pub settings: Vec<InfoRow>,
}

impl DashboardPage {
    pub fn new(page: Page, dashboard: &Dashboard, snapshot: &crate::monitoring::Snapshot, restart_available: bool) -> Self {
        let locale = page.locale;
        let counts = &dashboard.counts;
        let system = &dashboard.system;
        let (admins, suspended) = (count(locale, counts.admins), count(locale, counts.suspended));
        let users = if counts.admins == 1 { AdminText::UsersDetailOne(&suspended) } else { AdminText::UsersDetail(&admins, &suspended) };
        let repositories =
            AdminText::RepositoriesDetail(&count(locale, counts.public_repositories), &count(locale, counts.private_repositories));
        let tiles = vec![
            StatTile {
                label: DashboardMessage::Users,
                value: count(locale, counts.users),
                detail: users.text(&locale),
                url: AdminSection::Users.url(),
            },
            StatTile {
                label: DashboardMessage::Organizations,
                value: count(locale, counts.organizations),
                detail: DashboardMessage::SharedNamespaces.text(&locale),
                url: AdminSection::Organizations.url(),
            },
            StatTile {
                label: DashboardMessage::Repositories,
                value: count(locale, counts.public_repositories + counts.private_repositories),
                detail: repositories.text(&locale),
                url: AdminSection::Repositories.url(),
            },
            StatTile {
                label: DashboardMessage::AccessTokens,
                value: count(locale, counts.access_tokens),
                detail: DashboardMessage::AcrossAllUsers.text(&locale),
                url: AdminSection::Users.url(),
            },
        ];
        let name = |name: &str| Localized::english_only(name.to_owned(), locale);
        let unavailable = || AdminMessage::Unavailable.text(&locale).to_string();
        let system_rows = vec![
            InfoRow::mono(name("rbit"), system.rbit_version),
            InfoRow::mono(name("SQLite"), system.sqlite_version.clone()),
            InfoRow::mono(name("Git"), system.git_version.clone().unwrap_or_else(unavailable)),
            InfoRow::text(DashboardMessage::Uptime.text(&locale), uptime(locale, system.uptime).to_string()),
            InfoRow::mono(DashboardMessage::DataDirectory.text(&locale), system.data_dir.display().to_string()),
            InfoRow::text(
                DashboardMessage::DiskUsage.text(&locale),
                system.data_dir_bytes.map_or_else(unavailable, |size| bytes(locale, size)),
            ),
        ];
        let title = AdminMessage::SiteAdministration.text(&locale);
        Self {
            page: page.titled(title.as_str()),
            admin_nav: AdminSection::Dashboard.nav(),
            monitoring: monitoring::MonitoringPanel::new(snapshot, locale),
            restart_available,
            tiles,
            system: system_rows,
            settings: settings_summary(&dashboard.settings, locale),
        }
    }
}

pub(super) fn settings_summary(settings: &InstanceSettings, locale: Locale) -> Vec<InfoRow> {
    let row = |label: DashboardMessage, value: DashboardMessage| InfoRow::text(label.text(&locale), value.text(&locale).to_string());
    vec![
        InfoRow::text(DashboardMessage::SiteName.text(&locale), settings.site_name.clone()),
        row(
            DashboardMessage::Registration,
            match settings.registration {
                RegistrationMode::Open => DashboardMessage::RegistrationOpen,
                RegistrationMode::Closed => DashboardMessage::RegistrationClosed,
            },
        ),
        row(
            DashboardMessage::OrgPolicy,
            match settings.org_creation {
                OrgCreationPolicy::AnyUser => DashboardMessage::AnyUserCreates,
                OrgCreationPolicy::AdminsOnly => DashboardMessage::AdminsCreate,
            },
        ),
        row(
            DashboardMessage::NewRepositories,
            match settings.default_visibility {
                Visibility::Public => DashboardMessage::PublicByDefault,
                Visibility::Private => DashboardMessage::PrivateByDefault,
            },
        ),
        InfoRow::text(
            DashboardMessage::EmailDigests.text(&locale),
            AdminText::DigestsAt(&hour_label(settings.digest_hour)).text(&locale).to_string(),
        ),
    ]
}
