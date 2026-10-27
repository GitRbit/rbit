//! Runners and their registration tokens, for a repository's, an organisation's or a user's
//! settings and the administration panel.
//!
//! Every page shows the same section (`actions/runners_section.html`): the runners of the
//! scope — on a repository also those it inherits from its owner and the instance, read
//! only — with their backends, platform, version, capacity and when they were last seen,
//! and "host — no isolation" for any label that runs on the host; then the scope's
//! registration tokens. A token just created is shown once, with the `rbit-runner register`
//! command to run.
//!
//! Workstream U (interface and secrets) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Timestamp,
    actions::{RegistrationToken, Runner, RunnerBackend, RunnerLabel, RunnerScope},
};

use crate::views::{admin::AdminLink, components::FormError, layout::Page, org::OrgHeader, repo::RepoHeader, settings::SettingsLink};

/// A runner seen this recently counts as online; idle runners poll every 25 s.
const ONLINE_SECONDS: i64 = 90;

/// One runner as the list shows it.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools, reason = "independent facts the list shows")]
pub struct RunnerRow {
    pub id: i64,
    pub name: String,
    /// "repository", "owner" or "instance": where it was registered.
    pub scope: &'static str,
    pub labels: String,
    /// "container", "host" or both.
    pub backends: String,
    /// Some label runs jobs on the host, with no isolation.
    pub host: bool,
    pub platform: String,
    pub version: String,
    pub capacity: u32,
    /// "online", "offline", "disabled" or "never seen".
    pub state: &'static str,
    pub tone: &'static str,
    pub last_seen: Option<String>,
    pub ephemeral: bool,
    pub disabled: bool,
    /// The scope this page manages: its buttons are shown.
    pub manageable: bool,
    /// `POST` target of disable, enable and remove.
    pub url: String,
}

impl RunnerRow {
    pub fn new(runner: &Runner, manageable: bool, base_url: &str, now: Timestamp) -> Self {
        let backends: Vec<&str> = runner
            .capabilities
            .backends
            .iter()
            .map(|backend| match backend {
                RunnerBackend::Container => "container",
                RunnerBackend::Host => "host",
            })
            .collect();
        let (state, tone) = match runner.last_online {
            _ if runner.disabled => ("disabled", "text-ink-faint"),
            Some(seen) if now.seconds() - seen.seconds() <= ONLINE_SECONDS => ("online", "text-success"),
            Some(_) => ("offline", "text-ink-muted"),
            None => ("never seen", "text-ink-muted"),
        };
        let capabilities = &runner.capabilities;
        Self {
            id: runner.id.0,
            name: runner.name.clone(),
            scope: match runner.scope {
                RunnerScope::Repository(_) => "repository",
                RunnerScope::Owner(_) => "owner",
                RunnerScope::Instance => "instance",
            },
            labels: runner.labels.iter().map(RunnerLabel::as_str).collect::<Vec<_>>().join(", "),
            backends: if backends.is_empty() { "—".to_owned() } else { backends.join(", ") },
            host: runner.capabilities.runs_on_host(),
            platform: if capabilities.os.is_empty() { String::new() } else { format!("{}/{}", capabilities.os, capabilities.arch) },
            version: runner.version.clone(),
            capacity: capabilities.capacity,
            state,
            tone,
            last_seen: runner.last_online.map(Timestamp::relative),
            ephemeral: runner.ephemeral,
            disabled: runner.disabled,
            manageable,
            url: format!("{base_url}/{}", runner.id.0),
        }
    }
}

/// One registration token as the list shows it; its value is never shown again.
#[derive(Debug, Clone)]
pub struct TokenRow {
    pub id: i64,
    pub created: String,
    pub expires: String,
    /// "1 use left", "unlimited", "used up", "expired" or "revoked".
    pub uses: String,
    pub usable: bool,
    pub labels: Option<String>,
}

impl TokenRow {
    pub fn new(token: &RegistrationToken, now: Timestamp) -> Self {
        let uses = if token.revoked_at.is_some() {
            "revoked".to_owned()
        } else if now >= token.expires_at {
            "expired".to_owned()
        } else {
            match token.uses_remaining {
                None => "unlimited uses".to_owned(),
                Some(0) => "used up".to_owned(),
                Some(1) => "1 use left".to_owned(),
                Some(left) => format!("{left} uses left"),
            }
        };
        Self {
            id: token.id.0,
            created: token.created_at.relative(),
            expires: token.expires_at.until(),
            uses,
            usable: token.is_usable(now),
            labels: token.labels.as_ref().map(|labels| labels.iter().map(RunnerLabel::as_str).collect::<Vec<_>>().join(", ")),
        }
    }
}

/// A token just created: its value, shown this once, and the command that uses it.
#[derive(Debug, Clone)]
pub struct IssuedToken {
    pub value: String,
    pub command: String,
}

impl IssuedToken {
    /// The token `value` for the instance at `server_url`, pinned to `labels` if any.
    pub fn new(value: String, server_url: &str, labels: Option<&str>) -> Self {
        // The token is read from the terminal, so it stays out of the shell's history and the
        // process list; the page shows it above to paste.
        let mut command = format!("read -rs RBIT_RUNNER_TOKEN && export RBIT_RUNNER_TOKEN && rbit-runner register --url {server_url}");
        if let Some(labels) = labels.filter(|labels| !labels.is_empty()) {
            command.push_str(" --labels ");
            command.push_str(
                &labels
                    .split(',')
                    .map(|label| format!("{}:container:docker.io/library/ubuntu:24.04", label.trim()))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        Self { value, command }
    }
}

/// A choice of the token form.
#[derive(Debug, Clone)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

/// How long a new token lasts, as the form offers it.
pub const LIFETIMES: [Choice; 4] = [
    Choice { value: "3600", label: "1 hour" },
    Choice { value: "86400", label: "1 day" },
    Choice { value: "604800", label: "7 days" },
    Choice { value: "2592000", label: "30 days" },
];

/// The section every runners page shows.
#[derive(Debug, Clone)]
pub struct RunnersSection {
    /// "this repository", "acme", "your account" or "this instance", for sentences.
    pub subject: String,
    pub runners: Vec<RunnerRow>,
    pub tokens: Vec<TokenRow>,
    pub issued: Option<IssuedToken>,
    pub error: FormError,
    /// `POST` target of the token forms.
    pub action_url: String,
    pub lifetimes: &'static [Choice],
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/repo_runners.html")]
pub struct RepoRunnersPage {
    pub page: Page,
    pub header: RepoHeader,
    /// `/{owner}/{repo}/settings/actions`, for the breadcrumb.
    pub actions_url: String,
    pub section: RunnersSection,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/org_runners.html")]
pub struct OrgRunnersPage {
    pub page: Page,
    pub header: OrgHeader,
    pub actions_url: String,
    pub section: RunnersSection,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/user_runners.html")]
pub struct UserRunnersPage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub actions_url: String,
    pub section: RunnersSection,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/admin_runners.html")]
pub struct AdminRunnersPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub section: RunnersSection,
}

#[cfg(test)]
mod tests {
    use rbit_core::actions::{RegistrationTokenId, RunnerCapabilities, RunnerId};

    use super::*;

    fn runner(last_online: Option<Timestamp>, disabled: bool) -> Runner {
        Runner {
            id: RunnerId(3),
            uuid: "u".into(),
            name: "build-1".into(),
            scope: RunnerScope::Instance,
            labels: vec!["linux".parse().unwrap(), "self-hosted".parse().unwrap()],
            pinned_labels: None,
            version: "0.1.3".into(),
            ephemeral: false,
            disabled,
            last_online,
            created_at: Timestamp(0),
            capabilities: RunnerCapabilities {
                backends: vec![RunnerBackend::Container, RunnerBackend::Host],
                os: "linux".into(),
                arch: "x86_64".into(),
                capacity: 2,
                host_labels: vec!["self-hosted".parse().unwrap()],
            },
            protocol: 1,
        }
    }

    #[test]
    fn runners_say_where_they_run_and_whether_they_are_there() {
        let now = Timestamp(10_000);
        let row = RunnerRow::new(&runner(Some(Timestamp(9_950)), false), true, "/admin/runners", now);
        assert_eq!(
            (row.state, row.scope, row.backends.as_str(), row.platform.as_str()),
            ("online", "instance", "container, host", "linux/x86_64")
        );
        assert!(row.host, "a host label means no isolation");
        assert_eq!(row.url, "/admin/runners/3");
        assert_eq!(RunnerRow::new(&runner(Some(Timestamp(1)), false), true, "", now).state, "offline");
        assert_eq!(RunnerRow::new(&runner(None, false), true, "", now).state, "never seen");
        assert_eq!(RunnerRow::new(&runner(Some(now), true), true, "", now).state, "disabled");
    }

    #[test]
    fn tokens_say_how_they_can_still_be_used() {
        let now = Timestamp(100);
        let token = |uses, expires, revoked| RegistrationToken {
            id: RegistrationTokenId(1),
            scope: RunnerScope::Instance,
            labels: None,
            uses_remaining: uses,
            created_by: None,
            created_at: Timestamp(0),
            expires_at: Timestamp(expires),
            revoked_at: revoked,
        };
        assert_eq!(TokenRow::new(&token(Some(1), 200, None), now).uses, "1 use left");
        assert_eq!(TokenRow::new(&token(None, 200, None), now).uses, "unlimited uses");
        assert_eq!(TokenRow::new(&token(Some(0), 200, None), now).uses, "used up");
        assert_eq!(TokenRow::new(&token(Some(1), 50, None), now).uses, "expired");
        assert_eq!(TokenRow::new(&token(Some(1), 200, Some(Timestamp(90))), now).uses, "revoked");
        assert!(!TokenRow::new(&token(Some(1), 50, None), now).usable);
        let issued = IssuedToken::new("rbrg_x".into(), "https://git.example.com", Some("linux, arm64"));
        assert_eq!(
            issued.command,
            "read -rs RBIT_RUNNER_TOKEN && export RBIT_RUNNER_TOKEN && rbit-runner register --url https://git.example.com --labels linux:container:docker.io/library/ubuntu:24.04,arm64:container:docker.io/library/ubuntu:24.04"
        );
    }
}
