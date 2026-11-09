//! # rbit-storage
//!
//! Two embedded stores, each used for what it is best at:
//!
//! * [`sqlite`] — relational, constrained, migrated data (accounts, repositories, access tokens, …).
//! * [`kv`] — redb key/value tables for sessions and rebuildable caches.
//!
//! Credentials the server must read back are [sealed](sealing) under a key kept outside the database.
//!
//! Stores speak in `rbit-core` domain types; SQL rows never leak out of this crate.

mod access_collaborators;
mod access_organizations;
mod access_protection;
mod access_teams;
mod collaboration_comments;
mod collaboration_labels;
mod collaboration_milestones;
mod collaboration_thread_lifecycle;
mod collaboration_thread_state;
mod collaboration_thread_text;
mod collaboration_threads;
pub mod error;
mod identity_access_tokens;
mod identity_accounts;
mod identity_epoch_sessions;
mod identity_profiles;
mod identity_recovery;
pub use identity_recovery::RecoveryCredentials;
mod identity_sessions;
pub mod identity_upgrade;
pub mod kv;
mod notification_inbox;
pub use notification_inbox::NotificationInboxStore;
mod repository_creation;
mod repository_forks;
pub use repository_forks::{ForkCreationStore, ForkReadStore};
mod repository_aliases;
mod repository_discovery;
mod repository_transfer;
pub use repository_transfer::TransferStore;
pub mod sealing;
pub mod sqlite;
mod work;

pub use access_collaborators::CollaboratorAccessStore;
pub use access_organizations::OrganizationAccessStore;
pub use access_teams::TeamAccessStore;
pub use error::StorageError;
pub use kv::{
    AppliedSessionRevocation, CredentialRecoveryGate, CredentialRecoveryPermit, EpochSessionStore, EpochSnapshot, KvStore, SessionRecord,
    SessionStore,
};
pub use sealing::SealingKey;
pub use sqlite::{AccessTokenStore, AccountStore, Database, NewAccessToken, NewRepository, NewUser, RepositoryStore, SessionUser};
pub use work::{CapturedWork, Work, capture_work, count_work};
