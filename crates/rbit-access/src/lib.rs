//! Pure authorization rules, independent of identity records and persistence.
//!
//! Callers resolve current, repository-scoped grants and authenticated actor facts.
//! This crate combines those facts; it never authenticates credentials, loads grants,
//! or decides whether a previously cached grant is still valid. Organisation and team
//! rules reach records only through their ports; storage keeps race authority.

pub mod organizations;
mod policy;
mod repository_access;
pub mod teams;

pub use organizations::{OrgCreationPolicy, OrgRole};
pub use policy::{OrganizationAccess, RepositoryAccessFacts, ViewerAccessFacts, repo_access};
pub use repository_access::RepoAccess;

#[cfg(test)]
mod tests;
