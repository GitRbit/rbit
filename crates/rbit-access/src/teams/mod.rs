//! Organisation teams: who may see and manage them, their members and repository grants.
//!
//! Rules:
//! * Teams, their members and their repositories are visible to members of the
//!   organisation and to site administrators; everyone else is told they do not exist.
//! * Owners and site administrators create, rename and delete teams and manage their
//!   members and repositories. Members may leave a team themselves.
//! * Team members must be organisation members, and a team is only granted repositories
//!   of its own organisation: naming another owner is refused, not redirected. The
//!   [`TeamRepository`] adapter enforces both atomically with the write.

mod application;
mod port;

pub use application::{OpenedTeam, TeamInput, Teams};
pub use port::{RepositoryRef, TeamChange, TeamRepository, Unique};

#[cfg(test)]
mod tests;
