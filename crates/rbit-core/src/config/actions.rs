//! `[actions]`: CI/CD on external Forgejo or Gitea runners.
//!
//! ```toml
//! [actions]
//! # CI/CD on external Forgejo or Gitea runners. Off by default; when off, Actions is fully
//! # absent: no pages, no runner endpoint, no background work, workflow files are ignored.
//! # Changing it needs a restart.
//! enabled = false
//! # Turn Actions on for repositories created from now on (existing ones keep their setting).
//! enable_for_new_repositories = false
//! # Where `uses: owner/repo@ref` actions are fetched from: a URL, or "self" for this instance.
//! default_actions_url = "https://data.forgejo.org"
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
//! ```
//!
//! The switch is read once at start-up: with `enabled = false` the Actions service is never
//! constructed, so nothing is written under [`StorageConfig::actions_dir`] or to any
//! `actions_*` table. Repositories are then switched on one by one in their settings.
//!
//! The Actions foundation (workstream F of `docs/design/actions.md`) owns this file.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use super::StorageConfig;

/// The `default_actions_url` value that means "this instance's own public URL".
pub const SELF_ACTIONS_URL: &str = "self";

/// Bounds of [`ActionsConfig::evaluation_concurrency`].
pub const EVALUATION_CONCURRENCY: std::ops::RangeInclusive<usize> = 1..=16;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionsConfig {
    /// Run Actions at all. Read once at start-up; a change needs a restart.
    pub enabled: bool,
    /// Default of the per-repository switch for repositories created from now on.
    pub enable_for_new_repositories: bool,
    /// Where runners fetch `uses: owner/repo@ref` actions from: an HTTP(S) URL, or `self`.
    pub default_actions_url: String,
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
}

impl Default for ActionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            enable_for_new_repositories: false,
            default_actions_url: "https://data.forgejo.org".to_owned(),
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
        }
    }
}

impl ActionsConfig {
    /// Checks the values a typo could make meaningless. The URL is checked only for its
    /// shape here; `rbit_services::admin::configuration` parses it fully.
    pub fn validate(&self) -> Result<(), String> {
        let url = self.default_actions_url.as_str();
        let remote = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
        if url != SELF_ACTIONS_URL && remote.is_none_or(|rest| rest.is_empty() || rest.starts_with('/')) {
            return Err("the default actions URL must be an HTTP or HTTPS URL, or \"self\"".into());
        }
        if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("the default actions URL must not contain spaces or control characters".into());
        }
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
    pub fn secrets_key_path(&self, storage: &StorageConfig) -> PathBuf {
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

    /// [`Self::abandoned_timeout_seconds`] as a duration.
    pub fn abandoned_timeout(&self) -> Duration {
        Duration::from_secs(self.abandoned_timeout_seconds)
    }
}

impl StorageConfig {
    /// Actions state outside SQLite: `<actions_dir>/logs/<repo_id>/<task_id>.log` and the
    /// default secrets key. Never backed up; created only when Actions is enabled.
    pub fn actions_dir(&self) -> PathBuf {
        self.data_dir.join("actions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_with_documented_defaults() {
        let config = ActionsConfig::default();
        assert!(!config.enabled && !config.enable_for_new_repositories);
        assert_eq!(config.default_actions_url, "https://data.forgejo.org");
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
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn key_file_defaults_into_the_actions_directory() {
        let storage = StorageConfig { data_dir: PathBuf::from("data") };
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
        assert!(valid(|c| c.default_actions_url = "self".into()));
        assert!(valid(|c| c.default_actions_url = "http://git.internal:3000/".into()));
        assert!(valid(|c| c.evaluation_concurrency = 1));
        assert!(valid(|c| c.evaluation_concurrency = 16));
        assert!(!valid(|c| c.default_actions_url = String::new()));
        assert!(!valid(|c| c.default_actions_url = "Self".into()));
        assert!(!valid(|c| c.default_actions_url = "ftp://example.com".into()));
        assert!(!valid(|c| c.default_actions_url = "https://".into()));
        assert!(!valid(|c| c.default_actions_url = "https:///path".into()));
        assert!(!valid(|c| c.default_actions_url = "https://exa mple.com".into()));
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
