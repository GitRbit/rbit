//! Organisation pages: profile, members, settings, creation. Team pages live in
//! [`crate::views::teams`] and share [`OrgHeader`].

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{BasePermission, OrgMember, OrgRole, Organization, User};
use rbit_services::orgs::OrgContext;

use crate::views::{
    components::{Avatar, FormError, RepoCard, SettingsCard},
    layout::Page,
    teams, webhooks,
};

// ------------------------------------------------------------------ shared pieces

/// The organisation's tabs; compared in `org/header.html`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgTab {
    Repositories,
    Members,
    Teams,
    Settings,
}

/// Title block and tabs at the top of every organisation page.
#[derive(Debug, Clone)]
pub struct OrgHeader {
    pub name: String,
    pub label: String,
    pub avatar: Avatar,
    pub description: Option<String>,
    /// Validated `http(s)://` URL.
    pub website: Option<String>,
    /// The website without its scheme, for display.
    pub website_label: String,
    pub repos_url: String,
    pub members_url: String,
    pub teams_url: String,
    pub settings_url: String,
    pub repo_count: Option<usize>,
    pub member_count: Option<usize>,
    /// Show the Settings tab (owners and site admins).
    pub can_manage: bool,
    /// Show the Teams tab (members and site admins).
    pub can_see_teams: bool,
    pub active: OrgTab,
}

impl OrgHeader {
    pub fn new(ctx: &OrgContext, active: OrgTab) -> Self {
        let org = &ctx.org;
        let base = format!("/{}", org.name());
        let website_label = org.website.as_deref().map(display_url).unwrap_or_default();
        Self {
            name: org.name().to_string(),
            label: org.label().to_owned(),
            avatar: Avatar::for_account(&org.account),
            description: org.description.clone(),
            website: org.website.clone(),
            website_label,
            members_url: format!("{base}/members"),
            teams_url: format!("{base}/teams"),
            settings_url: format!("{base}/settings"),
            repos_url: base,
            repo_count: None,
            member_count: None,
            can_manage: ctx.can_manage,
            can_see_teams: ctx.is_member() || ctx.can_manage,
            active,
        }
    }

    #[must_use]
    pub fn with_counts(mut self, repos: Option<usize>, members: Option<usize>) -> Self {
        self.repo_count = repos;
        self.member_count = members;
        self
    }

    pub fn is_active(&self, tab: &str) -> bool {
        let current = match self.active {
            OrgTab::Repositories => "repositories",
            OrgTab::Members => "members",
            OrgTab::Teams => "teams",
            OrgTab::Settings => "settings",
        };
        current == tab
    }
}

/// `https://acme.example/` → `acme.example`.
fn display_url(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.strip_suffix('/').unwrap_or(rest).to_owned()
}

// ------------------------------------------------------------------ profile

/// `/{org}`: the Repositories tab.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/profile.html")]
pub struct OrgProfilePage {
    pub page: Page,
    pub header: OrgHeader,
    pub repos: Vec<RepoCard>,
    /// The viewer may create repositories here (members).
    pub can_create_repo: bool,
    /// `/new?owner={org}`.
    pub new_repo_url: String,
}

// ------------------------------------------------------------------ members

/// One member in the list, with the actions the viewer may take on them.
#[derive(Debug, Clone)]
pub struct MemberRow {
    pub name: String,
    pub label: String,
    pub url: String,
    pub avatar: Avatar,
    pub role_label: &'static str,
    pub is_owner: bool,
    /// This row is the viewer.
    pub is_viewer: bool,
    pub role_url: String,
    pub remove_url: String,
    /// The role offered by the "Make …" button.
    pub other_role: &'static str,
    pub other_role_label: &'static str,
}

impl MemberRow {
    pub fn new(org: &Organization, member: &OrgMember, viewer: Option<&User>) -> Self {
        let base = format!("/{}/members/{}", org.name(), member.user.name);
        let other = if member.role.is_owner() { OrgRole::Member } else { OrgRole::Owner };
        Self {
            name: member.user.name.to_string(),
            label: member.user.label().to_owned(),
            url: format!("/{}", member.user.name),
            avatar: Avatar::for_account(&member.user),
            role_label: member.role.label(),
            is_owner: member.role.is_owner(),
            is_viewer: viewer.is_some_and(|user| user.id() == member.user.id),
            role_url: format!("{base}/role"),
            remove_url: format!("{base}/remove"),
            other_role: other.as_str(),
            other_role_label: if other.is_owner() { "Make owner" } else { "Make member" },
        }
    }
}

/// The "add member" form, as submitted.
#[derive(Debug, Clone, Default)]
pub struct AddMemberForm {
    pub username: String,
    pub owner: bool,
}

/// `/{org}/members`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/members.html")]
pub struct MembersPage {
    pub page: Page,
    pub header: OrgHeader,
    pub members: Vec<MemberRow>,
    pub error: FormError,
    pub form: AddMemberForm,
    pub can_manage: bool,
    pub is_member: bool,
    pub add_url: String,
    pub leave_url: String,
}

// ------------------------------------------------------------------ settings

/// Profile fields, as stored or as submitted.
#[derive(Debug, Clone, Default)]
pub struct OrgProfileForm {
    pub display_name: String,
    pub description: String,
    pub website: String,
}

impl OrgProfileForm {
    pub fn from_org(org: &Organization) -> Self {
        Self {
            display_name: org.account.display_name.clone().unwrap_or_default(),
            description: org.description.clone().unwrap_or_default(),
            website: org.website.clone().unwrap_or_default(),
        }
    }
}

/// `/{org}/settings`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/settings.html")]
pub struct OrgSettingsPage {
    pub page: Page,
    pub header: OrgHeader,
    pub form: OrgProfileForm,
    pub error: FormError,
    pub delete_error: FormError,
    pub repo_count: usize,
    pub save_url: String,
    pub delete_url: String,
    /// Links to settings sub-pages (teams, webhooks).
    pub cards: Vec<SettingsCard>,
    /// Where the base permission form posts.
    pub permission_url: String,
    pub base_permissions: Vec<BasePermissionOption>,
    pub permission_error: FormError,
}

/// One choice of the organisation base permission.
#[derive(Debug, Clone)]
pub struct BasePermissionOption {
    pub value: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub selected: bool,
}

impl BasePermissionOption {
    /// Every level, with `selected` preselected.
    pub fn list(selected: BasePermission) -> Vec<Self> {
        BasePermission::ALL
            .into_iter()
            .map(|base| Self { value: base.as_str(), label: base.label(), description: base.description(), selected: base == selected })
            .collect()
    }
}

/// The organisation settings sub-pages, in display order; `settings_url` is `/{org}/settings`.
pub fn org_settings_cards(settings_url: &str) -> Vec<SettingsCard> {
    let mut cards = teams::org_settings_cards(settings_url);
    cards.extend(webhooks::org_settings_cards(settings_url));
    cards
}

// ------------------------------------------------------------------ creation

/// `/organizations/new`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/new.html")]
pub struct NewOrgPage {
    pub page: Page,
    pub error: FormError,
    pub name: String,
    pub display_name: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(role: Option<OrgRole>, can_manage: bool) -> OrgContext {
        let account = rbit_core::Account {
            id: rbit_core::AccountId(9),
            kind: rbit_core::AccountKind::Organization,
            name: "acme".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: rbit_core::Timestamp(0),
        };
        OrgContext { org: Organization { account, description: None, website: None }, role, can_manage }
    }

    #[test]
    fn teams_tab_is_for_members_and_managers() {
        assert!(OrgHeader::new(&ctx(Some(OrgRole::Member), false), OrgTab::Teams).can_see_teams);
        assert!(OrgHeader::new(&ctx(None, true), OrgTab::Teams).can_see_teams, "site admins");
        assert!(!OrgHeader::new(&ctx(None, false), OrgTab::Repositories).can_see_teams);
        let header = OrgHeader::new(&ctx(Some(OrgRole::Owner), true), OrgTab::Teams);
        assert_eq!(header.teams_url, "/acme/teams");
        assert!(header.is_active("teams"));
    }

    #[test]
    fn settings_page_offers_the_base_permission() {
        let header = OrgHeader::new(&ctx(Some(OrgRole::Owner), true), OrgTab::Settings);
        let html = OrgSettingsPage {
            page: Page {
                site_name: "rbit".into(),
                title: None,
                branding: rbit_core::config::BrandingConfig::default(),
                viewer: None,
                path: "/acme/settings".into(),
                theme: crate::themes::Theme::System,
                has_unread_notifications: false,
            },
            header,
            form: OrgProfileForm::default(),
            error: FormError::default(),
            delete_error: FormError::default(),
            repo_count: 0,
            save_url: "/acme/settings".into(),
            delete_url: "/acme/settings/delete".into(),
            cards: org_settings_cards("/acme/settings"),
            permission_url: "/acme/settings/permission".into(),
            base_permissions: BasePermissionOption::list(BasePermission::Read),
            permission_error: FormError::default(),
        }
        .render()
        .unwrap();
        assert!(html.contains(r#"action="/acme/settings/permission""#));
        assert!(html.contains(r#"value="read" checked"#));
        assert!(html.contains(r#"value="none""#) && html.contains(r#"value="write""#));
        assert!(html.contains(r#"href="/acme/teams""#), "the Teams card and tab");
    }

    #[test]
    fn websites_display_without_scheme() {
        assert_eq!(display_url("https://acme.example/"), "acme.example");
        assert_eq!(display_url("http://acme.example/about"), "acme.example/about");
    }
}
