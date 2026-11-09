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
    when::When,
};

/// Cards this workstream adds to the repository settings page: "Mirror", for mirrors.
pub fn repo_settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    if header.mirror_url.is_none() {
        return Vec::new();
    }
    vec![SettingsCard {
        url: format!("{}/mirror", header.settings_url),
        icon: "refresh",
        title: rbit_localization::settings::cards::CardMessage::Mirror,
        description: rbit_localization::settings::cards::CardMessage::MirrorText,
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
    pub last_sync: Option<When>,
    pub last_error: Option<String>,
    /// Shown as "in 3 hours" ("in under a minute" once due) in the page's language.
    pub next_sync: When,
    pub next_sync_iso: String,
}

impl From<&Mirror> for MirrorView {
    fn from(mirror: &Mirror) -> Self {
        Self {
            url: mirror.url.to_string(),
            interval_minutes: mirror.interval_minutes,
            last_sync: mirror.last_sync_at.map(When),
            last_error: mirror.last_error.clone(),
            next_sync: When(mirror.next_sync_at),
            next_sync_iso: mirror.next_sync_at.iso8601(),
        }
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
    pub notice: Option<rbit_localization::Localized>,
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
    fn only_mirrors_get_a_settings_card() {
        let plain = RepoHeader { settings_url: "/a/b/settings".into(), ..RepoHeader::default() };
        assert!(repo_settings_cards(&plain).is_empty());
        let mirror = RepoHeader { mirror_url: Some("https://example.com/x.git".into()), ..plain };
        assert_eq!(repo_settings_cards(&mirror)[0].url, "/a/b/settings/mirror");
    }
}
