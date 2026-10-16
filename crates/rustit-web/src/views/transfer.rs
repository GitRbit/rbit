//! The repository transfer page.
//!
//! Milestone 2 workstream A (access) owns this file.

use crate::views::{components::SettingsCard, repo::RepoHeader};

/// Cards shown in the danger zone of the repository settings page (e.g. "Transfer ownership").
pub fn danger_zone_cards(_header: &RepoHeader) -> Vec<SettingsCard> {
    Vec::new()
}
