//! Pure authorization rules, independent of identity records and persistence.
//!
//! Callers resolve current, repository-scoped grants and authenticated actor facts.
//! This crate combines those facts; it never authenticates credentials, loads grants,
//! or decides whether a previously cached grant is still valid.

mod policy;
mod repository_access;

pub use policy::{OrganizationAccess, RepositoryAccessFacts, ViewerAccessFacts, repo_access};
pub use repository_access::RepoAccess;

#[cfg(test)]
mod tests;
