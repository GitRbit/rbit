//! Pure authorization rules, independent of identity records and persistence.
//!
//! Callers resolve current, repository-scoped grants and authenticated actor facts.
//! This crate combines those facts; it never authenticates credentials, loads grants,
//! or decides whether a previously cached grant is still valid. Organisation, team,
//! collaborator and branch protection rules reach records only through their ports;
//! storage keeps race authority.

pub mod collaborators;
pub mod grants;
pub mod organizations;
mod policy;
pub mod protection;
mod repository_access;
pub mod teams;
mod token_scope;

pub use organizations::{OrgCreationPolicy, OrgRole};
pub use policy::{OrganizationAccess, RepositoryAccessFacts, ViewerAccessFacts, repo_access};
pub use repository_access::RepoAccess;
pub use token_scope::TokenScope;

#[cfg(test)]
mod tests;
