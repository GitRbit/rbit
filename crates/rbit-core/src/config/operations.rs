//! `[operations]`: background maintenance, metrics and backups.
//!
//! Milestone 1 workstream O (operations) owns this file.
//!
//! ```toml
//! [operations]
//! metrics = true
//! metrics_token = "a-long-random-string"   # optional: require `Authorization: Bearer …`
//! maintenance_interval_minutes = 360       # `git gc --auto` + cache pruning
//! job_concurrency = 2
//! insights_cache_max_age_days = 30
//! insights_cache_max_entries = 50000
//! ```

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OperationsConfig {
    /// Expose Prometheus metrics at `/metrics`.
    pub metrics: bool,
    /// When set, `/metrics` requires `Authorization: Bearer <token>`. Leave unset only if
    /// the endpoint is not reachable from untrusted networks.
    pub metrics_token: Option<String>,
    /// Warm the language and last-commit caches of the default branch after each push, so
    /// the first visitor after a push does not pay for the computation.
    pub warm_caches_on_push: bool,
    /// Minutes between maintenance runs (`git gc --auto` for repositories pushed to since
    /// the last run, then insights cache pruning). `0` disables periodic maintenance.
    pub maintenance_interval_minutes: u64,
    /// Upper bound on background jobs (cache warming, `git gc`) running at the same time.
    pub job_concurrency: usize,
    /// Insights cache entries older than this are removed by maintenance. `0` keeps
    /// entries regardless of age.
    pub insights_cache_max_age_days: u64,
    /// When the insights cache holds more entries than this, maintenance removes the
    /// oldest. `0` means no limit.
    pub insights_cache_max_entries: u64,
}

impl Default for OperationsConfig {
    fn default() -> Self {
        Self {
            metrics: false,
            metrics_token: None,
            warm_caches_on_push: true,
            maintenance_interval_minutes: 6 * 60,
            job_concurrency: 2,
            insights_cache_max_age_days: 30,
            insights_cache_max_entries: 50_000,
        }
    }
}

impl OperationsConfig {
    /// The pause between maintenance runs, or `None` when maintenance is disabled.
    pub fn maintenance_interval(&self) -> Option<Duration> {
        (self.maintenance_interval_minutes > 0).then(|| Duration::from_secs(self.maintenance_interval_minutes.saturating_mul(60)))
    }

    /// Concurrent background jobs; at least one.
    pub fn job_concurrency(&self) -> usize {
        self.job_concurrency.max(1)
    }

    /// The age after which insights cache entries are pruned, if any.
    pub fn insights_cache_max_age(&self) -> Option<Duration> {
        (self.insights_cache_max_age_days > 0).then(|| Duration::from_secs(self.insights_cache_max_age_days.saturating_mul(24 * 60 * 60)))
    }

    /// The configured metrics bearer token, ignoring an empty value.
    pub fn metrics_token(&self) -> Option<&str> {
        self.metrics_token.as_deref().map(str::trim).filter(|token| !token.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_disables_periodic_work() {
        let config =
            OperationsConfig { maintenance_interval_minutes: 0, insights_cache_max_age_days: 0, job_concurrency: 0, ..Default::default() };
        assert_eq!(config.maintenance_interval(), None);
        assert_eq!(config.insights_cache_max_age(), None);
        assert_eq!(config.job_concurrency(), 1);
    }

    #[test]
    fn defaults_are_conservative() {
        let config = OperationsConfig::default();
        assert!(!config.metrics);
        assert_eq!(config.maintenance_interval(), Some(Duration::from_hours(6)));
        assert_eq!(config.metrics_token(), None);
        assert_eq!(OperationsConfig { metrics_token: Some("  ".into()), ..config }.metrics_token(), None);
    }
}
