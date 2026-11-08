//! All of rbit-web's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

// Shared with rbit-storage: counts the SQL statements a request runs.
#[path = "../../rbit-storage/tests/common/query_count.rs"]
mod query_count;

// Actions: foundation (F).
// Actions: REST API.
mod actions_api;
// Actions: artifacts (A).
mod actions_artifacts;
// Actions: shared cache.
mod actions_caches;
mod actions_disabled;
mod actions_environments;
// Actions: interface and secrets (U).
mod actions_job_pages;
mod actions_pages;
// Actions: settings API.
mod actions_permissions;
mod actions_settings;
mod actions_settings_localization;
// Actions: S.
mod actions_runner;
mod actions_runner_pages;
mod actions_runner_tasks;
mod actions_tokens;
mod admin_configuration;
mod archives;
mod contributions;
mod explore_localization;
mod hostile_downloads;
// The hostile load regression group.
mod hostile_load;
mod listings;
mod locale_preferences;
mod markdown_preview;
mod mirror_progress;
mod notification_settings;
mod password_protocols;
mod permission_matrix;
// The permission regression world shared with rbit-services and rbit-ssh.
#[path = "../../rbit-services/tests/support/permission_matrix.rs"]
mod permission_world;
mod properties;
mod repository_documents;
mod security;
mod wiki_pages;
mod work_counts;
