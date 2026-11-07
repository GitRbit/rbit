//! Fork creation, visibility queries and private-ancestor policy; native/SQL ports stay concrete.
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
