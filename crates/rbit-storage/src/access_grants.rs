//! Grant loading adapter: the existing grant query stays authoritative; this projects its
//! stored row onto the `rbit-access` grant shape.

use std::future::Future;

use rbit_access::grants::{GrantRepository, RepositoryGrants};
use rbit_core::{AccountId, RepoId, membership::Permission};

use crate::{RepositoryStore, StorageError, sqlite::collaborators::ViewerGrants};

impl From<ViewerGrants> for RepositoryGrants<Permission> {
    fn from(stored: ViewerGrants) -> Self {
        Self { org_role: stored.org_role, org_base: stored.org_base.permission(), team: stored.team, collaborator: stored.collaborator }
    }
}

impl GrantRepository for RepositoryStore {
    type Error = StorageError;
    type Id = AccountId;
    type Repo = RepoId;
    type Permission = Permission;

    fn grants(
        &self,
        repo: RepoId,
        owner: AccountId,
        viewer: AccountId,
    ) -> impl Future<Output = Result<RepositoryGrants<Permission>, StorageError>> + Send {
        let stored = RepositoryStore::grants(self, repo, owner, viewer);
        async move { stored.await.map(Into::into) }
    }
}
