//! Repository general settings and destructive lifecycle authority through static ports.
mod application;
mod models;
mod port;
pub use application::{RepositorySettings, RepositorySettingsError};
pub use models::{RepoGeneralSettings, UpdateRepository};
pub use port::{RepositorySettingsPort, RepositorySettingsRepository};
