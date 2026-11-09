//! Commit statuses: what external CI reports about a commit, and the decisions made from
//! the reports — a commit's combined state ([`combined_state`]) and whether the required
//! checks of a protected branch pass ([`required_checks`]).
//!
//! The model is GitHub's commit status API, so existing CI integrations work unchanged:
//! * a status has a `state` (`pending`, `success`, `failure`, `error`), a `context` that
//!   names the check (default `default`), an optional `description` (≤
//!   [`MAX_DESCRIPTION_CHARS`] characters) and an optional `target_url` (an absolute
//!   `http`/`https` URL);
//! * every report is kept; for each `(repository, commit, context)` the **latest** report
//!   wins, and at most [`MAX_STATUSES_PER_CONTEXT`] reports are kept per triple;
//! * the combined state is `failure` if any latest status is `failure` or `error`,
//!   `pending` if there are none or any is `pending`, and `success` otherwise.
//!
//! A status is posted by a person (through the API) or by rbit Actions for its jobs
//! ([`StatusSource`]); Actions' statuses have no creator. With the `applications`
//! feature, `StatusApplication` decides who may post and in which order a submission is
//! checked, over the `CommitStatusRepository` port.

mod checks;
mod models;

pub use checks::{CombinedStatus, RequiredChecks, StatusPage, combined_state, creator_ids, required_checks};
pub use models::{
    CommitStatus, DEFAULT_CONTEXT, MAX_CONTEXT_CHARS, MAX_DESCRIPTION_CHARS, MAX_REQUIRED_CONTEXTS, MAX_STATUSES_PER_CONTEXT,
    StatusContext, StatusError, StatusId, StatusSource, StatusState, parse_sha,
};

#[cfg(feature = "applications")]
mod application;
#[cfg(feature = "applications")]
mod port;
#[cfg(feature = "applications")]
pub use application::{StatusAccess, StatusApplication, StatusApplicationError};
#[cfg(feature = "applications")]
pub use port::{CommitStatusRepository, StatusAuthor};
