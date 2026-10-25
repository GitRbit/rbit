//! All of rbit-web's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

// Shared with rbit-storage: counts the SQL statements a request runs.
#[path = "../../rbit-storage/tests/common/query_count.rs"]
mod query_count;

// Actions: foundation (F).
mod actions_disabled;
// Actions: interface and secrets (U).
mod actions_job_pages;
mod actions_pages;
mod actions_settings;
// Actions: S.
mod actions_runner;
mod actions_runner_tasks;
mod actions_tokens;
mod admin_configuration;
mod contributions;
mod listings;
mod markdown_preview;
mod mirror_progress;
mod properties;
mod repository_documents;
mod security;
