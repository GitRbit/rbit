//! Personal access token records and static persistence ports.
//!
//! Identity owns the token lifecycle: issuing, listing, revoking and resolving a secret.
//! The scope is a type parameter because its meaning (capping repository access) belongs
//! to the authorization model; identity stores and returns it without interpreting it.

mod records;
mod repository;

pub use records::{AccessTokenId, AccessTokenRecord, NewAccessTokenRecord};
pub use repository::{AccessTokenOwnerRepository, AccessTokenRepository, ResolvedAccessToken};
