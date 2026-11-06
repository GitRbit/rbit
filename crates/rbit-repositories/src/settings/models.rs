//! Existing settings contracts, shared unchanged by services and persistence.
use crate::Visibility;
/// Raw "general settings" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct UpdateRepository {
    pub description: String,
    pub visibility: Visibility,
    /// `None` keeps the current default branch (e.g. an empty repository has no branches to choose from).
    pub default_branch: Option<String>,
}

/// The general settings of a repository, written together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoGeneralSettings {
    pub description: Option<String>,
    pub visibility: Visibility,
    pub default_branch: String,
}
