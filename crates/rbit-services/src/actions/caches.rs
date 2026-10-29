//! Actions: shared cache. `actions/cache` entries the server keeps for every runner of a
//! repository, as GitHub's cache service does, so a job restores what another runner saved.
//!
//! * Files live at `<data_dir>/actions/caches/<repo id>/<entry id>.tar.gz`, outside backups.
//!   An upload streams to a temporary file beside them while its size and SHA-256 are
//!   counted, must match the digest the runner sent, and only then is recorded and moved
//!   into place.
//! * Scopes come from the task's run, never from the runner (§12.5.6 and
//!   [`super::scheduler::cache_scopes`]): a job restores from its own ref's scope, then the
//!   default branch's, and saves only to its own, so an untrusted run poisons nothing
//!   another run reads first.
//! * An entry matches on its key (exactly, else the newest whose key starts with a
//!   `restore-keys` prefix) and its version, which the runner derives from the cached paths
//!   and its platform. Entries are immutable: saving a key the scope has is refused unless
//!   the runner replaces one it could not restore.
//! * A repository keeps `max_cache_storage_bytes`; a save past it evicts the least recently
//!   restored entries first. The sweeper removes entries unused for `cache_retention_days`.

use std::path::{Path, PathBuf};

use rbit_core::{
    Timestamp, User,
    actions::{CacheEntry, CacheEntryId, Run, Task, TaskId},
};
pub use rbit_storage::sqlite::actions_caches::CacheOrder;
use rbit_storage::sqlite::actions_caches::{CacheFilter, CacheRecorded, NewCacheEntry};
use rbit_workflow::spec::CacheScopes;

use super::{
    ActionsService,
    artifacts::{digest_from_hex, receive, remove_file},
    runners::{CachedRunner, RunnerError},
};
use crate::{RepoContext, ServiceError};

/// Longest key, as on GitHub (and `rbit-runner`).
pub const MAX_CACHE_KEY_CHARS: usize = 512;
/// Most `restore-keys` a lookup may carry.
pub const MAX_RESTORE_KEYS: usize = 10;
/// Most entries one save evicts, and the sweeper removes in one pass.
const EVICT_ENTRIES: u32 = 256;
const SECONDS_PER_DAY: i64 = 86_400;

/// What a runner looks up.
#[derive(Debug, Clone)]
pub struct CacheLookup<'a> {
    pub key: &'a str,
    pub restore_keys: &'a [String],
    pub version: &'a str,
}

/// What a runner sends with a save.
#[derive(Debug, Clone)]
pub struct CacheSave<'a> {
    pub key: &'a str,
    pub version: &'a str,
    /// The tar.gz's SHA-256, as 64 hex digits.
    pub sha256: &'a str,
    /// Replace an entry of the key the job could not restore.
    pub replace: bool,
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.chars().count() <= MAX_CACHE_KEY_CHARS && !key.contains(',')
}

fn valid_version(version: &str) -> bool {
    version.len() == 64 && version.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// Actions: cache REST API.
fn require_writer(repo: &RepoContext) -> Result<(), ServiceError> {
    if !repo.access.can_write() {
        return Err(ServiceError::Forbidden);
    }
    repo.ensure_writable()
}

/// The ref GitHub names an entry's scope by: `refs/heads/main` for `heads/main`, and an
/// untrusted pull request's `pull/3` as `refs/pull/3/merge`.
pub fn scope_ref(scope: &str) -> String {
    match scope.strip_prefix("pull/").filter(|number| number.bytes().all(|b| b.is_ascii_digit())) {
        Some(number) => format!("refs/pull/{number}/merge"),
        None => format!("refs/{scope}"),
    }
}

/// The scope a `ref` filter names, the inverse of [`scope_ref`].
fn ref_to_scope(reference: &str) -> String {
    let scope = reference.strip_prefix("refs/").unwrap_or(reference);
    match scope.strip_prefix("pull/").and_then(|rest| rest.strip_suffix("/merge")) {
        Some(number) if number.bytes().all(|b| b.is_ascii_digit()) => format!("pull/{number}"),
        _ => scope.to_owned(),
    }
}

impl ActionsService {
    /// `<data_dir>/actions/caches`.
    pub(super) fn caches_dir(&self) -> PathBuf {
        self.ctx().config.storage.actions_dir().join("caches")
    }

    /// Where entry `entry`'s file is.
    fn cache_path(&self, entry: &CacheEntry) -> PathBuf {
        self.caches_dir().join(entry.repo_id.0.to_string()).join(format!("{}.tar.gz", entry.id.0))
    }

    /// Task `id` of `runner`, running, its run and the cache scopes the run may use.
    async fn cache_task(&self, runner: &CachedRunner, id: TaskId) -> Result<(Task, Run, CacheScopes), RunnerError> {
        let task = self.running_task(runner, id).await?;
        let database = &self.ctx().database;
        let job = database.actions_runs().job(task.job_id).await?.ok_or_else(|| RunnerError::not_found("there is no such job"))?;
        if job.task_id != Some(task.id) {
            return Err(RunnerError::conflict("the job has been rerun; this task no longer belongs to it"));
        }
        let run = database.actions_runs().run(job.run_id).await?.ok_or_else(|| RunnerError::not_found("there is no such run"))?;
        let repo =
            database.admin().find_repository(run.repo_id).await?.ok_or_else(|| RunnerError::not_found("there is no such repository"))?;
        let scopes = super::scheduler::cache_scopes(&run, &repo.repo.default_branch);
        Ok((task, run, scopes))
    }

    /// The entry task `task` restores for `lookup`: the exact key in each of its scopes in
    /// order, then for each `restore-keys` prefix the newest match, scope by scope.
    pub async fn find_task_cache(
        &self,
        runner: &CachedRunner,
        task: TaskId,
        lookup: &CacheLookup<'_>,
    ) -> Result<Option<CacheEntry>, RunnerError> {
        if !valid_key(lookup.key) || lookup.restore_keys.iter().any(|key| !valid_key(key)) {
            return Err(RunnerError::bad_request(format!("cache keys are 1 to {MAX_CACHE_KEY_CHARS} characters without commas")));
        }
        if lookup.restore_keys.len() > MAX_RESTORE_KEYS {
            return Err(RunnerError::bad_request(format!("a lookup has at most {MAX_RESTORE_KEYS} restore keys")));
        }
        if !valid_version(lookup.version) {
            return Err(RunnerError::bad_request("the cache version must be 64 lowercase hex digits"));
        }
        let (_, run, scopes) = self.cache_task(runner, task).await?;
        let store = self.ctx().database.actions_caches();
        for scope in &scopes.read {
            if let Some(entry) = store.exact(run.repo_id, scope, lookup.key, lookup.version).await? {
                return Ok(Some(entry));
            }
        }
        for prefix in lookup.restore_keys {
            for scope in &scopes.read {
                if let Some(entry) = store.newest_with_prefix(run.repo_id, scope, prefix, lookup.version).await? {
                    return Ok(Some(entry));
                }
            }
        }
        Ok(None)
    }

    /// Entry `id`, which task `task` may restore, and its file. Restoring counts as a use.
    pub async fn open_task_cache(
        &self,
        runner: &CachedRunner,
        task: TaskId,
        id: CacheEntryId,
    ) -> Result<(CacheEntry, tokio::fs::File), RunnerError> {
        let (_, run, scopes) = self.cache_task(runner, task).await?;
        let store = self.ctx().database.actions_caches();
        let entry = store
            .find(id)
            .await?
            .filter(|entry| entry.repo_id == run.repo_id && scopes.read.contains(&entry.scope))
            .ok_or_else(|| RunnerError::not_found("this task has no such cache entry"))?;
        let file =
            tokio::fs::File::open(self.cache_path(&entry)).await.map_err(|_| RunnerError::not_found("the cache entry's file is gone"))?;
        store.touch(entry.id, Timestamp::now()).await?;
        Ok((entry, file))
    }

    /// Stores what `body` uploads as entry `save.key` in task `task`'s write scope; `None`
    /// when the scope has the key already.
    pub async fn save_task_cache(
        &self,
        runner: &CachedRunner,
        task: TaskId,
        save: &CacheSave<'_>,
        mut body: impl tokio::io::AsyncRead + Unpin,
    ) -> Result<Option<CacheEntry>, RunnerError> {
        if !valid_key(save.key) {
            return Err(RunnerError::bad_request(format!("cache keys are 1 to {MAX_CACHE_KEY_CHARS} characters without commas")));
        }
        if !valid_version(save.version) {
            return Err(RunnerError::bad_request("the cache version must be 64 lowercase hex digits"));
        }
        let expected = digest_from_hex(save.sha256).ok_or_else(|| RunnerError::bad_request("the content digest must be 64 hex digits"))?;
        let (task, run, scopes) = self.cache_task(runner, task).await?;
        let store = self.ctx().database.actions_caches();
        if !save.replace && store.exact(run.repo_id, &scopes.write, save.key, save.version).await?.is_some() {
            return Ok(None);
        }
        let limit = self.config().max_cache_storage_bytes;

        let dir = self.caches_dir();
        tokio::fs::create_dir_all(dir.join("tmp")).await.map_err(ServiceError::internal)?;
        let temporary = dir.join("tmp").join(format!("{}-{}.part", task.id.0, super::tokens::random_name()?));
        let (size, digest) = match receive(&mut body, &temporary, limit).await {
            Ok(Some(received)) => received,
            Ok(None) => {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(RunnerError::too_large("the cache entry is larger than a repository's cache storage"));
            }
            Err(error) => {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(ServiceError::internal(error).into());
            }
        };
        if digest != expected {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(RunnerError::bad_request("the cache entry does not match its content digest"));
        }

        let new = NewCacheEntry {
            repo: run.repo_id,
            scope: &scopes.write,
            key: save.key,
            version: save.version,
            size,
            sha256: digest,
            now: Timestamp::now(),
        };
        let (entry, replaced) = match store.record(&new, save.replace).await? {
            CacheRecorded::Stored { entry, replaced } => (*entry, replaced),
            CacheRecorded::Exists => {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Ok(None);
            }
        };
        // The replaced file goes first: SQLite may give the new row the old one's id.
        if let Some(replaced) = &replaced {
            remove_file(&self.cache_path(replaced)).await;
        }
        let path = self.cache_path(&entry);
        let placed = async {
            tokio::fs::create_dir_all(path.parent().unwrap_or(&dir)).await?;
            tokio::fs::rename(&temporary, &path).await
        }
        .await;
        if let Err(error) = placed {
            let _ = tokio::fs::remove_file(&temporary).await;
            store.delete(entry.id).await?;
            return Err(ServiceError::internal(error).into());
        }
        self.evict_caches(&entry, limit).await?;
        tracing::info!(run = run.id.0, scope = %entry.scope, key = %entry.key, size, "cache entry saved");
        Ok(Some(entry))
    }

    /// Removes `saved`'s repository's least recently used entries, never `saved` itself,
    /// until the repository keeps at most `limit` bytes.
    async fn evict_caches(&self, saved: &CacheEntry, limit: u64) -> Result<(), ServiceError> {
        let store = self.ctx().database.actions_caches();
        let mut stored = store.stored_bytes(saved.repo_id).await?;
        if stored <= limit {
            return Ok(());
        }
        for entry in store.least_used(saved.repo_id, EVICT_ENTRIES).await? {
            if stored <= limit {
                break;
            }
            if entry.id == saved.id {
                continue;
            }
            self.remove_cache(&entry).await?;
            stored = stored.saturating_sub(entry.size);
        }
        Ok(())
    }

    /// Deletes entry `entry` with its file.
    async fn remove_cache(&self, entry: &CacheEntry) -> Result<(), ServiceError> {
        remove_file(&self.cache_path(entry)).await;
        self.ctx().database.actions_caches().delete(entry.id).await?;
        Ok(())
    }

    /// Deletes entries no job restored since `cache_retention_days` before `now`; how many.
    pub(super) async fn expire_caches(&self, now: Timestamp) -> Result<usize, ServiceError> {
        let days = i64::from(self.config().cache_retention_days);
        let unused = self.ctx().database.actions_caches().unused_since(Timestamp(now.0 - days * SECONDS_PER_DAY), EVICT_ENTRIES).await?;
        for entry in &unused {
            self.remove_cache(entry).await?;
        }
        Ok(unused.len())
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
        self.remove_cache(&entry).await?;
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
            self.remove_cache(entry).await?;
        }
        tracing::info!(repo = %repo.repo.full_name(), %key, count = entries.len(), by = %actor.name(), "cache entries deleted");
        Ok(entries)
    }

    /// Whether `file`, in a repository's cache directory, names an entry that is gone.
    pub(super) async fn orphaned_cache_file(&self, file: &Path) -> Result<bool, ServiceError> {
        let id = file.file_name().and_then(|name| name.to_str()).and_then(|name| name.strip_suffix(".tar.gz")?.parse().ok());
        Ok(match id {
            Some(id) => self.ctx().database.actions_caches().find(CacheEntryId(id)).await?.is_none(),
            None => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_versions_are_checked() {
        assert!(valid_key("cargo-Linux-abc"));
        assert!(!valid_key(""));
        assert!(!valid_key("a,b"));
        assert!(!valid_key(&"k".repeat(MAX_CACHE_KEY_CHARS + 1)));
        assert!(valid_version(&"0a".repeat(32)));
        assert!(!valid_version(&"0A".repeat(32)));
        assert!(!valid_version("abc"));
    }

    #[test]
    fn scopes_and_refs_map_both_ways() {
        for (scope, reference) in [("heads/main", "refs/heads/main"), ("pull/3", "refs/pull/3/merge"), ("pull/3/head", "refs/pull/3/head")]
        {
            assert_eq!(scope_ref(scope), reference);
            assert_eq!(ref_to_scope(reference), scope);
        }
        assert_eq!(ref_to_scope("heads/main"), "heads/main");
    }
}
