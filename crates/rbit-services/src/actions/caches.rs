//! Actions: shared cache. `actions/cache` entries the server keeps for every runner of a
//! repository, as GitHub's cache service does, so a job restores what another runner saved.
//! The composition of [`rbit_actions::caches`], which decides lookups, saves, eviction and
//! expiry.
//!
//! * Files live at `<data_dir>/actions/caches/<repo id>/<entry id>.tar.gz`, included in backups;
//!   [`super::cache_ports`] implements the store's ports over them and `actions_caches`.
//! * Scopes come from the task's run, never from the runner (§12.5.6 and
//!   [`super::scheduler::cache_scopes`]).

use std::path::{Path, PathBuf};

pub use rbit_actions::caches::{CacheLookup, CacheSave};
use rbit_actions::{archives::cache_entry_file, caches::SharedCache};
use rbit_core::{
    Timestamp, User,
    actions::{CacheEntry, CacheEntryId, Run, Task, TaskId},
};
use rbit_storage::sqlite::actions_caches::CacheFilter;
pub use rbit_storage::sqlite::actions_caches::CacheOrder;
use rbit_workflow::{protocol::ref_to_scope, spec::CacheScopes};
use tokio::io::AsyncRead;

use super::{
    ActionsService,
    archive_files::answer,
    cache_ports::{CacheRows, CacheTaskPort},
    runners::{CachedRunner, RunnerError},
};
use crate::{RepoContext, ServiceError};

// Actions: cache REST API.
fn require_writer(repo: &RepoContext) -> Result<(), ServiceError> {
    if !repo.access.can_write() {
        return Err(ServiceError::Forbidden);
    }
    repo.ensure_writable()
}

impl ActionsService {
    /// `<data_dir>/actions/caches`.
    pub(super) fn caches_dir(&self) -> PathBuf {
        self.ctx().config.storage.actions_dir().join("caches")
    }

    /// Where entry `entry`'s file is.
    pub(super) fn cache_path(&self, entry: &CacheEntry) -> PathBuf {
        self.caches_dir().join(entry.repo_id.0.to_string()).join(format!("{}.tar.gz", entry.id.0))
    }

    /// Task `id` of `runner`, running, its run and the cache scopes the run may use.
    pub(super) async fn cache_task(&self, runner: &CachedRunner, id: TaskId) -> Result<(Task, Run, CacheScopes), RunnerError> {
        let (task, job) = self.leased_task(runner, id).await?;
        let database = &self.ctx().database;
        let run = database.actions_runs().run(job.run_id).await?.ok_or_else(|| RunnerError::not_found("there is no such run"))?;
        let repo =
            database.admin().find_repository(run.repo_id).await?.ok_or_else(|| RunnerError::not_found("there is no such repository"))?;
        let scopes = super::scheduler::cache_scopes(&run, &repo.repo.default_branch);
        Ok((task, run, scopes))
    }

    /// The entry task `task` restores for `lookup` ([`SharedCache::find`]).
    pub async fn find_task_cache(
        &self,
        runner: &CachedRunner,
        task: TaskId,
        lookup: &CacheLookup<'_>,
    ) -> Result<Option<CacheEntry>, RunnerError> {
        SharedCache::find(&CacheTaskPort::<tokio::io::Empty>::new(self, runner, task), lookup).await.map_err(answer)
    }

    /// Entry `id`, which task `task` may restore, and its file. Restoring counts as a use.
    pub async fn open_task_cache(
        &self,
        runner: &CachedRunner,
        task: TaskId,
        id: CacheEntryId,
    ) -> Result<(CacheEntry, tokio::fs::File), RunnerError> {
        SharedCache::open(&CacheTaskPort::<tokio::io::Empty>::new(self, runner, task), id.0).await.map_err(answer)
    }

    /// Stores what `body` uploads as entry `save.key` in task `task`'s write scope; `None`
    /// when the scope has the key already.
    pub async fn save_task_cache(
        &self,
        runner: &CachedRunner,
        task: TaskId,
        save: &CacheSave<'_>,
        body: impl AsyncRead + Unpin + Send,
    ) -> Result<Option<CacheEntry>, RunnerError> {
        let port = CacheTaskPort::new(self, runner, task);
        let saved = SharedCache::save(&port, save, self.config().max_cache_storage_bytes, body).await.map_err(answer)?;
        if let Some(entry) = &saved {
            tracing::info!(run = port.run_id().map_or(0, |run| run.0), scope = %entry.scope, key = %entry.key, size = entry.size, "cache entry saved");
        }
        Ok(saved)
    }

    /// Deletes entries no job restored since `cache_retention_days` before `now`; how many.
    pub(super) async fn expire_caches(&self, now: Timestamp) -> Result<usize, ServiceError> {
        SharedCache::expire(&CacheRows::new(self), now.0, self.config().cache_retention_days).await
    }

    // Actions: cache REST API.
    /// A page of `repo`'s entries for a reader, those whose key starts with `key` and whose
    /// scope is `reference`'s when given, and how many match in all.
    pub async fn repo_caches(
        &self,
        repo: &RepoContext,
        key: Option<&str>,
        reference: Option<&str>,
        order: CacheOrder,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<CacheEntry>, u64), ServiceError> {
        self.require_pages(repo).await?;
        let scope = reference.map(ref_to_scope);
        let filter = CacheFilter { key_prefix: key, scope: scope.as_deref() };
        Ok(self.ctx().database.actions_caches().of_repo(repo.repo.repo.id, &filter, order, offset, limit).await?)
    }

    /// How many entries `repo` keeps and their bytes, for a reader.
    pub async fn cache_usage(&self, repo: &RepoContext) -> Result<(u64, u64), ServiceError> {
        self.require_pages(repo).await?;
        Ok(self.ctx().database.actions_caches().usage(repo.repo.repo.id).await?)
    }

    /// Deletes entry `id` of `repo` with its file, for someone who may write to it.
    pub async fn delete_cache(&self, actor: &User, repo: &RepoContext, id: CacheEntryId) -> Result<(), ServiceError> {
        self.require_pages(repo).await?;
        let entry = self
            .ctx()
            .database
            .actions_caches()
            .find(id)
            .await?
            .filter(|entry| entry.repo_id == repo.repo.repo.id)
            .ok_or(ServiceError::NotFound)?;
        require_writer(repo)?;
        SharedCache::remove(&CacheRows::new(self), &entry).await?;
        tracing::info!(repo = %repo.repo.full_name(), key = %entry.key, scope = %entry.scope, by = %actor.name(), "cache entry deleted");
        Ok(())
    }

    /// Deletes `repo`'s entries of exactly `key`, of `reference`'s scope when given, for
    /// someone who may write to it; the entries deleted.
    pub async fn delete_caches_by_key(
        &self,
        actor: &User,
        repo: &RepoContext,
        key: &str,
        reference: Option<&str>,
    ) -> Result<Vec<CacheEntry>, ServiceError> {
        self.require_pages(repo).await?;
        require_writer(repo)?;
        let scope = reference.map(ref_to_scope);
        let entries = self.ctx().database.actions_caches().with_key(repo.repo.repo.id, key, scope.as_deref()).await?;
        for entry in &entries {
            SharedCache::remove(&CacheRows::new(self), entry).await?;
        }
        tracing::info!(repo = %repo.repo.full_name(), %key, count = entries.len(), by = %actor.name(), "cache entries deleted");
        Ok(entries)
    }

    /// Whether `file`, in a repository's cache directory, names an entry that is gone.
    pub(super) async fn orphaned_cache_file(&self, file: &Path) -> Result<bool, ServiceError> {
        let id = file.file_name().and_then(|name| name.to_str()).and_then(cache_entry_file);
        Ok(match id {
            Some(id) => self.ctx().database.actions_caches().find(CacheEntryId(id)).await?.is_none(),
            None => false,
        })
    }
}
