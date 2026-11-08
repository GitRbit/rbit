//! All of rbit-services's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

mod access;
mod wiki_reads;
// Actions: foundation (F).
mod actions;
mod actions_claims;
// Actions: triggers (T).
// Actions: workflow_run and repository_dispatch.
mod actions_calls;
mod actions_chains;
mod actions_conditions;
mod actions_dispatch;
#[path = "support/actions_fixture.rs"]
mod actions_fixture;
mod actions_hooks;
mod actions_log_reads;
mod actions_needs_plans;
mod actions_planning;
mod actions_pulls;
mod actions_pushes;
mod actions_runs;
mod actions_secrets;
// Actions: interface and secrets (U).
mod actions_settings;
// Actions: S.
mod actions_runners;
mod actions_statuses;
mod actions_sweeper;
mod actions_tokens;
mod admin;
mod archives;
mod audit;
mod edits;
mod email_digests;
mod external_auth;
// The fake GitHub that imports and mirrors fetch from.
mod epoch_sessions;
#[path = "support/fake_forge.rs"]
mod fake_forge;
#[path = "support/fault_schema.rs"]
mod fault_schema;
mod forks;
// The hostile load regression group: workflow files.
mod hostile_workflows;
mod identity_races;
mod imports;
mod issues;
mod lfs;
mod mirrors;
mod notification_email;
mod notifications;
mod orgs;
mod passkeys;
mod password_admission;
mod password_policy;
mod permission_matrix;
#[path = "support/permission_matrix.rs"]
mod permission_world;
mod preferences;
mod profiles;
mod protection;
mod provider_original;
mod pulls;
mod push_mirrors;
mod recovery;
mod references;
mod releases;
mod repo_settings;
mod repository_discovery;
mod sessions;
mod statuses;
mod teams;
mod timeline;
mod tokens;
mod transfers;
mod two_factor;
mod webhooks;
