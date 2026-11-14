//! All of rbit-storage's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

// Shared by several modules; compiled once so its global tracing subscriber is installed once.
#[path = "common/query_count.rs"]
mod query_count;

mod access_tokens;
mod account_creation;
mod accounts;
// Actions: foundation (F).
mod actions;
// Agent loop: scratch refs.
mod actions_decisions;
mod actions_flaky;
mod actions_progress;
mod actions_scratch;
mod archives;
mod backup;
mod collaborators;
mod credential_epochs;
mod credential_fence_reads;
mod credential_mutations;
mod credential_revocations;
mod email_digests;
mod external_identities;
mod identity_races;
mod imports;
mod issue_meta;
mod issue_search;
mod issues;
mod lfs;
mod locale_preferences;
mod mirrors;
mod notification_email;
mod notifications;
mod orgs;
mod original_proof_writes;
mod preferences;
mod protection;
mod pulls;
mod push_mirrors;
mod readable_parity;
mod recovery;
mod redirects;
mod releases;
mod revocation_recovery;
mod sealing;
mod statuses;
mod teams;
mod two_factor;
mod upgrade;
mod webhooks;
mod writer_compatibility;
#[path = "common/writer_connection.rs"]
mod writer_connection;
