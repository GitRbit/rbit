//! Branch protection rule adapter: the existing SQL stays authoritative for the rule
//! limit, pattern uniqueness and organisation-only teams; this only projects records.

use std::future::Future;

use rbit_access::{
    protection::{ProtectionId, RuleChange, RuleRepository, StoredInput, StoredRule},
    teams::Unique,
};
use rbit_core::{AccountId, RepoId, membership::TeamId, protection::BranchProtection, status::StatusContext};

use crate::{StorageError, sqlite::protection::BranchProtectionStore};

/// A pattern conflict becomes `Taken`; every other failure keeps its identity.
fn unique(result: Result<RuleChange, StorageError>) -> Result<Unique<RuleChange>, StorageError> {
    match result {
        Ok(change) => Ok(Unique::Done(change)),
        Err(StorageError::Conflict(_)) => Ok(Unique::Taken),
        Err(other) => Err(other),
    }
}

impl RuleRepository for BranchProtectionStore {
    type Error = StorageError;
    type Repo = RepoId;
    type User = AccountId;
    type Team = TeamId;
    type Context = StatusContext;
    type Record = BranchProtection;

    fn rule(record: &BranchProtection) -> &StoredRule<Self> {
        &record.rule
    }

    fn into_rule(record: BranchProtection) -> StoredRule<Self> {
        record.rule
    }

    fn list(&self, repo: RepoId) -> impl Future<Output = Result<Vec<BranchProtection>, StorageError>> + Send {
        BranchProtectionStore::list(self, repo)
    }

    fn get(&self, repo: RepoId, id: ProtectionId) -> impl Future<Output = Result<Option<BranchProtection>, StorageError>> + Send {
        BranchProtectionStore::get(self, repo, id)
    }

    fn teams_of(&self, repo: RepoId, user: AccountId) -> impl Future<Output = Result<Vec<TeamId>, StorageError>> + Send {
        BranchProtectionStore::teams_of(self, repo, user)
    }

    async fn create(&self, repo: RepoId, input: &StoredInput<Self>, creator: AccountId) -> Result<Unique<RuleChange>, StorageError> {
        unique(BranchProtectionStore::create(self, repo, input, creator).await)
    }

    async fn update(&self, repo: RepoId, id: ProtectionId, input: &StoredInput<Self>) -> Result<Unique<RuleChange>, StorageError> {
        unique(BranchProtectionStore::update(self, repo, id, input).await)
    }

    fn delete(&self, repo: RepoId, id: ProtectionId) -> impl Future<Output = Result<bool, StorageError>> + Send {
        BranchProtectionStore::delete(self, repo, id)
    }
}
