//! All of rbit-services's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

mod access;
mod admin;
mod audit;
mod edits;
mod email_digests;
mod external_auth;
mod forks;
mod imports;
mod issues;
mod lfs;
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
