//! # rbit-core
//!
//! The domain model of rbit. Everything here is pure data and pure logic: no I/O,
//! no async, no framework types. Other crates depend on these types so that a
//! `RepoName` validated at the HTTP boundary is the same `RepoName` that reaches the
//! filesystem layer — validation happens exactly once, at construction.

pub mod access;
pub mod access_token;
pub mod account;
pub mod config;
pub mod instance;
pub mod issue;
pub mod issue_query;
pub mod membership;
pub mod mirror;
pub mod names;
pub mod network;
pub mod org;
pub mod protection;
pub mod pull;
pub mod ref_update;
pub mod references;
pub mod release;
pub mod repository;
// Milestone 3: commit statuses (S).
pub mod status;
pub mod timestamp;
pub mod webhook;
// Milestone 3: editing files in the browser (E).
pub mod file_edit;

pub use access::RepoAccess;
pub use access_token::{AccessToken, AccessTokenId, TokenScope};
pub use account::{Account, AccountId, AccountKind, User};
pub use config::Config;
pub use instance::{InstanceSettings, OrgCreationPolicy, RegistrationMode};
pub use issue::{
    Comment, CommentId, EventId, Issue, IssueId, IssueKind, IssueNumber, IssuePermissions, IssueState, IssueText, IssueTitle, LabelId,
    MilestoneId, ReviewId, StateReason, TimelineEvent,
};
pub use membership::{BasePermission, Permission, TeamName};
pub use names::{AccountName, Email, NameError, RepoName};
pub use org::{OrgMember, OrgMembership, OrgRole, Organization};
pub use ref_update::RefUpdate;
pub use release::ReleaseId;
pub use repository::{OwnedRepository, RepoId, RepoSort, Repository, Visibility};
pub use timestamp::Timestamp;
