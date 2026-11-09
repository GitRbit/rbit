//! Repository general settings and destructive lifecycle authority through static ports.
mod admin_deletion;
mod application;
mod models;
mod port;
pub use admin_deletion::{AdminDeletionError, AdminDeletionPort, AdminRepositoryDeletion};
pub use application::{RepositorySettings, RepositorySettingsError};
pub use models::{RepoGeneralSettings, UpdateRepository};
pub use port::{RepositorySettingsPort, RepositorySettingsRepository};
