//! The LFS locks API: a user locks a path so others do not edit a file that cannot be
//! merged; `git lfs push` asks which locks are the pusher's and which are not.
//!
//! # Rules
//!
//! * Anyone who can read the repository lists its locks.
//! * Creating, verifying and unlocking need write access *and* an account (deploy keys
//!   and anonymous callers cannot hold locks).
//! * A lock is removed by its owner; repository administrators may remove anyone's with
//!   `force`.
//! * Pages are ordered by lock id; the cursor is the id the next page starts after.

use rbit_core::{
    User,
    lfs::{LfsLock, LfsLockId, validate_lock_path},
};
use rbit_storage::{StorageError, sqlite::lfs::LockFilter};

use super::LfsService;
use crate::{RepoContext, ServiceError};

/// Locks per page when the client does not ask for fewer.
pub const MAX_LOCKS_PER_PAGE: u32 = 100;

/// The query parameters of `GET …/locks`, exactly as sent.
#[derive(Debug, Clone, Default)]
pub struct LockQuery {
    pub path: Option<String>,
    pub id: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// One page of locks.
#[derive(Debug, Clone)]
pub struct LockPage {
    pub locks: Vec<LfsLock>,
    pub next_cursor: Option<String>,
}

/// The answer to `POST …/locks/verify`, split by owner.
#[derive(Debug, Clone)]
pub struct LockVerification {
    pub ours: Vec<LfsLock>,
    pub theirs: Vec<LfsLock>,
    pub next_cursor: Option<String>,
}

/// What creating a lock did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockCreated {
    Created(LfsLock),
    /// The path was locked already (by this lock, possibly the caller's own).
    Exists(LfsLock),
}

fn page_size(limit: Option<u32>) -> u32 {
    limit.unwrap_or(MAX_LOCKS_PER_PAGE).clamp(1, MAX_LOCKS_PER_PAGE)
}

fn parse_cursor(cursor: Option<&str>) -> Result<Option<LfsLockId>, ServiceError> {
    match cursor.filter(|c| !c.is_empty()) {
        None => Ok(None),
        Some(cursor) => {
            cursor.parse().map(|id| Some(LfsLockId(id))).map_err(|_| ServiceError::Invalid("the cursor is not valid".to_owned()))
        }
    }
}

/// The user who may change locks: write access and an account.
fn locker<'a>(repo: &RepoContext, user: Option<&'a User>) -> Result<&'a User, ServiceError> {
    match user {
        Some(user) if repo.access.can_write() => Ok(user),
        _ => Err(ServiceError::Forbidden),
    }
}

impl LfsService {
    /// One page of `repo`'s locks matching `query`. A malformed lock id matches nothing.
    pub async fn list_locks(&self, repo: &RepoContext, query: &LockQuery) -> Result<LockPage, ServiceError> {
        let id = match query.id.as_deref().filter(|id| !id.is_empty()) {
            None => None,
            Some(id) => match id.parse() {
                Ok(id) => Some(LfsLockId(id)),
                Err(_) => return Ok(LockPage { locks: Vec::new(), next_cursor: None }),
            },
        };
        let limit = page_size(query.limit);
        let filter = LockFilter {
            path: query.path.as_deref().filter(|path| !path.is_empty()),
            id,
            after: parse_cursor(query.cursor.as_deref())?,
            limit: limit + 1,
        };
        let mut locks = self.db().list_locks(repo.repo.repo.id, &filter).await?;
        let next_cursor = next_cursor(&mut locks, limit);
        Ok(LockPage { locks, next_cursor })
    }

    /// Locks `path` for `user`.
    pub async fn create_lock(&self, repo: &RepoContext, user: Option<&User>, path: &str) -> Result<LockCreated, ServiceError> {
        let user = locker(repo, user)?;
        repo.ensure_writable()?;
        validate_lock_path(path).map_err(ServiceError::Invalid)?;
        let repo_id = repo.repo.repo.id;
        match self.db().create_lock(repo_id, path, user.id()).await {
            Ok(lock) => {
                tracing::info!(repo = %repo.repo.full_name(), user = %user.name(), path, "LFS lock created");
                Ok(LockCreated::Created(lock))
            }
            Err(StorageError::Conflict(_)) => {
                let existing = self.db().find_lock_by_path(repo_id, path).await?;
                // Unlocked again in between: let the client retry.
                existing.map(LockCreated::Exists).ok_or(ServiceError::Conflict("a lock for this path"))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// One page of `repo`'s locks, split into `user`'s and everyone else's (what a push
    /// must not touch).
    pub async fn verify_locks(
        &self,
        repo: &RepoContext,
        user: Option<&User>,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<LockVerification, ServiceError> {
        let user = locker(repo, user)?;
        let limit = page_size(limit);
        let filter = LockFilter { after: parse_cursor(cursor)?, limit: limit + 1, ..LockFilter::default() };
        let mut locks = self.db().list_locks(repo.repo.repo.id, &filter).await?;
        let next_cursor = next_cursor(&mut locks, limit);
        let (ours, theirs) = locks.into_iter().partition(|lock| lock.owner.id == user.id());
        Ok(LockVerification { ours, theirs, next_cursor })
    }

    /// Removes lock `id`: the owner's own, or anyone's with `force` for administrators.
    pub async fn unlock(&self, repo: &RepoContext, user: Option<&User>, id: &str, force: bool) -> Result<LfsLock, ServiceError> {
        let user = locker(repo, user)?;
        repo.ensure_writable()?;
        let id = id.parse().map(LfsLockId).map_err(|_| ServiceError::NotFound)?;
        let repo_id = repo.repo.repo.id;
        let lock = self.db().find_lock(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        if lock.owner.id != user.id() && !(force && repo.access.can_admin()) {
            return Err(ServiceError::Forbidden);
        }
        if !self.db().delete_lock(repo_id, id).await? {
            return Err(ServiceError::NotFound);
        }
        tracing::info!(repo = %repo.repo.full_name(), user = %user.name(), path = lock.path, force, "LFS lock removed");
        Ok(lock)
    }
}

/// Trims the one extra lock fetched beyond `limit` and returns the cursor for the next page.
fn next_cursor(locks: &mut Vec<LfsLock>, limit: u32) -> Option<String> {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if locks.len() <= limit {
        return None;
    }
    locks.truncate(limit);
    locks.last().map(|lock| lock.id.0.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_bounded() {
        assert_eq!(page_size(None), MAX_LOCKS_PER_PAGE);
        assert_eq!(page_size(Some(0)), 1);
        assert_eq!(page_size(Some(5)), 5);
        assert_eq!(page_size(Some(5000)), MAX_LOCKS_PER_PAGE);
    }

    #[test]
    fn cursors_are_lock_ids() {
        assert_eq!(parse_cursor(None).unwrap(), None);
        assert_eq!(parse_cursor(Some("")).unwrap(), None);
        assert_eq!(parse_cursor(Some("12")).unwrap(), Some(LfsLockId(12)));
        assert!(matches!(parse_cursor(Some("x")), Err(ServiceError::Invalid(_))));
    }
}
