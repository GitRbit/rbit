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

pub mod error;
pub mod kv;
pub mod sealing;
pub mod sqlite;
mod work;

pub use error::StorageError;
pub use kv::{KvStore, SessionRecord, SessionStore};
pub use sealing::SealingKey;
pub use sqlite::{AccessTokenStore, AccountStore, Database, NewAccessToken, NewRepository, NewUser, RepositoryStore, SessionUser};
pub use work::{Work, count_work};
