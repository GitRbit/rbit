//! All of rbit-web's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

// Shared with rbit-storage: counts the SQL statements a request runs.
#[cfg(feature = "actions")]
#[path = "../../rbit-storage/tests/common/query_count.rs"]
mod query_count;

// Actions: foundation (F).
// Actions: REST API.
#[cfg(feature = "actions")]
mod actions_api;
// Actions: artifacts (A).
#[cfg(feature = "actions")]
mod actions_artifacts;
// Actions: shared cache.
#[cfg(feature = "actions")]
mod actions_caches;
#[cfg(feature = "actions")]
mod actions_disabled;
#[cfg(feature = "actions")]
mod actions_environments;
// Actions: interface and secrets (U).
#[cfg(feature = "actions")]
mod actions_job_pages;
#[cfg(feature = "actions")]
mod actions_pages;
// Actions: settings API.
#[cfg(feature = "actions")]
mod actions_permissions;
#[cfg(feature = "actions")]
mod actions_settings;
// Actions: S.
#[cfg(feature = "actions")]
mod actions_runner;
#[cfg(feature = "actions")]
mod actions_runner_pages;
#[cfg(feature = "actions")]
mod actions_runner_tasks;
#[cfg(feature = "actions")]
mod actions_tokens;
mod admin_configuration;
mod archives;
mod contributions;
mod listings;
mod markdown_preview;
mod mirror_progress;
mod notification_settings;
mod plugins;
mod properties;
mod repository_documents;
mod security;
mod work_counts;
