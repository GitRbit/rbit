//! Shared host observation of whether the optional Actions control plane is available.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Cheap, shared availability flag for host services that advertise Actions features.
///
/// This is visibility metadata, not an authorization grant. Trusted composition code
/// updates it when the control plane activates or stops; task credentials still require
/// their own current authorization checks.
#[derive(Debug, Clone, Default)]
pub struct ActionsAvailability(Arc<AtomicBool>);

impl ActionsAvailability {
    /// Whether the host currently has an active Actions control plane.
    pub fn is_enabled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Publishes control-plane availability to all cloned host contexts.
    pub fn set_enabled(&self, enabled: bool) {
        self.0.store(enabled, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::ActionsAvailability;

    #[test]
    fn disabled_by_default_and_changes_reach_existing_contexts() {
        let availability = ActionsAvailability::default();
        let cloned = availability.clone();
        assert!(!availability.is_enabled());
        availability.set_enabled(true);
        assert!(cloned.is_enabled());
        cloned.set_enabled(false);
        assert!(!availability.is_enabled());
    }
}
