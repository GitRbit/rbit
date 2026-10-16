//! # rustit-core
//!
//! The domain model of Rustit. Everything here is pure data and pure logic: no I/O,
//! no async, no framework types. Other crates depend on these types so that a
//! `RepoName` validated at the HTTP boundary is the same `RepoName` that reaches the
//! filesystem layer — validation happens exactly once, at construction.

pub mod access;
pub mod access_token;
pub mod account;
pub mod config;
pub mod instance;
pub mod names;
pub mod org;
pub mod repository;
pub mod timestamp;

pub use access::RepoAccess;
pub use access_token::{AccessToken, AccessTokenId, TokenScope};
pub use account::{Account, AccountId, AccountKind, User};
pub use config::Config;
pub use instance::{InstanceSettings, OrgCreationPolicy, RegistrationMode};
pub use names::{AccountName, Email, NameError, RepoName};
pub use org::{OrgMember, OrgMembership, OrgRole, Organization};
pub use repository::{OwnedRepository, RepoId, RepoSort, Repository, Visibility};
pub use timestamp::Timestamp;
