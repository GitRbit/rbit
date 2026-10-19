//! Pull mirror settings and the "mirror a repository" form (stretch goal).
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::mirror::Mirror;

use crate::views::{
    components::{FormError, SettingsCard},
    home::OwnerOption,
    layout::Page,
    repo::RepoHeader,
};

/// Cards this workstream adds to the repository settings page: "Mirror", for mirrors.
pub fn repo_settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    if header.mirror_url.is_none() {
        return Vec::new();
    }
    vec![SettingsCard {
        url: format!("{}/mirror", header.settings_url),
        icon: "refresh",
        title: "Mirror",
        description: "Upstream, fetch interval and the outcome of the last fetch.",
    }]
}

/// `/new/mirror`: create a repository that mirrors an upstream.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/new_mirror.html")]
pub struct NewMirrorPage {
    pub page: Page,
    pub error: FormError,
    /// Mirrors are disabled on this instance: the form is replaced by a notice.
    pub disabled: bool,
    pub owners: Vec<OwnerOption>,
    pub url: String,
    pub name: String,
    pub description: String,
    pub private: bool,
    pub interval_minutes: String,
    pub min_interval_minutes: u64,
}

/// A mirror's state as shown on its settings page.
#[derive(Debug, Clone)]
pub struct MirrorView {
    pub url: String,
    pub interval_minutes: u64,
    pub last_sync: Option<(String, String)>,
    pub last_error: Option<String>,
    pub next_sync: String,
    pub next_sync_iso: String,
}

impl From<&Mirror> for MirrorView {
    fn from(mirror: &Mirror) -> Self {
        Self {
            url: mirror.url.to_string(),
            interval_minutes: mirror.interval_minutes,
            last_sync: mirror.last_sync_at.map(|at| (at.relative(), at.iso8601())),
            last_error: mirror.last_error.clone(),
            next_sync: until(mirror.next_sync_at.seconds() - rbit_core::Timestamp::now().seconds()),
            next_sync_iso: mirror.next_sync_at.iso8601(),
        }
    }
}

/// "in 3 hours" for a time `seconds` from now; "due now" once it has passed.
fn until(seconds: i64) -> String {
    let plural = |n: i64, unit: &str| if n == 1 { format!("in 1 {unit}") } else { format!("in {n} {unit}s") };
    match seconds {
        ..60 => "due now".to_owned(),
        60..3600 => plural(seconds / 60, "minute"),
        3600..86_400 => plural(seconds / 3600, "hour"),
        _ => plural(seconds / 86_400, "day"),
    }
}

/// `/{owner}/{repo}/settings/mirror`
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/mirror_settings.html")]
pub struct MirrorSettingsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub mirror: MirrorView,
    pub error: FormError,
    pub notice: Option<&'static str>,
    pub min_interval_minutes: u64,
    pub action_url: String,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/mirror_progress.html")]
pub struct MirrorProgressPage {
    pub page: Page,
    pub header: RepoHeader,
    pub status_url: String,
    pub refresh_url: String,
    pub settings_url: String,
    pub phase: &'static str,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upcoming_fetches_read_naturally() {
        assert_eq!(until(-5), "due now");
        assert_eq!(until(90), "in 1 minute");
        assert_eq!(until(3 * 3600 + 5), "in 3 hours");
        assert_eq!(until(2 * 86_400), "in 2 days");
    }

    #[test]
    fn only_mirrors_get_a_settings_card() {
        let plain = RepoHeader { settings_url: "/a/b/settings".into(), ..RepoHeader::default() };
        assert!(repo_settings_cards(&plain).is_empty());
        let mirror = RepoHeader { mirror_url: Some("https://example.com/x.git".into()), ..plain };
        assert_eq!(repo_settings_cards(&mirror)[0].url, "/a/b/settings/mirror");
    }
}
