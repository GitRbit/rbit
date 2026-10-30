//! Serializable lifecycle status and activation identities.

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

/// Generation zero is reserved and cannot identify an activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("plugin generation must be nonzero")]
pub struct GenerationError;

/// Unique activation identity, assigned by the host rather than by a plugin.
///
/// Hosts must not reuse generation values while old handles or credentials can exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Generation(NonZeroU64);

impl Generation {
    pub fn new(value: u64) -> Result<Self, GenerationError> {
        NonZeroU64::new(value).map(Self).ok_or(GenerationError)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Persisted administrative intent, independent of observed runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    Enabled,
    Disabled,
}

/// Observed state of an installed plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    Installed,
    Waiting,
    Preparing,
    Active,
    Quiescing,
    Disabled,
    Failed,
    RestartRequired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_generation_is_rejected_at_both_construction_boundaries() {
        assert!(Generation::new(0).is_err());
        assert!(serde_json::from_str::<Generation>("0").is_err());
        assert_eq!(serde_json::from_str::<Generation>("18446744073709551615").unwrap().get(), u64::MAX);
        assert_eq!(serde_json::to_string(&Generation::new(42).unwrap()).unwrap(), "42");
    }

    #[test]
    fn administrative_intent_and_observed_state_have_stable_wire_values() {
        assert_eq!(serde_json::to_string(&DesiredState::Disabled).unwrap(), "\"disabled\"");
        assert_eq!(serde_json::to_string(&PluginState::RestartRequired).unwrap(), "\"restart_required\"");
        assert!(serde_json::from_str::<PluginState>("\"unknown\"").is_err());
    }
}
