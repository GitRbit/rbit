//! All of rbit-storage's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

// Shared by several modules; compiled once so its global tracing subscriber is installed once.
#[path = "common/query_count.rs"]
mod query_count;

mod access_tokens;
mod accounts;
mod collaborators;
mod external_identities;
mod imports;
mod issue_meta;
mod issue_search;
mod issues;
mod lfs;
mod mirrors;
mod notification_email;
mod notifications;
mod orgs;
mod preferences;
mod protection;
mod pulls;
mod push_mirrors;
mod recovery;
mod redirects;
mod releases;
mod statuses;
mod teams;
mod two_factor;
mod upgrade;
mod webhooks;
