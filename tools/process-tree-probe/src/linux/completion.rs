//! The receipt applies only to known fixture capabilities, never arbitrary descendants.

use anyhow::{Result, ensure};

#[derive(Debug)]
pub(super) struct FixtureCompletion {
    _private: (),
}
impl FixtureCompletion {
    pub fn observe(root_reaped: bool, known_pidfds_exited: bool, pump_completed: bool) -> Result<Self> {
        ensure!(root_reaped && known_pidfds_exited && pump_completed, "known fixture completion is unproved");
        Ok(Self { _private: () })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_success_or_an_exited_root_cannot_replace_other_observations() {
        for observations in [(false, true, true), (true, false, true), (true, true, false)] {
            assert!(FixtureCompletion::observe(observations.0, observations.1, observations.2).is_err());
        }
        assert!(FixtureCompletion::observe(true, true, true).is_ok());
    }
}
