//! Repository collaborator pages.
//!
//! Milestone 2 workstream A (access) owns this file.

use crate::views::{components::SettingsCard, repo::RepoHeader};

/// Cards this workstream adds to the repository settings page (e.g. "Collaborators").
pub fn repo_settings_cards(_header: &RepoHeader) -> Vec<SettingsCard> {
    Vec::new()
}
