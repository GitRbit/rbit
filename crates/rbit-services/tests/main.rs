//! All of rbit-services's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

mod access;
// Actions: foundation (F).
mod actions;
// Actions: triggers (T).
mod actions_conditions;
mod actions_dispatch;
#[path = "support/actions_fixture.rs"]
mod actions_fixture;
mod actions_hooks;
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
mod audit;
mod edits;
mod external_auth;
// The fake GitHub that imports and mirrors fetch from.
#[path = "support/fake_forge.rs"]
mod fake_forge;
mod forks;
mod imports;
mod issues;
mod lfs;
mod mirrors;
mod notification_email;
mod notifications;
mod orgs;
mod passkeys;
mod preferences;
mod profiles;
mod protection;
mod pulls;
mod push_mirrors;
mod recovery;
mod references;
mod releases;
mod repo_settings;
mod sessions;
mod statuses;
mod teams;
mod timeline;
mod tokens;
mod transfers;
mod two_factor;
mod webhooks;
