//! rbit Actions settings pages: a repository's switch, default token permission, secrets,
//! variables and trusted authors; an organisation's and a user's own secrets and variables.
//!
//! Secret values are write-only: no view model below has a field that could hold one, so no
//! page can render a value back, not even after a rejected form. Variables are plain
//! configuration and are shown. The pages that hold secret forms carry `hx-history="false"`,
//! so htmx never keeps a copy of them in its history cache.
//!
//! Workstream U (interface and secrets) owns this file and `templates/actions/**`.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    RepoId,
    actions::{DefaultTokenPermission, SecretVisibility, Variable},
};
use rbit_services::actions::{
    secrets::{ListedSecret, MAX_VALUE_BYTES, MAX_VARIABLE_BYTES},
    settings::{OwnerRepository, TrustedAuthor},
};

use crate::views::{
    components::{FormError, SettingsCard},
    layout::Page,
    org::OrgHeader,
    repo::RepoHeader,
    settings::SettingsLink,
};

/// The Actions card of the repository settings page: only when the instance has Actions
/// (and the viewer administers the repository, as on every settings page).
pub fn repo_settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    header
        .actions_settings_url
        .iter()
        .map(|url| SettingsCard {
            url: url.clone(),
            icon: "zap",
            title: "Actions",
            description: "Run workflows on pushes and pull requests, with the secrets and variables they use.",
        })
        .collect()
}

/// The Actions card of the organisation settings page; `settings_url` is `/{org}/settings`.
/// Added only when the instance has Actions.
pub fn org_settings_cards(settings_url: &str) -> Vec<SettingsCard> {
    vec![SettingsCard {
        url: format!("{settings_url}/actions"),
        icon: "zap",
        title: "Actions",
        description: "Secrets and variables for the workflows of every repository of this organization.",
    }]
}

/// What the last form did, from `?done=` after its redirect. Unknown values show nothing,
/// so the query never puts words on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    Saved,
    SecretSaved,
    SecretDeleted,
    VariableSaved,
    VariableDeleted,
    Untrusted,
}

impl Notice {
    const ALL: [Self; 6] =
        [Self::Saved, Self::SecretSaved, Self::SecretDeleted, Self::VariableSaved, Self::VariableDeleted, Self::Untrusted];

    /// The `?done=` value.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::SecretSaved => "secret-saved",
            Self::SecretDeleted => "secret-deleted",
            Self::VariableSaved => "variable-saved",
            Self::VariableDeleted => "variable-deleted",
            Self::Untrusted => "untrusted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|notice| notice.key() == value)
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::Saved => "Settings saved.",
            Self::SecretSaved => "Secret saved.",
            Self::SecretDeleted => "Secret deleted.",
            Self::VariableSaved => "Variable saved.",
            Self::VariableDeleted => "Variable deleted.",
            Self::Untrusted => "The author's pull requests need approval again.",
        }
    }
}

/// One of the owner's repositories as a checkbox.
#[derive(Debug, Clone)]
pub struct RepoChoice {
    pub id: i64,
    pub name: String,
    pub checked: bool,
}

impl RepoChoice {
    pub fn list(repositories: &[OwnerRepository], checked: &[RepoId]) -> Vec<Self> {
        repositories.iter().map(|repo| Self { id: repo.id.0, name: repo.name.clone(), checked: checked.contains(&repo.id) }).collect()
    }
}

/// One stored secret: its name and when it changed, never its value.
#[derive(Debug, Clone)]
pub struct SecretRow {
    pub name: String,
    pub updated: String,
    pub updated_iso: String,
    /// Sealed with the current key; otherwise it must be set again.
    pub readable: bool,
    /// Owner secrets: which repositories receive it. Empty for repository secrets.
    pub visibility: String,
    /// Given to selected repositories only.
    pub selected: bool,
    /// The owner's repositories, those it is given to checked (owner secrets).
    pub repositories: Vec<RepoChoice>,
}

impl SecretRow {
    pub fn new(secret: &ListedSecret, owner_repositories: Option<&[OwnerRepository]>) -> Self {
        let selected = secret.info.visibility == SecretVisibility::Selected;
        let visibility = match (owner_repositories, selected) {
            (None, _) => String::new(),
            (Some(_), false) => "All repositories".to_owned(),
            (Some(_), true) => match secret.repositories.len() {
                1 => "1 selected repository".to_owned(),
                count => format!("{count} selected repositories"),
            },
        };
        Self {
            name: secret.info.name.to_string(),
            updated: secret.info.updated_at.relative(),
            updated_iso: secret.info.updated_at.iso8601(),
            readable: secret.readable,
            visibility,
            selected,
            repositories: owner_repositories.map(|all| RepoChoice::list(all, &secret.repositories)).unwrap_or_default(),
        }
    }
}

/// The secrets section of a settings page.
#[derive(Debug, Clone)]
pub struct SecretsSection {
    /// Where its forms post (`kind=secret`).
    pub action_url: String,
    pub rows: Vec<SecretRow>,
    /// Why secrets cannot be stored or used now (the key file), shown above the list.
    pub problem: Option<String>,
    pub error: FormError,
    /// The new secret's name as submitted, after a rejection. Its value is never kept.
    pub name: String,
    /// Owner secrets choose the repositories they are given to.
    pub for_owner: bool,
    /// The new secret's visibility choice.
    pub selected: bool,
    /// The owner's repositories for the new secret (owner secrets).
    pub repositories: Vec<RepoChoice>,
    pub max_kib: usize,
}

impl SecretsSection {
    pub fn new(
        action_url: String,
        secrets: &[ListedSecret],
        owner_repositories: Option<&[OwnerRepository]>,
        problem: Option<String>,
    ) -> Self {
        Self {
            action_url,
            rows: secrets.iter().map(|secret| SecretRow::new(secret, owner_repositories)).collect(),
            problem,
            error: FormError::default(),
            name: String::new(),
            for_owner: owner_repositories.is_some(),
            selected: false,
            repositories: owner_repositories.map(|all| RepoChoice::list(all, &[])).unwrap_or_default(),
            max_kib: MAX_VALUE_BYTES / 1024,
        }
    }

    /// Shows `error` above the new-secret form, keeping the submitted name and repository
    /// choice (never the value).
    #[must_use]
    pub fn rejected(mut self, error: FormError, name: &str, selected: bool, repositories: &[RepoId]) -> Self {
        self.error = error;
        name.trim().clone_into(&mut self.name);
        self.selected = selected;
        for choice in &mut self.repositories {
            choice.checked = repositories.contains(&RepoId(choice.id));
        }
        self
    }
}

/// One stored variable, with its value.
#[derive(Debug, Clone)]
pub struct VariableRow {
    pub name: String,
    pub value: String,
    pub updated: String,
    pub updated_iso: String,
}

impl From<&Variable> for VariableRow {
    fn from(variable: &Variable) -> Self {
        Self {
            name: variable.name.to_string(),
            value: variable.value.clone(),
            updated: variable.updated_at.relative(),
            updated_iso: variable.updated_at.iso8601(),
        }
    }
}

/// The variables section of a settings page.
#[derive(Debug, Clone)]
pub struct VariablesSection {
    /// Where its forms post (`kind=variable`).
    pub action_url: String,
    pub rows: Vec<VariableRow>,
    pub error: FormError,
    /// The new variable as submitted, after a rejection.
    pub name: String,
    pub value: String,
    pub max_kib: usize,
}

impl VariablesSection {
    pub fn new(action_url: String, variables: &[Variable]) -> Self {
        Self {
            action_url,
            rows: variables.iter().map(VariableRow::from).collect(),
            error: FormError::default(),
            name: String::new(),
            value: String::new(),
            max_kib: MAX_VARIABLE_BYTES / 1024,
        }
    }

    /// Shows `error` above the new-variable form, keeping what was submitted.
    #[must_use]
    pub fn rejected(mut self, error: FormError, name: &str, value: &str) -> Self {
        self.error = error;
        name.trim().clone_into(&mut self.name);
        value.clone_into(&mut self.value);
        self
    }
}

/// A choice of the default token permission.
#[derive(Debug, Clone)]
pub struct PermissionOption {
    pub value: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub selected: bool,
}

impl PermissionOption {
    pub fn list(selected: DefaultTokenPermission) -> Vec<Self> {
        [
            (DefaultTokenPermission::Read, "Read", "Workflows can read the repository's contents, issues and pull requests."),
            (
                DefaultTokenPermission::Write,
                "Read and write",
                "Workflows can also push, and comment on issues and pull requests. Branch protection still applies.",
            ),
        ]
        .into_iter()
        .map(|(permission, label, description)| Self { value: permission.as_str(), label, description, selected: permission == selected })
        .collect()
    }
}

/// A trusted pull request author.
#[derive(Debug, Clone)]
pub struct TrustedRow {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub approved_by: Option<String>,
    pub since: String,
    pub since_iso: String,
}

impl From<&TrustedAuthor> for TrustedRow {
    fn from(author: &TrustedAuthor) -> Self {
        Self {
            id: author.account.id.0,
            name: author.account.name.to_string(),
            url: format!("/{}", author.account.name),
            approved_by: author.approved_by.as_ref().map(|account| account.name.to_string()),
            since: author.since.relative(),
            since_iso: author.since.iso8601(),
        }
    }
}

/// `/{owner}/{repo}/settings/actions`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/repo_settings.html")]
pub struct RepoActionsSettingsPage {
    pub page: Page,
    pub header: RepoHeader,
    /// `/{owner}/{repo}/settings`, for the breadcrumb.
    pub settings_url: String,
    /// This page; the switch and the permission post here.
    pub action_url: String,
    pub enabled: bool,
    pub permissions: Vec<PermissionOption>,
    pub general_error: FormError,
    pub notice: Option<Notice>,
    pub secrets: SecretsSection,
    pub variables: VariablesSection,
    pub trusted: Vec<TrustedRow>,
    pub trusted_url: String,
    /// The runners page of this repository.
    pub runners_url: String,
    // Actions: environments.
    pub environments_url: String,
}

/// `/{org}/settings/actions`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/org_settings.html")]
pub struct OrgActionsSettingsPage {
    pub page: Page,
    pub header: OrgHeader,
    pub notice: Option<Notice>,
    pub secrets: SecretsSection,
    pub variables: VariablesSection,
    /// The organisation's runners page.
    pub runners_url: String,
}

/// `/settings/actions`: the signed-in user's own secrets and variables.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/user_settings.html")]
pub struct UserActionsSettingsPage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub notice: Option<Notice>,
    pub secrets: SecretsSection,
    pub variables: VariablesSection,
    /// The user's runners page.
    pub runners_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_round_trip_and_unknown_ones_show_nothing() {
        for notice in Notice::ALL {
            assert_eq!(Notice::parse(notice.key()), Some(notice));
            assert!(notice.message().ends_with('.'));
        }
        assert_eq!(Notice::parse("<script>"), None);
    }

    #[test]
    fn the_card_follows_the_header() {
        assert!(repo_settings_cards(&RepoHeader::default()).is_empty());
        let header = RepoHeader { actions_settings_url: Some("/a/b/settings/actions".to_owned()), ..RepoHeader::default() };
        assert_eq!(repo_settings_cards(&header)[0].url, "/a/b/settings/actions");
        assert_eq!(org_settings_cards("/acme/settings")[0].url, "/acme/settings/actions");
    }

    #[test]
    fn permissions_mark_the_stored_one() {
        let options = PermissionOption::list(DefaultTokenPermission::Write);
        let selected: Vec<_> = options.iter().filter(|option| option.selected).map(|option| option.value).collect();
        assert_eq!(selected, ["write"]);
    }
}

// Actions: environments.
/// An environment in the list.
#[derive(Debug, Clone)]
pub struct EnvironmentRow {
    pub name: String,
    pub url: String,
    /// "2 reviewers · branches main, release/*", or "No protection".
    pub protection: String,
}

/// `/{owner}/{repo}/settings/actions/environments`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/environments.html")]
pub struct EnvironmentsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub settings_url: String,
    /// `/{owner}/{repo}/settings/actions`, for the breadcrumb.
    pub actions_url: String,
    /// This page; the new environment form posts here.
    pub action_url: String,
    pub environments: Vec<EnvironmentRow>,
    pub error: FormError,
    pub new_name: String,
}

/// `/{owner}/{repo}/settings/actions/environments/{name}`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/environment.html")]
pub struct EnvironmentPage {
    pub page: Page,
    pub header: RepoHeader,
    pub settings_url: String,
    pub actions_url: String,
    pub environments_url: String,
    pub name: String,
    /// This page; the protection rules post here.
    pub action_url: String,
    pub delete_url: String,
    /// The reviewers' usernames, one per line.
    pub reviewers: String,
    pub prevent_self_review: bool,
    /// Minutes, as typed.
    pub wait_timer: String,
    /// `all`, `protected` or `selected`.
    pub policy: String,
    /// The selected patterns, one per line.
    pub patterns: String,
    pub error: FormError,
    pub notice: Option<Notice>,
    pub secrets: SecretsSection,
    pub variables: VariablesSection,
}
