//! Repository stars: read-gated starring, summaries and bounded listings.
//!
//! Rules:
//! * Only a repository the actor may read can be starred, unstarred or have its
//!   stargazers listed; anything else is reported as not found, so existence does not
//!   leak.
//! * Starring and unstarring are idempotent and answer with the fresh summary.
//! * Lists show at most [`MAX_LISTED`] entries, newest first. Starred-repository lists
//!   are filtered by the storage visibility predicate for the viewer.
//! * A summary is one indexed query; counts for a listing are one query, and
//!   unstarred repositories are absent from them.

use std::{collections::HashMap, future::Future};

use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, User};

use crate::OwnedRepository;

/// Lists show at most this many entries (newest first).
pub const MAX_LISTED: u32 = 500;

/// Star state of a repository as seen by one viewer.
#[derive(Debug, Clone, Copy, Default)]
pub struct StarSummary {
    pub count: i64,
    pub starred_by_viewer: bool,
}

/// An account that starred a repository, and when.
#[derive(Debug, Clone)]
pub struct Stargazer {
    pub account: Account,
    pub starred_at: Timestamp,
}

/// Star persistence; storage keeps idempotence and the visibility predicate.
pub trait StarRepository: Sync {
    type Error: Send;
    /// `viewer = None` never counts as starred.
    fn summary(&self, repo: RepoId, viewer: Option<AccountId>) -> impl Future<Output = Result<StarSummary, Self::Error>> + Send;
    fn counts(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashMap<RepoId, i64>, Self::Error>> + Send;
    fn star(&self, account: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn unstar(&self, account: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn stargazers(&self, repo: RepoId, limit: u32) -> impl Future<Output = Result<Vec<Stargazer>, Self::Error>> + Send;
    fn starred_by(
        &self,
        account: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
}

/// Refusals are policy; store failures keep their identity.
#[derive(Debug)]
pub enum StarError<E> {
    NotFound,
    Store(E),
}

/// Star commands and queries over one borrowed store.
pub struct RepositoryStars<'a, S> {
    store: &'a S,
}

impl<S> std::fmt::Debug for RepositoryStars<'_, S> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("RepositoryStars").finish_non_exhaustive()
    }
}

impl<'a, S: StarRepository> RepositoryStars<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    pub async fn summary(&self, repo: RepoId, viewer: Option<&User>) -> Result<StarSummary, S::Error> {
        self.store.summary(repo, viewer.map(User::id)).await
    }

    pub async fn counts(&self, repos: &[OwnedRepository]) -> Result<HashMap<RepoId, i64>, S::Error> {
        let ids: Vec<RepoId> = repos.iter().map(|repo| repo.repo.id).collect();
        self.store.counts(&ids).await
    }

    /// `readable` is the loaded request's existing read authority.
    pub async fn star(&self, user: &User, repo: &OwnedRepository, readable: bool) -> Result<StarSummary, StarError<S::Error>> {
        if !readable {
            return Err(StarError::NotFound);
        }
        self.store.star(user.id(), repo.repo.id).await.map_err(StarError::Store)?;
        self.summary(repo.repo.id, Some(user)).await.map_err(StarError::Store)
    }

    pub async fn unstar(&self, user: &User, repo: &OwnedRepository, readable: bool) -> Result<StarSummary, StarError<S::Error>> {
        if !readable {
            return Err(StarError::NotFound);
        }
        self.store.unstar(user.id(), repo.repo.id).await.map_err(StarError::Store)?;
        self.summary(repo.repo.id, Some(user)).await.map_err(StarError::Store)
    }

    pub async fn stargazers(&self, repo: &OwnedRepository, readable: bool) -> Result<Vec<Stargazer>, StarError<S::Error>> {
        if !readable {
            return Err(StarError::NotFound);
        }
        self.store.stargazers(repo.repo.id, MAX_LISTED).await.map_err(StarError::Store)
    }

    /// Repositories `account` starred that `viewer` may see, most recently starred first.
    pub async fn starred_by(&self, viewer: Option<&User>, account: &Account) -> Result<Vec<OwnedRepository>, S::Error> {
        let is_admin = viewer.is_some_and(|viewer| viewer.is_admin);
        self.store.starred_by(account.id, viewer.map(User::id), is_admin, MAX_LISTED).await
    }
}
