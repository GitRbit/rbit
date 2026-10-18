//! Repository collaborator pages.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! `/{owner}/{repo}/settings/collaborators` lists the people given access directly (with
//! a permission picker and a remove button each), an add form, and — for organisation
//! repositories — the teams with access, which are managed on the team pages.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::membership::{Collaborator, Permission, TeamGrant};

use crate::views::{
    components::{Avatar, FormError, SettingsCard},
    layout::Page,
    repo::RepoHeader,
};

/// Cards this workstream adds to the repository settings page (e.g. "Collaborators").
pub fn repo_settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    vec![SettingsCard {
        url: format!("{}/collaborators", header.settings_url),
        icon: "users",
        title: "Collaborators",
        description: "People and teams with access to this repository, and what each may do.",
    }]
}

/// The repository sidebar's note for a collaborator who does not administer the
/// repository, with a button to leave it (administrators leave on the collaborators page).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaboratorAccess {
    pub permission_label: &'static str,
    /// `POST` target: the viewer's own row of the collaborators page.
    pub leave_url: String,
}

impl CollaboratorAccess {
    /// `home_url` is `/{owner}/{repo}`; `username` is the viewer's name.
    pub fn new(permission: Permission, home_url: &str, username: &str) -> Self {
        Self { permission_label: permission.label(), leave_url: format!("{home_url}/settings/collaborators/{username}/remove") }
    }
}

/// One `<option>` of a permission picker.
#[derive(Debug, Clone)]
pub struct PermissionOption {
    pub value: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub selected: bool,
}

impl PermissionOption {
    /// Every permission level, with `selected` preselected.
    pub fn list(selected: Permission) -> Vec<Self> {
        Permission::ALL
            .into_iter()
            .map(|permission| Self {
                value: permission.as_str(),
                label: permission.label(),
                description: permission.description(),
                selected: permission == selected,
            })
            .collect()
    }
}

/// One collaborator in the list.
#[derive(Debug, Clone)]
pub struct CollaboratorRow {
    pub name: String,
    pub label: String,
    pub url: String,
    pub avatar: Avatar,
    pub permission_label: &'static str,
    pub options: Vec<PermissionOption>,
    pub permission_url: String,
    pub remove_url: String,
    /// This row is the viewer (offers "Leave" instead of "Remove").
    pub is_viewer: bool,
    pub added: String,
    pub added_iso: String,
}

impl CollaboratorRow {
    /// `base_url` is `/{owner}/{repo}/settings/collaborators`.
    pub fn new(collaborator: &Collaborator, base_url: &str, viewer: &str) -> Self {
        let user = &collaborator.user;
        let row_url = format!("{base_url}/{}", user.name);
        Self {
            name: user.name.to_string(),
            label: user.label().to_owned(),
            url: format!("/{}", user.name),
            avatar: Avatar::for_account(user),
            permission_label: collaborator.permission.label(),
            options: PermissionOption::list(collaborator.permission),
            permission_url: format!("{row_url}/permission"),
            remove_url: format!("{row_url}/remove"),
            is_viewer: user.name.as_str().eq_ignore_ascii_case(viewer),
            added: format!("Added {}", collaborator.added_at.date()),
            added_iso: collaborator.added_at.iso8601(),
        }
    }
}

/// A team with access, linking to the team's page.
#[derive(Debug, Clone)]
pub struct TeamGrantRow {
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub permission_label: &'static str,
}

impl TeamGrantRow {
    pub fn new(grant: &TeamGrant, org: &str) -> Self {
        Self {
            name: grant.team.name.to_string(),
            url: format!("/{org}/teams/{}", grant.team.name),
            description: grant.team.description.clone(),
            permission_label: grant.permission.label(),
        }
    }
}

/// The add form, blank or as submitted.
#[derive(Debug, Clone)]
pub struct AddCollaboratorForm {
    pub username: String,
    pub options: Vec<PermissionOption>,
}

impl AddCollaboratorForm {
    pub fn new(username: &str, permission: Permission) -> Self {
        Self { username: username.to_owned(), options: PermissionOption::list(permission) }
    }
}

impl Default for AddCollaboratorForm {
    fn default() -> Self {
        Self::new("", Permission::Write)
    }
}

/// `/{owner}/{repo}/settings/collaborators`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "access/collaborators.html")]
pub struct CollaboratorsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub full_name: String,
    pub settings_url: String,
    /// Where the add form posts.
    pub add_url: String,
    pub collaborators: Vec<CollaboratorRow>,
    pub teams: Vec<TeamGrantRow>,
    /// The repository belongs to an organisation (teams can be granted access).
    pub is_org: bool,
    /// `/{org}/teams`, when `is_org`.
    pub teams_url: String,
    pub form: AddCollaboratorForm,
    pub error: FormError,
}

impl CollaboratorsPage {
    /// `viewer` is the signed-in user's name.
    pub fn new(page: Page, header: RepoHeader, collaborators: &[Collaborator], teams: &[TeamGrant], is_org: bool, viewer: &str) -> Self {
        let full_name = format!("{}/{}", header.owner, header.name);
        let add_url = format!("{}/collaborators", header.settings_url);
        Self {
            page: page.titled(format!("Collaborators · {full_name}")),
            collaborators: collaborators.iter().map(|c| CollaboratorRow::new(c, &add_url, viewer)).collect(),
            teams: teams.iter().map(|grant| TeamGrantRow::new(grant, &header.owner)).collect(),
            teams_url: format!("/{}/teams", header.owner),
            settings_url: header.settings_url.clone(),
            is_org,
            full_name,
            add_url,
            header,
            form: AddCollaboratorForm::default(),
            error: FormError::default(),
        }
    }

    /// Re-renders with `error` above the add form, which keeps what was submitted.
    #[must_use]
    pub fn rejected(mut self, form: AddCollaboratorForm, error: FormError) -> Self {
        self.form = form;
        self.error = error;
        self
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{Account, AccountId, AccountKind, Timestamp, membership::Team, membership::TeamId};

    use super::*;

    fn account(id: i64, name: &str) -> Account {
        Account {
            id: AccountId(id),
            kind: AccountKind::User,
            name: name.parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        }
    }

    #[test]
    fn collaborators_leave_through_their_own_row() {
        let access = CollaboratorAccess::new(Permission::Write, "/acme/rocket", "will");
        assert_eq!(access.permission_label, "Write");
        assert_eq!(access.leave_url, "/acme/rocket/settings/collaborators/will/remove");
    }

    fn header() -> RepoHeader {
        RepoHeader {
            owner: "acme".to_owned(),
            name: "rocket".to_owned(),
            home_url: "/acme/rocket".to_owned(),
            settings_url: "/acme/rocket/settings".to_owned(),
            can_admin: true,
            active_tab: "settings",
            ..RepoHeader::default()
        }
    }

    fn page() -> Page {
        Page {
            site_name: "rbit".to_owned(),
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/acme/rocket/settings/collaborators".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    #[test]
    fn settings_card_links_to_the_page() {
        let cards = repo_settings_cards(&header());
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].url, "/acme/rocket/settings/collaborators");
        assert_eq!(cards[0].icon, "users");
    }

    #[test]
    fn permission_options_mark_the_selection() {
        let options = PermissionOption::list(Permission::Write);
        let values: Vec<_> = options.iter().map(|o| (o.value, o.selected)).collect();
        assert_eq!(values, [("read", false), ("write", true), ("admin", false)]);
    }

    #[test]
    fn page_lists_collaborators_and_teams() {
        let collaborators = [
            Collaborator { user: account(2, "bob"), permission: Permission::Read, added_at: Timestamp(1_792_022_400) },
            Collaborator { user: account(3, "carol"), permission: Permission::Admin, added_at: Timestamp(1_792_022_400) },
        ];
        let team = Team { id: TeamId(1), org_id: AccountId(9), name: "core".parse().unwrap(), description: None, created_at: Timestamp(0) };
        let teams = [TeamGrant { team, permission: Permission::Write }];
        let html = CollaboratorsPage::new(page(), header(), &collaborators, &teams, true, "Carol")
            .rejected(AddCollaboratorForm::new("dave", Permission::Admin), FormError::new("No such user."))
            .render()
            .unwrap();

        assert!(html.contains(r#"action="/acme/rocket/settings/collaborators""#));
        assert!(html.contains(r#"action="/acme/rocket/settings/collaborators/bob/permission""#));
        assert!(html.contains(r#"action="/acme/rocket/settings/collaborators/bob/remove""#));
        assert!(html.contains(r#"aria-label="Remove bob""#));
        assert!(html.contains(">Leave<"), "the viewer's own row offers leaving");
        assert!(html.contains(r#"href="/acme/teams/core""#));
        assert!(html.contains(r#"value="dave""#));
        assert!(html.contains(r#"<option value="admin" selected>"#));
        assert!(html.contains("No such user."));
        assert!(html.contains("Added Oct 15, 2026"));
        assert!(!html.contains("style="), "no inline styles under the CSP");
    }

    #[test]
    fn personal_repositories_mention_no_teams() {
        let html = CollaboratorsPage::new(page(), header(), &[], &[], false, "alice").render().unwrap();
        assert!(html.contains("No collaborators"));
        assert!(!html.contains("/acme/teams"));
    }
}
