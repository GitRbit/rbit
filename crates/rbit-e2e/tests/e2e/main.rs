//! End-to-end tests: every test boots its own server (see `rbit_e2e::TestServer`) and
//! drives it over HTTP and with the real `git` CLI. One test binary keeps link times
//! down; each area is a module.

mod access;
mod accounts;
mod api;
mod api_workflow;
mod compatibility;
mod edge;
mod edits;
mod essentials;
mod harness;
mod issues;
mod lfs;
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
