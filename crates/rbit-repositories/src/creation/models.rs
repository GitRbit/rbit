//! Creation inputs retain original constructors, derives and field ownership.
use crate::{RepoName, Visibility};
use rbit_contracts::AccountId;
/// Raw "new repository" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct CreateRepository {
    pub name: String,
    pub description: String,
    pub visibility: Visibility,
}

#[derive(Debug)]
pub struct NewRepository {
    pub owner_id: AccountId,
    pub name: RepoName,
    pub description: Option<String>,
    pub visibility: Visibility,
    pub default_branch: String,
}
