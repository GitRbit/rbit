//! Branch protection settings pages.
//!
//! Milestone 2 workstream B (branch protection) owns this file.
//!
//! `/{owner}/{repo}/settings/branches` lists the rules (pattern in monospace, one badge per
//! protection, the branches each matches) with "Add rule"; `/settings/branches/new` and
//! `/settings/branches/{id}` hold the rule form, the edit page also the branches the
//! pattern matches and a delete form with confirmation.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    membership::Team,
    protection::{BranchPattern, BranchProtection, MAX_REQUIRED_APPROVALS, MAX_RULES, RuleSettings},
};
use rbit_services::protection::ProtectionInput;

use crate::views::{components::FormError, components::SettingsCard, layout::Page, repo::RepoHeader};

/// Branch names listed per rule before "and N more".
const MATCHES_SHOWN: usize = 5;

/// Cards this workstream adds to the repository settings page ("Branch protection",
/// icon `shield`).
pub fn repo_settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    vec![SettingsCard {
        url: rules_url(header),
        icon: "shield",
        title: "Branch protection",
        description: "Keep important branches from being force-pushed, deleted or pushed to without a pull request.",
    }]
}

/// `/{owner}/{repo}/settings/branches`.
pub fn rules_url(header: &RepoHeader) -> String {
    format!("{}/branches", header.settings_url)
}

/// The summary badges of a rule, as on the list.
pub fn badges(settings: &RuleSettings) -> Vec<String> {
    let mut badges = Vec::new();
    if settings.deny_force_push {
        badges.push("No force pushes".to_owned());
    }
    if settings.deny_deletion {
        badges.push("No deletion".to_owned());
    }
    if settings.require_pull_request {
        badges.push(match settings.required_approvals {
            0 => "Pull request".to_owned(),
            1 => "1 approval".to_owned(),
            n => format!("{n} approvals"),
        });
    }
    if settings.restrict_pushers {
        badges.push("Restricted".to_owned());
    }
    if settings.allow_admin_bypass {
        badges.push("Admins bypass".to_owned());
    }
    badges
}

/// The existing branches `pattern` matches, in the given order.
pub fn matching<'a>(pattern: &BranchPattern, branches: &'a [String]) -> Vec<&'a str> {
    branches.iter().filter(|branch| pattern.matches(branch)).map(String::as_str).collect()
}

/// "main, release/1.0" (the first few names, then "and N more"); empty without matches.
fn matches_text(matches: &[&str]) -> String {
    let shown = matches.iter().take(MATCHES_SHOWN).copied().collect::<Vec<_>>().join(", ");
    match matches.len().saturating_sub(MATCHES_SHOWN) {
        0 => shown,
        more => format!("{shown} and {more} more"),
    }
}

/// One rule on the list.
#[derive(Debug, Clone)]
pub struct RuleRow {
    pub pattern: String,
    pub edit_url: String,
    pub badges: Vec<String>,
    /// "Matches main, release/1.0" or "Matches no branch yet".
    pub matches: String,
}

/// `/{owner}/{repo}/settings/branches`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "protection/list.html")]
pub struct RulesPage {
    pub page: Page,
    pub header: RepoHeader,
    pub full_name: String,
    pub settings_url: String,
    pub new_url: String,
    pub rows: Vec<RuleRow>,
    pub can_add: bool,
    pub max_rules: usize,
    /// "Rule deleted." after a deletion.
    pub deleted: bool,
}

impl RulesPage {
    pub fn new(page: Page, header: RepoHeader, rules: &[BranchProtection], branches: &[String]) -> Self {
        let list_url = rules_url(&header);
        let rows = rules
            .iter()
            .map(|rule| {
                let matches = matching(&rule.rule.pattern, branches);
                RuleRow {
                    pattern: rule.rule.pattern.to_string(),
                    edit_url: format!("{list_url}/{}", rule.id.0),
                    badges: badges(&rule.rule.settings),
                    matches: if matches.is_empty() {
                        "Matches no branch yet".to_owned()
                    } else {
                        format!("Matches {}", matches_text(&matches))
                    },
                }
            })
            .collect();
        Self {
            page: page.titled(format!("Branch protection · {}/{}", header.owner, header.name)),
            full_name: format!("{}/{}", header.owner, header.name),
            settings_url: header.settings_url.clone(),
            new_url: format!("{list_url}/new"),
            can_add: rules.len() < MAX_RULES,
            max_rules: MAX_RULES,
            rows,
            header,
            deleted: false,
        }
    }

    #[must_use]
    pub fn deleted(mut self, deleted: bool) -> Self {
        self.deleted = deleted;
        self
    }
}

/// A team checkbox on the rule form.
#[derive(Debug, Clone)]
pub struct TeamChoice {
    pub name: String,
    pub checked: bool,
}

/// One option of the approvals picker.
#[derive(Debug, Clone)]
pub struct ApprovalOption {
    pub value: u8,
    pub selected: bool,
}

/// The rule form's values.
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "one checkbox per rule flag")]
pub struct RuleForm {
    pub pattern: String,
    pub deny_force_push: bool,
    pub deny_deletion: bool,
    pub require_pull_request: bool,
    pub approvals: Vec<ApprovalOption>,
    pub dismiss_stale_approvals: bool,
    pub block_on_changes_requested: bool,
    pub restrict_pushers: bool,
    pub allow_admin_bypass: bool,
    /// Allowed usernames, separated by spaces.
    pub users: String,
    pub teams: Vec<TeamChoice>,
}

impl RuleForm {
    /// The form showing `input`, with the organisation's `teams` to pick from.
    pub fn new(input: &ProtectionInput, teams: &[Team]) -> Self {
        let settings = &input.settings;
        let picked = |name: &str| input.teams.iter().any(|team| team.trim().eq_ignore_ascii_case(name));
        Self {
            pattern: input.pattern.clone(),
            deny_force_push: settings.deny_force_push,
            deny_deletion: settings.deny_deletion,
            require_pull_request: settings.require_pull_request,
            approvals: (0..=MAX_REQUIRED_APPROVALS)
                .map(|value| ApprovalOption { value, selected: value == settings.required_approvals })
                .collect(),
            dismiss_stale_approvals: settings.dismiss_stale_approvals,
            block_on_changes_requested: settings.block_on_changes_requested,
            restrict_pushers: settings.restrict_pushers,
            allow_admin_bypass: settings.allow_admin_bypass,
            users: input.users.join(" "),
            teams: teams.iter().map(|team| TeamChoice { name: team.name.to_string(), checked: picked(team.name.as_str()) }).collect(),
        }
    }
}

/// What the edit page shows besides the form.
#[derive(Debug, Clone)]
pub struct RuleMeta {
    pub pattern: String,
    pub delete_url: String,
    /// The existing branches the stored pattern matches.
    pub matches: Vec<String>,
}

/// `/settings/branches/new` and `/settings/branches/{id}`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "protection/form.html")]
pub struct RuleFormPage {
    pub page: Page,
    pub header: RepoHeader,
    pub settings_url: String,
    pub list_url: String,
    pub action_url: String,
    pub is_org: bool,
    pub form: RuleForm,
    pub error: FormError,
    /// Set on the edit page.
    pub rule: Option<RuleMeta>,
    pub delete_error: FormError,
    pub saved: bool,
}

impl RuleFormPage {
    /// The empty form for a new rule (the defaults of [`RuleSettings`]).
    pub fn new_rule(page: Page, header: RepoHeader, is_org: bool, teams: &[Team]) -> Self {
        let list_url = rules_url(&header);
        Self {
            page: page.titled(format!("New branch protection rule · {}/{}", header.owner, header.name)),
            settings_url: header.settings_url.clone(),
            action_url: list_url.clone(),
            list_url,
            is_org,
            form: RuleForm::new(&ProtectionInput::default(), teams),
            error: FormError::default(),
            rule: None,
            delete_error: FormError::default(),
            saved: false,
            header,
        }
    }

    /// The edit page of `rule`, with the existing `branches`.
    pub fn edit(page: Page, header: RepoHeader, is_org: bool, teams: &[Team], rule: &BranchProtection, branches: &[String]) -> Self {
        let list_url = rules_url(&header);
        let rule_url = format!("{list_url}/{}", rule.id.0);
        Self {
            page: page.titled(format!("{} · Branch protection · {}/{}", rule.rule.pattern, header.owner, header.name)),
            settings_url: header.settings_url.clone(),
            action_url: rule_url.clone(),
            list_url,
            is_org,
            form: RuleForm::new(&ProtectionInput::from_rule(rule), teams),
            error: FormError::default(),
            rule: Some(RuleMeta {
                pattern: rule.rule.pattern.to_string(),
                delete_url: format!("{rule_url}/delete"),
                matches: matching(&rule.rule.pattern, branches).into_iter().map(str::to_owned).collect(),
            }),
            delete_error: FormError::default(),
            saved: false,
            header,
        }
    }

    /// Shows `form` as submitted with a validation error.
    #[must_use]
    pub fn rejected(mut self, form: RuleForm, error: FormError) -> Self {
        self.form = form;
        self.error = error;
        self
    }

    #[must_use]
    pub fn delete_rejected(mut self, error: FormError) -> Self {
        self.delete_error = error;
        self
    }

    #[must_use]
    pub fn saved(mut self, saved: bool) -> Self {
        self.saved = saved;
        self
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        Account, AccountId, AccountKind, RepoId, Timestamp,
        membership::TeamId,
        protection::{ProtectionId, ProtectionRule},
    };

    use super::*;

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
            title: None,
            viewer: None,
            path: "/acme/rocket/settings/branches".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    fn team(id: i64, name: &str) -> Team {
        Team { id: TeamId(id), org_id: AccountId(9), name: name.parse().unwrap(), description: None, created_at: Timestamp(0) }
    }

    fn rule(id: i64, pattern: &str, settings: RuleSettings) -> BranchProtection {
        let bob = Account {
            id: AccountId(2),
            kind: AccountKind::User,
            name: "bob".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        };
        BranchProtection {
            id: ProtectionId(id),
            repo_id: RepoId(1),
            rule: ProtectionRule {
                pattern: pattern.parse().unwrap(),
                settings,
                allowed_users: vec![bob.id],
                allowed_teams: vec![TeamId(1)],
            },
            users: vec![bob],
            teams: vec![team(1, "core")],
            created_by: None,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        }
    }

    fn branches() -> Vec<String> {
        ["main", "release/1.0", "release/2.0", "feature"].map(str::to_owned).to_vec()
    }

    #[test]
    fn settings_card_and_badges() {
        let cards = repo_settings_cards(&header());
        assert_eq!((cards[0].url.as_str(), cards[0].icon), ("/acme/rocket/settings/branches", "shield"));
        assert_eq!(badges(&RuleSettings::default()), ["No force pushes", "No deletion"]);
        let strict = RuleSettings {
            require_pull_request: true,
            required_approvals: 2,
            restrict_pushers: true,
            allow_admin_bypass: true,
            ..RuleSettings::default()
        };
        assert_eq!(badges(&strict), ["No force pushes", "No deletion", "2 approvals", "Restricted", "Admins bypass"]);
        assert_eq!(badges(&RuleSettings { require_pull_request: true, ..RuleSettings::default() })[2], "Pull request");
        assert_eq!(badges(&RuleSettings { require_pull_request: true, required_approvals: 1, ..RuleSettings::default() })[2], "1 approval");
    }

    #[test]
    fn matches_are_listed_briefly() {
        let names: Vec<String> = (0..8).map(|n| format!("b{n}")).collect();
        let all = matching(&"b*".parse().unwrap(), &names);
        assert_eq!(matches_text(&all), "b0, b1, b2, b3, b4 and 3 more");
        assert_eq!(matches_text(&all[..2]), "b0, b1");
    }

    #[test]
    fn the_list_shows_patterns_badges_and_matches() {
        let rules = [
            rule(3, "main", RuleSettings::default()),
            rule(4, "release/*", RuleSettings { require_pull_request: true, required_approvals: 2, ..RuleSettings::default() }),
            rule(5, "hotfix/*", RuleSettings::default()),
        ];
        let html = RulesPage::new(page(), header(), &rules, &branches()).deleted(true).render().unwrap();
        assert!(html.contains(r#"href="/acme/rocket/settings/branches/new""#));
        assert!(html.contains(r#"href="/acme/rocket/settings/branches/4""#));
        assert!(html.contains(">release/*<") && html.contains("font-mono"));
        assert!(html.contains("Matches release/1.0, release/2.0"));
        assert!(html.contains("Matches no branch yet"));
        assert!(html.contains(">2 approvals<") && html.contains(">No force pushes<"));
        assert!(html.contains("Rule deleted."));
        assert!(!html.contains("style="), "no inline styles under the CSP");

        let empty = RulesPage::new(page(), header(), &[], &branches()).render().unwrap();
        assert!(empty.contains("No protected branches"));
    }

    #[test]
    fn the_form_shows_defaults_and_the_stored_rule() {
        let teams = [team(1, "core"), team(2, "docs")];
        let html = RuleFormPage::new_rule(page(), header(), true, &teams).render().unwrap();
        assert!(html.contains(r#"action="/acme/rocket/settings/branches""#));
        assert!(html.contains(r#"name="deny_force_push" value="on" class="mt-1 accent-(--ink)" checked"#));
        assert!(html.contains(r#"name="block_on_changes_requested" value="on" class="mt-0.5 accent-(--ink)" checked"#));
        assert!(!html.contains(r#"name="allow_admin_bypass" value="on" class="mt-1 accent-(--ink)" checked"#), "bypass is off by default");
        assert!(html.contains(r#"<option value="0" selected>"#) && html.contains(r#"<option value="10">"#));
        assert!(html.contains(r#"name="team" value="docs""#));
        assert!(!html.contains("Danger zone"));

        let stored = rule(3, "release/*", RuleSettings { restrict_pushers: true, ..RuleSettings::default() });
        let html = RuleFormPage::edit(page(), header(), true, &teams, &stored, &branches()).saved(true).render().unwrap();
        assert!(html.contains(r#"action="/acme/rocket/settings/branches/3""#));
        assert!(html.contains(r#"action="/acme/rocket/settings/branches/3/delete""#));
        assert!(html.contains(r#"value="release/*""#) && html.contains(r#"value="bob""#));
        assert!(html.contains(r#"name="team" value="core" class="accent-(--ink)" checked"#));
        assert!(html.contains(r#"name="team" value="docs" class="accent-(--ink)">"#));
        assert!(html.contains(">release/1.0<") && html.contains(">release/2.0<") && !html.contains(">feature<"));
        assert!(html.contains("Rule saved."));
        assert!(!html.contains("style="));

        let personal = RuleFormPage::new_rule(page(), header(), false, &[])
            .rejected(
                RuleForm::new(&ProtectionInput { pattern: "a b".to_owned(), ..ProtectionInput::default() }, &[]),
                FormError::new("Bad pattern."),
            )
            .render()
            .unwrap();
        assert!(personal.contains("Bad pattern.") && personal.contains(r#"value="a b""#));
        assert!(!personal.contains(r#"name="team""#), "teams only for organisations");
    }
}
