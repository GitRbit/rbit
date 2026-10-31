//! Organisation team pages.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! `/{org}/teams` lists the teams, `/{org}/teams/new` creates one, `/{org}/teams/{team}`
//! shows its members and repositories (with forms for owners), and
//! `/{org}/teams/{team}/settings` renames or deletes it. Every page shares the
//! organisation header with the Teams tab selected.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Account,
    membership::{Permission, TeamSummary},
    repository::OwnedRepository,
};
use rbit_services::teams::TeamContext;

use crate::views::{
    collaborators::PermissionOption,
    components::{Avatar, FormError, SettingsCard},
    layout::Page,
    org::OrgHeader,
};

/// Cards this workstream adds to the organisation settings page; `settings_url` is
/// `/{org}/settings`.
pub fn org_settings_cards(settings_url: &str) -> Vec<SettingsCard> {
    let org_url = settings_url.strip_suffix("/settings").unwrap_or(settings_url);
    vec![SettingsCard {
        url: format!("{org_url}/teams"),
        icon: "users",
        title: "Teams",
        description: "Group members and give each group its own access to repositories.",
    }]
}

/// `/{org}/teams/{team}`.
fn team_url(org: &str, team: &str) -> String {
    format!("/{org}/teams/{team}")
}

// ------------------------------------------------------------------ list

/// One team in the list.
#[derive(Debug, Clone)]
pub struct TeamRow {
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    /// e.g. `3 members · 2 repositories`.
    pub counts: String,
}

impl TeamRow {
    pub fn new(summary: &TeamSummary, org: &str) -> Self {
        let plural = |n: i64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        Self {
            name: summary.team.name.to_string(),
            url: team_url(org, summary.team.name.as_str()),
            description: summary.team.description.clone(),
            counts: format!(
                "{} · {}",
                plural(summary.member_count, "member", "members"),
                plural(summary.repo_count, "repository", "repositories")
            ),
        }
    }
}

/// `/{org}/teams`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/teams.html")]
pub struct TeamsPage {
    pub page: Page,
    pub header: OrgHeader,
    pub teams: Vec<TeamRow>,
    pub can_manage: bool,
    pub new_url: String,
}

// ------------------------------------------------------------------ create and settings

/// Name and description, blank, stored or as submitted.
#[derive(Debug, Clone, Default)]
pub struct TeamForm {
    pub name: String,
    pub description: String,
}

/// `/{org}/teams/new`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/team_new.html")]
pub struct NewTeamPage {
    pub page: Page,
    pub header: OrgHeader,
    pub teams_url: String,
    pub form: TeamForm,
    pub error: FormError,
}

/// `/{org}/teams/{team}/settings`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/team_settings.html")]
pub struct TeamSettingsPage {
    pub page: Page,
    pub header: OrgHeader,
    pub team_name: String,
    pub team_url: String,
    pub save_url: String,
    pub delete_url: String,
    pub form: TeamForm,
    pub error: FormError,
}

impl TeamSettingsPage {
    pub fn new(page: Page, header: OrgHeader, team: &TeamContext) -> Self {
        let url = team_url(&header.name, team.team.name.as_str());
        Self {
            page: page.titled(format!("Settings · {}", team.full_name())),
            team_name: team.team.name.to_string(),
            save_url: format!("{url}/settings"),
            delete_url: format!("{url}/settings/delete"),
            team_url: url,
            form: TeamForm { name: team.team.name.to_string(), description: team.team.description.clone().unwrap_or_default() },
            error: FormError::default(),
            header,
        }
    }
}

// ------------------------------------------------------------------ team page

/// A member of the team.
#[derive(Debug, Clone)]
pub struct TeamMemberRow {
    pub name: String,
    pub label: String,
    pub url: String,
    pub avatar: Avatar,
    pub remove_url: String,
    pub is_viewer: bool,
}

/// A repository the team was granted.
#[derive(Debug, Clone)]
pub struct TeamRepoRow {
    pub name: String,
    pub full_name: String,
    pub url: String,
    pub is_private: bool,
    pub permission_label: &'static str,
    pub options: Vec<PermissionOption>,
    pub remove_url: String,
}

/// The "add a repository" form, blank or as submitted.
#[derive(Debug, Clone)]
pub struct TeamRepoForm {
    pub name: String,
    pub options: Vec<PermissionOption>,
}

impl TeamRepoForm {
    pub fn new(name: &str, permission: Permission) -> Self {
        Self { name: name.to_owned(), options: PermissionOption::list(permission) }
    }
}

impl Default for TeamRepoForm {
    fn default() -> Self {
        Self::new("", Permission::Write)
    }
}

/// Which form on the team page a validation error belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamPageForm {
    Members,
    Repositories,
}

/// `/{org}/teams/{team}`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "org/team.html")]
pub struct TeamPage {
    pub page: Page,
    pub header: OrgHeader,
    pub team_name: String,
    pub description: Option<String>,
    pub teams_url: String,
    pub settings_url: String,
    pub can_manage: bool,
    pub is_member: bool,
    pub members: Vec<TeamMemberRow>,
    pub repositories: Vec<TeamRepoRow>,
    pub members_url: String,
    pub repositories_url: String,
    pub member_username: String,
    pub repo_form: TeamRepoForm,
    pub member_error: FormError,
    pub repo_error: FormError,
}

impl TeamPage {
    /// `viewer` is the signed-in user's name.
    pub fn new(
        page: Page,
        header: OrgHeader,
        team: &TeamContext,
        members: &[Account],
        repositories: &[(OwnedRepository, Permission)],
        viewer: &str,
    ) -> Self {
        let url = team_url(&header.name, team.team.name.as_str());
        let members_url = format!("{url}/members");
        let repositories_url = format!("{url}/repositories");
        Self {
            page: page.titled(team.full_name()),
            team_name: team.team.name.to_string(),
            description: team.team.description.clone(),
            teams_url: header.teams_url.clone(),
            settings_url: format!("{url}/settings"),
            can_manage: team.can_manage,
            is_member: team.is_member,
            members: members
                .iter()
                .map(|account| TeamMemberRow {
                    name: account.name.to_string(),
                    label: account.label().to_owned(),
                    url: format!("/{}", account.name),
                    avatar: Avatar::for_account(account),
                    remove_url: format!("{members_url}/{}/remove", account.name),
                    is_viewer: account.name.as_str().eq_ignore_ascii_case(viewer),
                })
                .collect(),
            repositories: repositories
                .iter()
                .map(|(repo, permission)| TeamRepoRow {
                    name: repo.repo.name.to_string(),
                    full_name: repo.full_name(),
                    url: format!("/{}", repo.full_name()),
                    is_private: !repo.repo.visibility.is_public(),
                    permission_label: permission.label(),
                    options: PermissionOption::list(*permission),
                    remove_url: format!("{repositories_url}/{}/remove", repo.repo.name),
                })
                .collect(),
            members_url,
            repositories_url,
            member_username: String::new(),
            repo_form: TeamRepoForm::default(),
            member_error: FormError::default(),
            repo_error: FormError::default(),
            header,
        }
    }

    /// Re-renders with `error` above `form`; `submitted` is the username or repository
    /// name that was entered, `permission` the permission chosen (repositories only).
    #[must_use]
    pub fn rejected(mut self, form: TeamPageForm, error: FormError, submitted: &str, permission: Permission) -> Self {
        match form {
            TeamPageForm::Members => {
                submitted.clone_into(&mut self.member_username);
                self.member_error = error;
            }
            TeamPageForm::Repositories => {
                self.repo_form = TeamRepoForm::new(submitted, permission);
                self.repo_error = error;
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        AccountId, AccountKind, OrgRole, Organization, RepoId, Repository, Timestamp, Visibility,
        membership::{Team, TeamId},
    };
    use rbit_services::orgs::OrgContext;

    use super::*;

    fn account(id: i64, name: &str, kind: AccountKind) -> Account {
        Account { id: AccountId(id), kind, name: name.parse().unwrap(), display_name: None, bio: None, created_at: Timestamp(0) }
    }

    fn org() -> Organization {
        Organization { account: account(9, "acme", AccountKind::Organization), description: None, website: None }
    }

    fn header(can_manage: bool) -> OrgHeader {
        let ctx = OrgContext { org: org(), role: Some(if can_manage { OrgRole::Owner } else { OrgRole::Member }), can_manage };
        OrgHeader::new(&ctx, crate::views::org::OrgTab::Teams)
    }

    fn team(can_manage: bool) -> TeamContext {
        let team = Team {
            id: TeamId(1),
            org_id: AccountId(9),
            name: "core".parse().unwrap(),
            description: Some("Core".into()),
            created_at: Timestamp(0),
        };
        TeamContext { org: org(), team, can_manage, is_member: true }
    }

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
            path: "/acme/teams/core".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    fn rocket() -> OwnedRepository {
        OwnedRepository {
            owner: account(9, "acme", AccountKind::Organization),
            repo: Repository {
                id: RepoId(4),
                owner_id: AccountId(9),
                name: "rocket".parse().unwrap(),
                description: None,
                visibility: Visibility::Private,
                default_branch: "main".into(),
                created_at: Timestamp(0),
                updated_at: Timestamp(0),
                pushed_at: None,
                archived_at: None,
            },
        }
    }

    #[test]
    fn settings_card_links_to_the_teams_page() {
        let cards = org_settings_cards("/acme/settings");
        assert_eq!(cards[0].url, "/acme/teams");
    }

    #[test]
    fn rows_count_members_and_repositories() {
        let summary = TeamSummary { team: team(true).team, member_count: 1, repo_count: 2 };
        let row = TeamRow::new(&summary, "acme");
        assert_eq!(row.url, "/acme/teams/core");
        assert_eq!(row.counts, "1 member · 2 repositories");
    }

    #[test]
    fn list_offers_creation_to_managers_only() {
        let summary = TeamSummary { team: team(true).team, member_count: 0, repo_count: 0 };
        let render = |can_manage| {
            TeamsPage {
                page: page(),
                header: header(can_manage),
                teams: vec![TeamRow::new(&summary, "acme")],
                can_manage,
                new_url: "/acme/teams/new".into(),
            }
            .render()
            .unwrap()
        };
        let owner = render(true);
        assert!(owner.contains(r#"href="/acme/teams/new""#));
        assert!(owner.contains(r#"href="/acme/teams" aria-current="page""#), "the Teams tab is selected");
        assert!(!render(false).contains(r#"href="/acme/teams/new""#));
    }

    #[test]
    fn team_page_shows_forms_to_managers() {
        let members = [account(2, "bob", AccountKind::User)];
        let repos = [(rocket(), Permission::Write)];
        let html = TeamPage::new(page(), header(true), &team(true), &members, &repos, "bob")
            .rejected(TeamPageForm::Repositories, FormError::new("No such repository."), "missing", Permission::Admin)
            .render()
            .unwrap();
        assert!(html.contains(r#"action="/acme/teams/core/members""#));
        assert!(html.contains(r#"action="/acme/teams/core/repositories""#));
        assert!(html.contains(r#"action="/acme/teams/core/repositories/rocket/remove""#));
        assert!(html.contains(r#"action="/acme/teams/core/members/bob/remove""#));
        assert!(html.contains(r#"value="missing""#));
        assert!(html.contains("No such repository."));
        assert!(html.contains(r#"href="/acme/teams/core/settings""#));
        assert!(!html.contains("style="));

        let member_view = TeamPage::new(page(), header(false), &team(false), &members, &repos, "bob").render().unwrap();
        assert!(!member_view.contains(r#"action="/acme/teams/core/repositories""#), "members see no management forms");
        assert!(member_view.contains(">Leave<"), "but may leave");
        assert!(member_view.contains("Write"));
    }

    #[test]
    fn settings_and_new_pages_render() {
        let html = TeamSettingsPage::new(page(), header(true), &team(true)).render().unwrap();
        assert!(html.contains(r#"action="/acme/teams/core/settings""#));
        assert!(html.contains(r#"action="/acme/teams/core/settings/delete""#));
        assert!(html.contains(r#"value="core""#));
        let new = NewTeamPage {
            page: page(),
            header: header(true),
            teams_url: "/acme/teams".into(),
            form: TeamForm::default(),
            error: FormError::default(),
        };
        assert!(new.render().unwrap().contains(r#"action="/acme/teams/new""#));
    }
}
