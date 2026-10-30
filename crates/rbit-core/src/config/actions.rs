//! Compatibility bridge from the forge storage configuration to the Actions module.
//!
//! Actions configuration is owned by `rbit-actions-core`; existing host callers retain
//! the `rbit_core::config::ActionsConfig` path during the module extraction.

use std::path::PathBuf;

use rbit_actions_core::ActionsStorage;
pub use rbit_actions_core::{ActionsConfig, EVALUATION_CONCURRENCY};

use super::StorageConfig;

impl StorageConfig {
    /// Actions state outside SQLite: logs, artifacts, caches and the default secrets key.
    /// Created only when Actions is enabled; backup policy is defined by the host.
    pub fn actions_dir(&self) -> PathBuf {
        self.data_dir.join("actions")
    }
}

impl ActionsStorage for StorageConfig {
    fn actions_dir(&self) -> PathBuf {
        Self::actions_dir(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_storage_preserves_the_existing_config_api() {
        let storage = StorageConfig { data_dir: PathBuf::from("data"), ..StorageConfig::default() };
        let config = ActionsConfig::default();
        assert_eq!(storage.actions_dir(), PathBuf::from("data").join("actions"));
        assert_eq!(config.secrets_key_path(&storage), storage.actions_dir().join("secrets.key"));
        let custom = ActionsConfig { secrets_key_file: PathBuf::from("/run/credentials/rbit/key"), ..config };
        assert_eq!(custom.secrets_key_path(&storage), PathBuf::from("/run/credentials/rbit/key"));
    }
}
