//! Transfer persistence over the existing stores and SQL.
//!
//! Each transfer call opens the store handle it used before (repositories, accounts or
//! organizations), so refused commands allocate none and pool labels are unchanged.
use crate::{
    RepositoryStore, StorageError,
    sqlite::{Database, orgs},
};
use rbit_core::{Account, AccountId, RepoId, org::OrgMembership};
use rbit_repositories::transfer::{TransferRepository, TransferWriteError};

/// Borrowed transfer persistence; constructing it opens no store.
pub struct TransferStore<'a> {
    database: &'a Database,
}

impl<'a> TransferStore<'a> {
    pub fn new(database: &'a Database) -> Self {
        Self { database }
    }
}

impl std::fmt::Debug for TransferStore<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("TransferStore").finish_non_exhaustive()
    }
}

type Organizations = std::iter::Map<std::vec::IntoIter<OrgMembership>, fn(OrgMembership) -> Account>;

fn organization(membership: OrgMembership) -> Account {
    membership.org
}

impl TransferRepository for TransferStore<'_> {
    type Error = StorageError;
    type Owners = RepositoryStore;
    type Organizations = Organizations;

    fn owners(&self) -> RepositoryStore {
        self.database.repositories()
    }
    async fn is_suspended(&self, account: AccountId) -> Result<bool, StorageError> {
        self.database.accounts().is_suspended(account).await
    }
    async fn organizations_of(&self, member: AccountId) -> Result<Organizations, StorageError> {
        let store: orgs::OrgStore = self.database.orgs();
        Ok(store.memberships_of(member).await?.into_iter().map(organization as fn(OrgMembership) -> Account))
    }
    async fn transfer(&self, repo: RepoId, owner: AccountId) -> Result<(), TransferWriteError<StorageError>> {
        self.database.repositories().transfer(repo, owner).await.map_err(|error| match error {
            StorageError::Conflict(_) => TransferWriteError::Conflict,
            other => TransferWriteError::Failure(other),
        })
    }
}
