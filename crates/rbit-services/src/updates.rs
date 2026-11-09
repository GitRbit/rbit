//! The shared release-notice snapshot read by the footer and administrator dashboard.
//!
//! Rendering never performs network I/O. The background check that decides and publishes
//! these snapshots is composed by the server (`rbit-server` `updates`); this cell only
//! stores the latest one and stamps its ordering revision.

use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use rbit_core::config::UpdateConfig;

/// A validated public release. Its link always belongs to the fixed upstream repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableRelease {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateState {
    Disabled,
    Pending,
    UpToDate,
    Available,
    Unavailable,
}

/// A cheap snapshot of the last check, shared by the footer and administrator dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    /// UTC nanoseconds of the last observation, ordered across page caches and restarts.
    pub revision: u128,
    pub state: UpdateState,
    pub release: Option<AvailableRelease>,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self { revision: 0, state: UpdateState::Disabled, release: None }
    }
}

impl UpdateStatus {
    /// Versionless browser notice metadata, independent of current-version reporting.
    pub fn notice(&self) -> &str {
        self.release.as_ref().map_or_else(
            || match self.state {
                UpdateState::Disabled | UpdateState::UpToDate => "none",
                _ => "pending",
            },
            |release| release.url.as_str(),
        )
    }

    pub const fn label(&self) -> &'static str {
        match self.state {
            UpdateState::Disabled => "Automatic update checks are disabled.",
            UpdateState::Pending => "Waiting for the first release check.",
            UpdateState::UpToDate => "No newer stable release is available.",
            UpdateState::Available => "A newer stable release is available.",
            UpdateState::Unavailable => "The last release check was unavailable. Retrying automatically.",
        }
    }
}

/// The latest published snapshot. Cheap to clone; every clone shares one cell.
#[derive(Debug, Clone)]
pub struct UpdateNotices {
    status: Arc<Mutex<UpdateStatus>>,
}

impl UpdateNotices {
    /// Disabled when checks are off, otherwise waiting for the first published result.
    pub fn new(config: &UpdateConfig) -> Self {
        let state = if config.check_for_updates { UpdateState::Pending } else { UpdateState::Disabled };
        Self { status: Arc::new(Mutex::new(UpdateStatus { revision: next_revision(0), state, release: None })) }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Replaces the snapshot with a strictly newer revision. Reading never advances it.
    pub fn publish(&self, state: UpdateState, release: Option<AvailableRelease>) {
        let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        *status = UpdateStatus { revision: next_revision(status.revision), state, release };
    }
}

fn next_revision(previous: u128) -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(previous.saturating_add(1), |elapsed| elapsed.as_nanos().max(previous.saturating_add(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_selects_the_initial_state_and_publishing_advances_the_revision() {
        let disabled = UpdateNotices::new(&UpdateConfig { check_for_updates: false, report_version: true });
        assert_eq!(disabled.status().state, UpdateState::Disabled);
        let notices = UpdateNotices::new(&UpdateConfig::default());
        let initial = notices.status();
        assert_eq!((initial.state, initial.notice()), (UpdateState::Pending, "pending"));
        assert_eq!(notices.status().revision, initial.revision, "reading a snapshot does not advance its revision");
        let release = AvailableRelease { version: "99.0.0".into(), url: "https://github.com/GitRbit/rbit/releases/tag/v99.0.0".into() };
        notices.publish(UpdateState::Available, Some(release.clone()));
        let available = notices.clone().status();
        assert!(available.revision > initial.revision);
        assert_eq!(available.notice(), release.url);
        notices.publish(UpdateState::UpToDate, None);
        assert!(notices.status().revision > available.revision);
        assert_eq!(notices.status().notice(), "none");
    }
}
