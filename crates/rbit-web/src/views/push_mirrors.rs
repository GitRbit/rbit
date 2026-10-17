//! Push mirror settings: `/{owner}/{repo}/settings/push-mirrors`.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::push_mirror::PushMirror;

use crate::views::{
    components::{FormError, SettingsCard},
    layout::Page,
    repo::RepoHeader,
};

/// The "Push mirrors" card on the repository settings page.
pub fn repo_settings_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    vec![SettingsCard {
        url: format!("{}/push-mirrors", header.settings_url),
        icon: "refresh",
        title: "Push mirrors",
        description: "Remotes that receive this repository's branches and tags after every push.",
    }]
}

/// One mirror as listed.
#[derive(Debug, Clone)]
pub struct PushMirrorView {
    pub url: String,
    pub username: String,
    pub has_secret: bool,
    pub enabled: bool,
    pub sync_on_push: bool,
    /// `(relative, ISO 8601)` of the last attempt.
    pub last_attempt: Option<(String, String)>,
    pub last_error: Option<String>,
    pub edit_url: String,
    pub sync_url: String,
    pub delete_url: String,
}

impl PushMirrorView {
    pub fn new(mirror: &PushMirror, base_url: &str) -> Self {
        let url = format!("{base_url}/{}", mirror.id.0);
        Self {
            url: mirror.url.clone(),
            username: mirror.username.clone(),
            has_secret: mirror.has_secret,
            enabled: mirror.enabled,
            sync_on_push: mirror.sync_on_push,
            last_attempt: mirror.last_attempt_at.map(|at| (at.relative(), at.iso8601())),
            last_error: mirror.last_error.clone(),
            sync_url: format!("{url}/sync"),
            delete_url: format!("{url}/delete"),
            edit_url: url,
        }
    }
}

/// The add or edit form as submitted (or prefilled).
#[derive(Debug, Clone, Default)]
pub struct PushMirrorForm {
    pub url: String,
    pub username: String,
    /// A secret is stored (the field then says "leave blank to keep it").
    pub has_secret: bool,
    pub enabled: bool,
    pub sync_on_push: bool,
}

/// `/{owner}/{repo}/settings/push-mirrors`
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/push_mirrors.html")]
pub struct PushMirrorsPage {
    pub page: Page,
    pub header: RepoHeader,
    /// Push mirrors are switched off on this instance: a notice replaces the page.
    pub disabled: bool,
    pub mirrors: Vec<PushMirrorView>,
    pub form: PushMirrorForm,
    pub error: FormError,
    pub notice: Option<&'static str>,
    pub interval_minutes: u64,
    pub action_url: String,
    /// The test switch admits `file://` remotes.
    pub allow_local: bool,
}

/// `/{owner}/{repo}/settings/push-mirrors/{id}`
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/push_mirror_edit.html")]
pub struct PushMirrorEditPage {
    pub page: Page,
    pub header: RepoHeader,
    pub mirror: PushMirrorView,
    pub form: PushMirrorForm,
    pub error: FormError,
    pub list_url: String,
    pub allow_local: bool,
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        AccountId, RepoId, Timestamp,
        push_mirror::{PushMirror, PushMirrorId},
    };

    use super::*;

    #[test]
    fn mirrors_link_to_their_actions_and_never_carry_a_secret() {
        let mirror = PushMirror {
            id: PushMirrorId(7),
            repo_id: RepoId(1),
            url: "https://example.com/a.git".into(),
            username: "bot".into(),
            has_secret: true,
            enabled: true,
            sync_on_push: false,
            next_sync_at: Timestamp(0),
            last_attempt_at: Some(Timestamp(1)),
            last_success_at: None,
            last_error: Some("rejected".into()),
            created_by: Some(AccountId(1)),
            created_at: Timestamp(0),
        };
        let view = PushMirrorView::new(&mirror, "/a/b/settings/push-mirrors");
        assert_eq!(view.edit_url, "/a/b/settings/push-mirrors/7");
        assert_eq!(view.sync_url, "/a/b/settings/push-mirrors/7/sync");
        assert_eq!(view.delete_url, "/a/b/settings/push-mirrors/7/delete");
        assert!(view.has_secret && view.last_attempt.is_some());
        let header = RepoHeader { settings_url: "/a/b/settings".into(), ..RepoHeader::default() };
        assert_eq!(repo_settings_cards(&header)[0].url, "/a/b/settings/push-mirrors");
    }
}
