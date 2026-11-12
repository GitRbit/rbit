//! End-to-end tests: every test boots its own server (see `rbit_e2e::TestServer`) and
//! drives it over HTTP and with the real `git` CLI. One test binary keeps link times
//! down; each area is a module.

mod access;
mod accounts;
// Actions: foundation (F).
mod actions_container;
mod actions_disabled;
mod actions_runner;
// Agent plan step 1: runs by commit, error codes, the CLI's output contract.
mod agent_exclusive;
mod agent_land;
mod agent_paths;
mod agent_runs;
mod agent_scratch;
mod agent_supersede;
mod agent_termination;
mod agent_tokens;
mod api;
mod api_workflow;
mod archives;
mod compatibility;
mod edge;
mod edits;
mod email_digests;
mod essentials;
mod harness;
mod issues;
mod lfs;
mod limits;
mod notification_email;
mod orgs;
mod protection;
mod pull_base;
mod pulls;
mod redirects;
mod repositories;
mod review_threads;
mod security;
mod stars;
mod themes;
mod timeline;
mod tokens;
mod visibility;
mod workflow;
// Milestone 3: commit statuses and required checks (S).
mod statuses;
// Milestone 3: external sign-in against fake providers (O).
mod external_auth;
// Milestone 3: importing and push mirrors (M).
mod imports;
// Frontend wave: deferred slots (S).
mod slots;
// Frontend wave: partial responses and preloading (P).
mod navigation;
