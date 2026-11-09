//! Site administration adapter: the existing SQL stays authoritative for the
//! last-administrator rule; this only projects records.

use std::future::Future;

use rbit_access::administration::{AdministrationRepository, ListFilter, Listing, Moderation};
use rbit_core::{AccountId, OwnedRepository, RepoId};

use crate::{
    StorageError,
    sqlite::admin::{AdminStore, OrgRecord, UserRecord},
};

impl AdministrationRepository for AdminStore {
    type Error = StorageError;
    type Id = AccountId;
    type RepoId = RepoId;
    type User = UserRecord;
    type Organization = OrgRecord;
    type Repository = OwnedRepository;

    fn user_id(user: &UserRecord) -> AccountId {
        user.user.id()
    }

    fn user_name(user: &UserRecord) -> &str {
        user.user.name().as_str()
    }

    fn list_users(&self, filter: ListFilter<'_>) -> impl Future<Output = Result<Listing<UserRecord>, StorageError>> + Send {
        AdminStore::list_users(self, filter)
    }

    fn find_user(&self, id: AccountId) -> impl Future<Output = Result<Option<UserRecord>, StorageError>> + Send {
        AdminStore::find_user(self, id)
    }

    fn set_admin(&self, id: AccountId, is_admin: bool) -> impl Future<Output = Result<Moderation, StorageError>> + Send {
        AdminStore::set_admin(self, id, is_admin)
    }

    fn set_suspended(&self, id: AccountId, suspended: bool) -> impl Future<Output = Result<Moderation, StorageError>> + Send {
        AdminStore::set_suspended(self, id, suspended)
    }

    fn delete_user(&self, id: AccountId) -> impl Future<Output = Result<Moderation, StorageError>> + Send {
        AdminStore::delete_user(self, id)
    }

    fn repositories_owned_by(&self, owner: AccountId) -> impl Future<Output = Result<Vec<(RepoId, String)>, StorageError>> + Send {
        AdminStore::repositories_owned_by(self, owner)
    }

    fn organizations_solely_owned_by(&self, user: AccountId) -> impl Future<Output = Result<Vec<String>, StorageError>> + Send {
        AdminStore::organizations_solely_owned_by(self, user)
    }

    fn list_organizations(&self, filter: ListFilter<'_>) -> impl Future<Output = Result<Listing<OrgRecord>, StorageError>> + Send {
        AdminStore::list_organizations(self, filter)
    }

    fn list_repositories(&self, filter: ListFilter<'_>) -> impl Future<Output = Result<Listing<OwnedRepository>, StorageError>> + Send {
        AdminStore::list_repositories(self, filter)
    }

    fn find_repository(&self, id: RepoId) -> impl Future<Output = Result<Option<OwnedRepository>, StorageError>> + Send {
        AdminStore::find_repository(self, id)
    }
}
