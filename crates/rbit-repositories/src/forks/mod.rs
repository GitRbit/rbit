//! Fork creation, visibility queries and private-ancestor policy; native/SQL ports stay concrete.
//!
//! Rules:
//! * Anyone who can read a repository may fork it, into their own account or an
//!   organization where they may create repositories (the shared creation-owner rule).
//! * The fork keeps the parent's visibility (a private parent yields a private fork),
//!   description and default branch; its branches and tags are copied on disk.
//! * A fork cannot be made public while its parent, or any repository further up the
//!   fork network, is private ([`ForkQueries::has_private_ancestor`]): that would publish
//!   the private history. Forks are not removed when someone loses access to the parent.
//! * Metadata precedes copying. Copy/relationship errors attempt disk and SQL rollback;
//!   cancellation or failed cleanup can still require repair.
//! * Forking publishes a fork event, never a creation event.
//! * Deleting the parent leaves its forks intact; they simply lose their parent.
//! * The readable fork network ([`ForkQueries::network`]) bounds cross-fork pull heads
//!   and shared LFS objects to repositories the viewer may read.
mod creation;
mod port;
mod queries;
pub use creation::{ForkCreation, ForkCreationError};
pub use port::{ForkCreationPort, ForkCreationRepository, ForkParentAccess, ForkReadRepository};
pub use queries::{ForkParentError, ForkQueries, ForkViewer};
/// Forks listed on the forks page.
pub const MAX_LISTED_FORKS: u32 = 100;
/// Raw fork form input; empty owner/name retain actor/source defaults.
#[derive(Debug, Clone, Default)]
pub struct ForkInput {
    pub owner: String,
    pub name: String,
}
