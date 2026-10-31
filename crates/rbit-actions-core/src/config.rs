//! `[actions]`: CI/CD on rbit's own runner, `rbit-runner`.
//!
//! ```toml
//! [actions]
//! # CI/CD with rbit-runner. Off by default; when off, Actions is fully
//! # absent: no pages, no runner endpoint, no background work, workflow files are ignored.
//! # Startup default; administrator Plugins controls persist a runtime override.
//! enabled = false
//! # Turn Actions on for repositories created from now on (existing ones keep their setting).
//! enable_for_new_repositories = false
//! # Encryption key for secrets; empty = <data_dir>/actions/secrets.key. Back it up separately.
//! secrets_key_file = ""
//! log_retention_days = 90
//! run_retention_days = 365
//! max_log_bytes = 67108864
//! max_workflow_bytes = 262144
//! # Fail a task with no heartbeat, a task running too long, cancel a job nobody picks up.
//! zombie_timeout_seconds = 600
//! endless_timeout_seconds = 10800
//! abandoned_timeout_seconds = 86400
//! # Workflow evaluations (Git reads + YAML) running at once.
//! evaluation_concurrency = 2
//! # Let instance runners serve every owner; when off, only owners an administrator allowed.
//! instance_runners_for_all = false
//! # Log bytes kept across the instance; beyond it the oldest finished runs lose their logs.
//! max_log_storage_bytes = 10737418240
//! # Waiting jobs per repository; runs beyond it fail with a readable error.
//! max_waiting_jobs_per_repository = 256
//! # Runs and jobs one push or pull request event may create.
//! max_runs_per_event = 32
//! max_jobs_per_event = 1024
//! # `schedule` entries per repository.
//! max_schedules_per_repository = 20
//! # Largest artifact one job may upload, artifact bytes kept per repository, and days they
//! # are kept (a workflow's `retention-days` may only lower it). Artifacts are not backed up.
//! max_artifact_bytes = 2147483648
//! max_artifact_storage_bytes = 10737418240
//! artifact_retention_days = 90
//! # `actions/cache` bytes kept per repository for all its runners (the least recently used
//! # entries go first), and days an entry is kept unused. Caches are not backed up.
//! max_cache_storage_bytes = 10737418240
//! cache_retention_days = 7
//! # How long a runner's poll for work is held open, in seconds (at most 25).
//! long_poll_seconds = 25
//! # Runner polls held open at once across the instance; beyond it polls are refused (503).
//! max_polls = 1024
//! ```
//!
//! This is the startup default until an administrator saves runtime intent through Plugins.
//! Explicit runtime intent survives restarts and overrides this default. Other configuration
//! changes still require restart. Repositories are switched on individually in their settings.
//!
//! The Actions foundation (workstream F of `docs/design/actions.md`) owns this file.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

/// Host-provided location for Actions files. Implementing this small interface keeps
/// configuration independent of the forge host and its storage configuration.
pub trait ActionsStorage {
    /// Directory containing Actions logs, artifacts, caches and the default secrets key.
    fn actions_dir(&self) -> PathBuf;
}

/// Bounds of [`ActionsConfig::evaluation_concurrency`].
pub const EVALUATION_CONCURRENCY: std::ops::RangeInclusive<usize> = 1..=16;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionsConfig {
    /// Startup default, overridden by persisted administrator plugin intent.
    pub enabled: bool,
    /// Default of the per-repository switch for repositories created from now on.
    pub enable_for_new_repositories: bool,
    /// Key file for encrypted secrets; empty means `<data_dir>/actions/secrets.key`.
    pub secrets_key_file: PathBuf,
    /// Days job logs are kept.
    pub log_retention_days: u32,
    /// Days finished runs are kept.
    pub run_retention_days: u32,
    /// Log bytes kept per task; later lines are acknowledged but dropped.
    pub max_log_bytes: u64,
    /// Largest workflow file read, in bytes.
    pub max_workflow_bytes: u64,
    /// A running task without a heartbeat for this long fails, in seconds.
    pub zombie_timeout_seconds: u64,
    /// A task running for this long fails, in seconds.
    pub endless_timeout_seconds: u64,
    /// A waiting job no runner picks up for this long is cancelled, in seconds.
    pub abandoned_timeout_seconds: u64,
    /// Workflow evaluations (Git reads and YAML parsing) running at once.
    pub evaluation_concurrency: usize,
    /// Instance runners serve every owner, not only those an administrator allowed.
    pub instance_runners_for_all: bool,
    /// Log bytes kept across the instance; the oldest finished runs' logs are pruned first.
    pub max_log_storage_bytes: u64,
    /// Waiting jobs per repository; new runs beyond it fail with a readable error.
    pub max_waiting_jobs_per_repository: u32,
    /// Runs one event (a push, a pull request update) may create.
    pub max_runs_per_event: u32,
    /// Jobs one event may create across its runs.
    pub max_jobs_per_event: u32,
    /// `schedule` entries per repository.
    pub max_schedules_per_repository: u32,
    // Actions: S.
    /// How long a runner's poll is held open, in seconds; at most [`MAX_LONG_POLL_SECONDS`].
    pub long_poll_seconds: u64,
    /// Polls held open at once; beyond it a poll is answered "unavailable".
    pub max_polls: usize,
    // Actions: artifacts (A).
    /// Largest artifact a job may upload, in bytes.
    pub max_artifact_bytes: u64,
    /// Artifact bytes kept per repository; an upload beyond it is refused.
    pub max_artifact_storage_bytes: u64,
    /// Days artifacts are kept; a workflow's `retention-days` may only lower it.
    pub artifact_retention_days: u32,
    // Actions: shared cache.
    /// `actions/cache` bytes kept per repository; saving beyond it evicts the least
    /// recently used entries.
    pub max_cache_storage_bytes: u64,
    /// Days a cache entry is kept without being restored.
    pub cache_retention_days: u32,
}

// Actions: S.
/// The longest poll the runner protocol allows (`rbit_workflow::protocol::MAX_LONG_POLL`).
pub const MAX_LONG_POLL_SECONDS: u64 = 25;

impl Default for ActionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            enable_for_new_repositories: false,
            secrets_key_file: PathBuf::new(),
            log_retention_days: 90,
            run_retention_days: 365,
            max_log_bytes: 64 * 1024 * 1024,
            max_workflow_bytes: 256 * 1024,
            zombie_timeout_seconds: 10 * 60,
            endless_timeout_seconds: 3 * 60 * 60,
            abandoned_timeout_seconds: 24 * 60 * 60,
            evaluation_concurrency: 2,
            instance_runners_for_all: false,
            max_log_storage_bytes: 10 * 1024 * 1024 * 1024,
            max_waiting_jobs_per_repository: 256,
            max_runs_per_event: 32,
            max_jobs_per_event: 1024,
            max_schedules_per_repository: 20,
            // Actions: S.
            long_poll_seconds: MAX_LONG_POLL_SECONDS,
            max_polls: 1024,
            // Actions: artifacts (A).
            max_artifact_bytes: 2 * 1024 * 1024 * 1024,
            max_artifact_storage_bytes: 10 * 1024 * 1024 * 1024,
            artifact_retention_days: 90,
            // Actions: shared cache. GitHub's limits.
            max_cache_storage_bytes: 10 * 1024 * 1024 * 1024,
            cache_retention_days: 7,
        }
    }
}

impl ActionsConfig {
    /// Checks the values a typo could make meaningless.
    pub fn validate(&self) -> Result<(), String> {
        if [self.zombie_timeout_seconds, self.endless_timeout_seconds, self.abandoned_timeout_seconds].contains(&0) {
            return Err("Actions timeouts must not be zero".into());
        }
        if self.log_retention_days == 0 || self.run_retention_days == 0 {
            return Err("Actions retention periods must not be zero".into());
        }
        if self.max_log_bytes == 0 || self.max_workflow_bytes == 0 || self.max_log_storage_bytes < self.max_log_bytes {
            return Err(
                "Actions log and workflow size limits must not be zero, and the instance log budget must hold one task's log".into()
            );
        }
        if [self.max_waiting_jobs_per_repository, self.max_runs_per_event, self.max_jobs_per_event, self.max_schedules_per_repository]
            .contains(&0)
        {
            return Err("Actions run, job and schedule caps must not be zero".into());
        }
        // Actions: S.
        if !(1..=MAX_LONG_POLL_SECONDS).contains(&self.long_poll_seconds) {
            return Err(format!("the Actions long poll must be between 1 and {MAX_LONG_POLL_SECONDS} seconds"));
        }
        if self.max_polls == 0 {
            return Err("Actions must allow at least one runner poll".into());
        }
        // Actions: artifacts (A).
        if self.max_artifact_bytes == 0 || self.max_artifact_storage_bytes < self.max_artifact_bytes || self.artifact_retention_days == 0 {
            return Err("Actions artifact limits must not be zero, and a repository's artifact storage must hold one artifact".into());
        }
        // Actions: shared cache.
        if self.max_cache_storage_bytes == 0 || self.cache_retention_days == 0 {
            return Err("Actions cache storage and retention must not be zero".into());
        }
        if !EVALUATION_CONCURRENCY.contains(&self.evaluation_concurrency) {
            return Err(format!(
                "Actions evaluation concurrency must be between {} and {}",
                EVALUATION_CONCURRENCY.start(),
                EVALUATION_CONCURRENCY.end()
            ));
        }
        Ok(())
    }

    /// The secrets key file: [`Self::secrets_key_file`], or `<actions_dir>/secrets.key`.
    pub fn secrets_key_path(&self, storage: &(impl ActionsStorage + ?Sized)) -> PathBuf {
        if self.secrets_key_file.as_os_str().is_empty() { storage.actions_dir().join("secrets.key") } else { self.secrets_key_file.clone() }
    }

    /// [`Self::zombie_timeout_seconds`] as a duration.
    pub fn zombie_timeout(&self) -> Duration {
        Duration::from_secs(self.zombie_timeout_seconds)
    }

    /// [`Self::endless_timeout_seconds`] as a duration.
    pub fn endless_timeout(&self) -> Duration {
        Duration::from_secs(self.endless_timeout_seconds)
    }

    // Actions: S.
    /// [`Self::long_poll_seconds`] as a duration.
    pub fn long_poll(&self) -> Duration {
        Duration::from_secs(self.long_poll_seconds)
    }

    /// [`Self::abandoned_timeout_seconds`] as a duration.
    pub fn abandoned_timeout(&self) -> Duration {
        Duration::from_secs(self.abandoned_timeout_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestStorage(PathBuf);

    impl ActionsStorage for TestStorage {
        fn actions_dir(&self) -> PathBuf {
            self.0.join("actions")
        }
    }

    #[test]
    fn off_by_default_with_documented_defaults() {
        let config = ActionsConfig::default();
        assert!(!config.enabled && !config.enable_for_new_repositories);
        assert_eq!((config.log_retention_days, config.run_retention_days), (90, 365));
        assert_eq!((config.max_log_bytes, config.max_workflow_bytes), (67_108_864, 262_144));
        assert_eq!(config.zombie_timeout(), Duration::from_mins(10));
        assert_eq!(config.endless_timeout(), Duration::from_hours(3));
        assert_eq!(config.abandoned_timeout(), Duration::from_hours(24));
        assert_eq!(config.evaluation_concurrency, 2);
        assert!(!config.instance_runners_for_all);
        assert_eq!(config.max_log_storage_bytes, 10_737_418_240);
        assert_eq!(config.max_waiting_jobs_per_repository, 256);
        assert_eq!((config.max_runs_per_event, config.max_jobs_per_event), (32, 1024));
        assert_eq!(config.max_schedules_per_repository, 20);
        assert_eq!((config.long_poll(), config.max_polls), (Duration::from_secs(25), 1024));
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn key_file_defaults_into_the_actions_directory() {
        let storage = TestStorage(PathBuf::from("data"));
        assert_eq!(storage.actions_dir(), PathBuf::from("data").join("actions"));
        let config = ActionsConfig::default();
        assert_eq!(config.secrets_key_path(&storage), PathBuf::from("data").join("actions").join("secrets.key"));
        let custom = ActionsConfig { secrets_key_file: PathBuf::from("/run/credentials/rbit/key"), ..config };
        assert_eq!(custom.secrets_key_path(&storage), PathBuf::from("/run/credentials/rbit/key"));
    }

    #[test]
    fn validation_rejects_meaningless_values() {
        let valid = |change: fn(&mut ActionsConfig)| {
            let mut config = ActionsConfig::default();
            change(&mut config);
            config.validate().is_ok()
        };
        assert!(valid(|c| c.evaluation_concurrency = 1));
        assert!(valid(|c| c.evaluation_concurrency = 16));
        assert!(!valid(|c| c.zombie_timeout_seconds = 0));
        assert!(!valid(|c| c.endless_timeout_seconds = 0));
        assert!(!valid(|c| c.abandoned_timeout_seconds = 0));
        assert!(!valid(|c| c.log_retention_days = 0));
        assert!(!valid(|c| c.run_retention_days = 0));
        assert!(!valid(|c| c.max_log_bytes = 0));
        assert!(!valid(|c| c.max_workflow_bytes = 0));
        assert!(!valid(|c| c.max_log_storage_bytes = c.max_log_bytes - 1));
        assert!(!valid(|c| c.max_waiting_jobs_per_repository = 0));
        assert!(!valid(|c| c.max_runs_per_event = 0));
        assert!(!valid(|c| c.max_jobs_per_event = 0));
        assert!(!valid(|c| c.max_schedules_per_repository = 0));
        assert!(valid(|c| c.long_poll_seconds = 1));
        assert!(!valid(|c| c.long_poll_seconds = 0));
        assert!(!valid(|c| c.long_poll_seconds = 26));
        assert!(!valid(|c| c.max_polls = 0));
        assert!(!valid(|c| c.evaluation_concurrency = 0));
        assert!(!valid(|c| c.evaluation_concurrency = 17));
    }

    #[test]
    fn deserializes_from_partial_sections_and_rejects_unknown_keys() {
        let config: ActionsConfig = serde_json::from_str(r#"{"enabled": true}"#).unwrap();
        assert!(config.enabled && config.evaluation_concurrency == 2);
        assert!(serde_json::from_str::<ActionsConfig>(r#"{"enable": true}"#).is_err());
    }
}
