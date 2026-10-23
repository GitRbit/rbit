//! Independent release-check and running-version disclosure controls.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateConfig {
    /// Check the upstream public release feed in the background, at most daily after success.
    pub check_for_updates: bool,
    /// Expose the running version publicly and in existing outbound integration User-Agents.
    /// Release checks themselves never transmit the running version.
    pub report_version: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self { check_for_updates: true, report_version: true }
    }
}
