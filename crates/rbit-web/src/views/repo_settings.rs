//! Repository settings pages, the star button and the stargazers list.
//!
//! Workstream D owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rbit_services::{StarSummary, stars::Stargazer};

use crate::{
    urls::encode_path,
    views::{
        collaborators,
        components::{Avatar, FormError, SettingsCard},
        layout::Page,
        mirrors, protection,
        repo::RepoHeader,
        transfer, webhooks,
    },
};

// ------------------------------------------------------------------ settings

/// One `<option>` of the default-branch select.
#[derive(Debug, Clone)]
pub struct BranchOption {
    pub name: String,
    pub selected: bool,
}

impl BranchOption {
    /// One option per branch, with `selected` preselected.
    pub fn list<'a>(branches: impl IntoIterator<Item = &'a str>, selected: &str) -> Vec<Self> {
        branches.into_iter().map(|name| Self { name: name.to_owned(), selected: name == selected }).collect()
    }
}

/// The general form's values (stored, or as submitted when re-rendered with an error).
#[derive(Debug, Clone)]
pub struct GeneralForm {
    pub description: String,
    pub is_private: bool,
    /// Empty when the repository has no branches yet.
    pub branches: Vec<BranchOption>,
}

/// Which settings form a validation error belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoSettingsForm {
    General,
    Rename,
    Delete,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/settings.html")]
pub struct RepoSettingsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub general: GeneralForm,
    /// The rename field's value.
    pub name: String,
    /// `owner/name`, which must be typed to confirm deletion.
    pub full_name: String,
    pub general_url: String,
    pub rename_url: String,
    /// Archives or unarchives (`archived=true|false`); the state is `header.archived`.
    pub archive_url: String,
    pub delete_url: String,
    pub general_error: FormError,
    pub rename_error: FormError,
    pub delete_error: FormError,
    /// Set after a successful save (`?saved=1`).
    pub saved: bool,
    /// Links to settings sub-pages (deploy keys, and those milestone 2 workstreams add).
    pub cards: Vec<SettingsCard>,
    /// Links shown in the danger zone above the delete form.
    pub danger_cards: Vec<SettingsCard>,
}

/// The settings sub-pages, in display order. Each feature area contributes its own.
fn settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    let mut cards = vec![SettingsCard {
        url: format!("{}/keys", header.settings_url),
        icon: "server",
        title: "Deploy keys",
        description: "SSH keys that give a server or CI job access to this repository only.",
    }];
    cards.extend(collaborators::repo_settings_cards(header));
    cards.extend(webhooks::repo_settings_cards(header));
    cards.extend(mirrors::repo_settings_cards(header));
    cards.extend(protection::repo_settings_cards(header));
    // Milestone 3: importing and push mirrors (M).
    cards.extend(super::push_mirrors::repo_settings_cards(header));
    cards
}

impl RepoSettingsPage {
    pub fn new(page: Page, header: RepoHeader, general: GeneralForm) -> Self {
        let settings_url = header.settings_url.clone();
        Self {
            cards: settings_cards(&header),
            danger_cards: transfer::danger_zone_cards(&header),
            page: page.titled(format!("Settings · {}/{}", header.owner, header.name)),
            name: header.name.clone(),
            full_name: format!("{}/{}", header.owner, header.name),
            general_url: settings_url.clone(),
            rename_url: format!("{settings_url}/rename"),
            archive_url: format!("{settings_url}/archive"),
            delete_url: format!("{settings_url}/delete"),
            header,
            general,
            general_error: FormError::default(),
            rename_error: FormError::default(),
            delete_error: FormError::default(),
            saved: false,
        }
    }

    /// Re-renders with `error` shown above `form`.
    #[must_use]
    pub fn with_error(mut self, form: RepoSettingsForm, error: FormError) -> Self {
        match form {
            RepoSettingsForm::General => self.general_error = error,
            RepoSettingsForm::Rename => self.rename_error = error,
            RepoSettingsForm::Delete => self.delete_error = error,
        }
        self
    }

    /// Keeps the submitted (rejected) name in the rename field.
    #[must_use]
    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    #[must_use]
    pub fn saved(mut self, saved: bool) -> Self {
        self.saved = saved;
        self
    }
}

// ------------------------------------------------------------------ stars

/// The star toggle and count shown in the repository sub-navigation.
#[derive(Debug, Clone)]
pub struct StarButton {
    /// `POST` target of the toggle.
    pub action_url: String,
    pub stargazers_url: String,
    /// Where anonymous visitors are sent to sign in first.
    pub login_url: String,
    /// Where the no-JavaScript fallback returns to after toggling.
    pub return_to: String,
    pub count: i64,
    pub starred: bool,
    pub is_signed_in: bool,
}

impl StarButton {
    /// `repo_url` is `/{owner}/{repo}`; `return_to` a local path.
    pub fn new(repo_url: &str, summary: StarSummary, is_signed_in: bool, return_to: &str) -> Self {
        Self {
            action_url: format!("{repo_url}/star"),
            stargazers_url: format!("{repo_url}/stargazers"),
            login_url: format!("/login?next={}", encode_path(return_to).replace('&', "%26")),
            return_to: return_to.to_owned(),
            count: summary.count,
            starred: summary.starred_by_viewer,
            is_signed_in,
        }
    }

    /// The toggle's next action (`star` or `unstar`), sent as the form's `action` field.
    pub fn next_action(&self) -> &'static str {
        if self.starred { "unstar" } else { "star" }
    }
}

impl RepoHeader {
    /// The star button for this header; called from `repo/subnav.html`.
    pub fn star_button(&self, is_signed_in: bool, return_to: &str) -> StarButton {
        let summary = StarSummary { count: self.star_count, starred_by_viewer: self.starred_by_viewer };
        StarButton::new(&self.home_url, summary, is_signed_in, return_to)
    }
}

/// Just the star button, swapped in place after an htmx toggle.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/star_button.html")]
pub struct StarButtonFragment {
    pub star: StarButton,
}

/// One account in the stargazers list.
#[derive(Debug, Clone)]
pub struct StargazerRow {
    pub name: String,
    pub label: String,
    pub url: String,
    pub avatar: Avatar,
    pub starred: String,
    pub starred_iso: String,
}

impl From<&Stargazer> for StargazerRow {
    fn from(stargazer: &Stargazer) -> Self {
        let account = &stargazer.account;
        Self {
            name: account.name.to_string(),
            label: account.label().to_owned(),
            url: format!("/{}", account.name),
            avatar: Avatar::for_account(account),
            starred: stargazer.starred_at.relative(),
            starred_iso: stargazer.starred_at.iso8601(),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/stargazers.html")]
pub struct StargazersPage {
    pub page: Page,
    pub header: RepoHeader,
    pub stargazers: Vec<StargazerRow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_button_urls_and_state() {
        let summary = StarSummary { count: 3, starred_by_viewer: true };
        let button = StarButton::new("/alice/proj", summary, true, "/alice/proj/tree/main");
        assert_eq!(button.action_url, "/alice/proj/star");
        assert_eq!(button.stargazers_url, "/alice/proj/stargazers");
        assert_eq!(button.login_url, "/login?next=/alice/proj/tree/main");
        assert_eq!(button.next_action(), "unstar");

        let anonymous = StarButton::new("/alice/proj", StarSummary::default(), false, "/alice/proj/blob/main/a b&c.md");
        assert_eq!(anonymous.login_url, "/login?next=/alice/proj/blob/main/a%20b%26c.md");
        assert_eq!(anonymous.next_action(), "star");
    }

    fn header(can_admin: bool) -> RepoHeader {
        RepoHeader {
            owner: "alice".to_owned(),
            name: "proj".to_owned(),
            owner_avatar: Avatar { initials: "A".to_owned(), palette: 0 },
            clone_url: "http://localhost/alice/proj.git".to_owned(),
            home_url: "/alice/proj".to_owned(),
            commits_url: "/alice/proj/commits/main".to_owned(),
            branches_url: "/alice/proj/branches".to_owned(),
            tags_url: "/alice/proj/tags".to_owned(),
            settings_url: "/alice/proj/settings".to_owned(),
            can_admin,
            star_count: 2,
            starred_by_viewer: false,
            active_tab: "stargazers",
            ..RepoHeader::default()
        }
    }

    fn page() -> Page {
        Page {
            locale: rbit_messages::LocaleSnapshot::english(0),
            ui: rbit_services::ui::UiSnapshot::default(),
            has_i18n: false,
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".to_owned(),
            plugin_revision: 0,
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/alice/proj/stargazers".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    #[test]
    fn star_fragment_renders_toggle() {
        let star = StarButton::new("/alice/proj", StarSummary { count: 1, starred_by_viewer: true }, true, "/alice/proj");
        let html = StarButtonFragment { star }.render().unwrap();
        assert!(html.contains(r#"id="star-button""#));
        assert!(html.contains(r#"hx-post="/alice/proj/star""#));
        assert!(html.contains(r#"value="unstar""#));
        assert!(html.contains("Starred"));
        assert!(html.contains("1 stargazer\""));
    }

    #[test]
    fn settings_tab_only_for_admins() {
        let render = |can_admin| StargazersPage { page: page(), header: header(can_admin), stargazers: Vec::new() }.render().unwrap();
        let admin = render(true);
        assert!(admin.contains(r#"href="/alice/proj/settings""#));
        let visitor = render(false);
        assert!(!visitor.contains(r#"href="/alice/proj/settings""#));
        assert!(visitor.contains(r#"href="/login?next=/alice/proj/stargazers""#), "anonymous visitors get a sign-in link");
    }

    #[test]
    fn settings_page_renders_forms() {
        let general = GeneralForm { description: "Hi".to_owned(), is_private: true, branches: BranchOption::list(["main", "dev"], "dev") };
        let html = RepoSettingsPage::new(page(), header(true), general)
            .with_error(RepoSettingsForm::Delete, FormError::new("Nope."))
            .render()
            .unwrap();
        assert!(html.contains(r#"action="/alice/proj/settings/rename""#));
        assert!(html.contains(r#"action="/alice/proj/settings/delete""#));
        assert!(html.contains(r#"<option value="dev" selected>"#));
        assert!(html.contains("Nope."));
        assert!(html.contains(r#"href="/alice/proj/settings/keys""#), "settings sub-pages are linked as cards");
        assert!(html.contains("Deploy keys"));
    }

    #[test]
    fn issue_and_pull_request_tabs_follow_code_with_open_counts() {
        let mut header = header(false);
        header.issues_url = "/alice/proj/issues".to_owned();
        header.pulls_url = "/alice/proj/pulls".to_owned();
        let render = |header: RepoHeader| StargazersPage { page: page(), header, stargazers: Vec::new() }.render().unwrap();
        let html = render(header.clone());
        let (code, issues, pulls) = (html.find(">Code<").unwrap(), html.find(">Issues").unwrap(), html.find(">Pull requests").unwrap());
        assert!(code < issues && issues < pulls, "Code, Issues, Pull requests");
        assert!(html.contains(r#"href="/alice/proj/issues""#) && html.contains(r#"href="/alice/proj/pulls""#));
        assert!(!html.contains(r#"<span class="badge tabular-nums">"#), "no count while there is nothing open");
        let html = render(RepoHeader { open_issues: 3, open_pulls: 1, active_tab: "issues", ..header });
        assert!(
            html.contains(r#"<span class="badge tabular-nums">3</span>"#) && html.contains(r#"<span class="badge tabular-nums">1</span>"#)
        );
        assert!(html.contains(r#"href="/alice/proj/issues" aria-current="page""#));
    }

    #[test]
    fn branch_options_mark_selection() {
        let options = BranchOption::list(["main", "develop"], "develop");
        assert!(!options[0].selected);
        assert!(options[1].selected);
    }
}
