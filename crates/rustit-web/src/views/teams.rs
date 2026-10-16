//! Organisation team pages.
//!
//! Milestone 2 workstream A (access) owns this file.

use crate::views::components::SettingsCard;

/// Cards this workstream adds to the organisation settings page; `settings_url` is
/// `/{org}/settings`.
pub fn org_settings_cards(_settings_url: &str) -> Vec<SettingsCard> {
    Vec::new()
}
