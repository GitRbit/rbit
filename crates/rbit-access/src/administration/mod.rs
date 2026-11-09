//! Site administration: who may administer the instance, the guard rails that keep it
//! manageable, and listing normalisation.
//!
//! Rules:
//! * Every operation needs a site administrator; everyone else is `Forbidden`.
//! * Administrators cannot demote, suspend or delete themselves, and the last active
//!   administrator can never be demoted, suspended or deleted (the store settles that in
//!   the same statement as the write).
//! * Deleting a user needs their exact username and must not leave an organisation
//!   without an owner; deleting a repository needs its exact `owner/name`.
//! * Listing searches are trimmed and capped; pages start at 1.
//!
//! Disk removal, events, audit entries and the restart-required configuration editor
//! stay with the caller.

mod application;
mod listing;
mod port;

pub use application::{Administration, DeletedUser, describe_changes, require_site_admin};
pub use listing::{ListFilter, ListQuery, Listing, MAX_SEARCH_LEN, PAGE_SIZE, Paged};
pub use port::{AdministrationRepository, Moderation};

#[cfg(test)]
mod tests;
