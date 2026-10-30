//! All of rbit-services's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

mod access;
#[cfg(not(feature = "actions"))]
mod actions_unavailable;
// Actions: foundation (F).
#[cfg(feature = "actions")]
mod actions;
#[cfg(feature = "actions")]
mod actions_plugin;
// Actions: triggers (T).
// Actions: workflow_run and repository_dispatch.
#[cfg(feature = "actions")]
mod actions_calls;
#[cfg(feature = "actions")]
mod actions_chains;
#[cfg(feature = "actions")]
mod actions_conditions;
#[cfg(feature = "actions")]
mod actions_dispatch;
#[cfg(feature = "actions")]
#[path = "support/actions_fixture.rs"]
mod actions_fixture;
#[cfg(feature = "actions")]
mod actions_hooks;
#[cfg(feature = "actions")]
mod actions_needs_plans;
#[cfg(feature = "actions")]
mod actions_planning;
#[cfg(feature = "actions")]
mod actions_pulls;
#[cfg(feature = "actions")]
mod actions_pushes;
#[cfg(feature = "actions")]
mod actions_runs;
#[cfg(feature = "actions")]
mod actions_secrets;
// Actions: interface and secrets (U).
#[cfg(feature = "actions")]
mod actions_settings;
// Actions: S.
#[cfg(feature = "actions")]
mod actions_runners;
#[cfg(feature = "actions")]
mod actions_statuses;
#[cfg(feature = "actions")]
mod actions_sweeper;
#[cfg(feature = "actions")]
mod actions_tokens;
mod admin;
mod archives;
mod audit;
mod edits;
mod email_digests;
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
