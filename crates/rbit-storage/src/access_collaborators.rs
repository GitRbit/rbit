//! Collaborator rule adapter: the existing SQL stays authoritative for one entry per user
//! and repository; this only projects records.

use std::future::Future;

use rbit_access::{collaborators::CollaboratorRepository, organizations::UserAccount, teams::Unique};
use rbit_core::{
    AccountId, RepoId,
    membership::{Collaborator, Permission},
};

use crate::{AccountStore, Database, StorageError, access_organizations::find_user, sqlite::collaborators::CollaboratorStore};

/// Collaborator and account stores behind the `rbit-access` collaborator port.
#[derive(Debug, Clone)]
pub struct CollaboratorAccessStore {
    collaborators: CollaboratorStore,
    accounts: AccountStore,
}

impl CollaboratorAccessStore {
    pub fn new(database: &Database) -> Self {
        Self { collaborators: database.collaborators(), accounts: database.accounts() }
    }
}

impl CollaboratorRepository for CollaboratorAccessStore {
    type Error = StorageError;
    type Id = AccountId;
    type Repo = RepoId;
    type Permission = Permission;
    type Collaborator = Collaborator;

    fn find_user(&self, name: &str) -> impl Future<Output = Result<Option<UserAccount<AccountId>>, StorageError>> + Send {
        find_user(&self.accounts, name)
    }

    fn is_suspended(&self, user: AccountId) -> impl Future<Output = Result<bool, StorageError>> + Send {
        self.accounts.is_suspended(user)
    }

    fn list(&self, repo: RepoId) -> impl Future<Output = Result<Vec<Collaborator>, StorageError>> + Send {
        self.collaborators.list(repo)
    }

    fn permission(&self, repo: RepoId, user: AccountId) -> impl Future<Output = Result<Option<Permission>, StorageError>> + Send {
        self.collaborators.permission(repo, user)
    }

    async fn add(&self, repo: RepoId, user: AccountId, permission: Permission, added_by: AccountId) -> Result<Unique<()>, StorageError> {
        match self.collaborators.add(repo, user, permission, added_by).await {
            Ok(()) => Ok(Unique::Done(())),
            Err(StorageError::Conflict(_)) => Ok(Unique::Taken),
            Err(other) => Err(other),
        }
    }

    fn set_permission(
        &self,
        repo: RepoId,
        user: AccountId,
        permission: Permission,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send {
        self.collaborators.set_permission(repo, user, permission)
    }

    fn remove(&self, repo: RepoId, user: AccountId) -> impl Future<Output = Result<bool, StorageError>> + Send {
        self.collaborators.remove(repo, user)
    }
}
