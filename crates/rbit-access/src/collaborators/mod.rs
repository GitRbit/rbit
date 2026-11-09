//! Repository collaborators: who may manage them and the refusals their changes get.
//!
//! Rules:
//! * Repository administrators add, change and remove collaborators; a collaborator may
//!   always remove themselves.
//! * Organisations, suspended users and the personal owner of the repository cannot be
//!   added. One entry per user and repository: the store settles that atomically.
//! * Setting a permission a collaborator already has changes nothing.

mod application;
mod port;

pub use application::{CollaboratorScope, Collaborators, SetOutcome};
pub use port::CollaboratorRepository;

#[cfg(test)]
mod tests;
