//! Webhook settings pages and the delivery log.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.

use crate::views::{components::SettingsCard, repo::RepoHeader};

/// Cards this workstream adds to the repository settings page (e.g. "Webhooks").
pub fn repo_settings_cards(_header: &RepoHeader) -> Vec<SettingsCard> {
    Vec::new()
}

/// Cards this workstream adds to the organisation settings page; `settings_url` is
/// `/{org}/settings`.
pub fn org_settings_cards(_settings_url: &str) -> Vec<SettingsCard> {
    Vec::new()
}
