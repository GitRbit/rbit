//! Branch protection settings pages.
//!
//! Milestone 2 workstream B (branch protection) owns this file.

use crate::views::{components::SettingsCard, repo::RepoHeader};

/// Cards this workstream adds to the repository settings page ("Branch protection",
/// icon `shield`).
pub fn repo_settings_cards(_header: &RepoHeader) -> Vec<SettingsCard> {
    Vec::new()
}
