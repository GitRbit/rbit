//! Preserve public-parent fast paths, SQL pagination and conservative ancestry bounds.
use super::{ForkParentAccess, ForkReadRepository, MAX_LISTED_FORKS};
use crate::OwnedRepository;
use rbit_contracts::{AccountId, RepoId};
use std::collections::HashSet;
const MAX_FORK_DEPTH: usize = 64;
#[derive(Debug, Clone, Copy)]
pub struct ForkViewer {
    pub account: Option<AccountId>,
    pub is_admin: bool,
}
#[derive(Debug)]
pub enum ForkParentError<S, A> {
    Store(S),
    Access(A),
}
pub struct ForkQueries<'a, S> {
    store: &'a S,
}
impl<S> std::fmt::Debug for ForkQueries<'_, S> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ForkQueries").finish_non_exhaustive()
    }
}
impl<'a, S: ForkReadRepository> ForkQueries<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }
    pub async fn parent<A: ForkParentAccess>(
        &self,
        repo: RepoId,
        access: &A,
    ) -> Result<Option<OwnedRepository>, ForkParentError<S::Error, A::Error>> {
        let Some(parent) = self.store.parent_of(repo).await.map_err(ForkParentError::Store)? else { return Ok(None) };
        if parent.repo.visibility.is_public() {
            return Ok(Some(parent));
        }
        if access.can_open(&parent).await.map_err(ForkParentError::Access)? { Ok(Some(parent)) } else { Ok(None) }
    }
    pub async fn forks_among(&self, repos: &[RepoId]) -> Result<HashSet<RepoId>, S::Error> {
        self.store.forks_among(repos).await
    }
    pub async fn is_fork(&self, repo: RepoId) -> Result<bool, S::Error> {
        Ok(self.forks_among(&[repo]).await?.contains(&repo))
    }
    pub async fn count(&self, viewer: ForkViewer, repo: RepoId) -> Result<u64, S::Error> {
        self.store.count_readable(repo, viewer.account, viewer.is_admin).await
    }
    pub async fn list(&self, viewer: ForkViewer, repo: RepoId) -> Result<Vec<OwnedRepository>, S::Error> {
        Ok(self.page(viewer, repo, MAX_LISTED_FORKS, 0).await?.0)
    }
    pub async fn page(&self, viewer: ForkViewer, repo: RepoId, limit: u32, offset: u32) -> Result<(Vec<OwnedRepository>, u64), S::Error> {
        let forks = self.store.list_readable(repo, viewer.account, viewer.is_admin, limit, offset).await?;
        Ok((forks, self.store.count_readable(repo, viewer.account, viewer.is_admin).await?))
    }
    pub async fn has_private_ancestor(&self, repo: RepoId) -> Result<bool, S::Error> {
        let mut seen = HashSet::from([repo]);
        let mut current = repo;
        for _ in 0..MAX_FORK_DEPTH {
            let Some(parent) = self.store.parent_of(current).await? else { return Ok(false) };
            if !parent.repo.visibility.is_public() || !seen.insert(parent.repo.id) {
                return Ok(true);
            }
            current = parent.repo.id;
        }
        Ok(true)
    }
}
