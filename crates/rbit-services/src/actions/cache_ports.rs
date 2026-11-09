//! The rbit-actions shared cache's ports over `actions_caches` and the files below
//! `<data_dir>/actions/caches`: one task port per runner request (its lease, scopes, body
//! and temporary file) and one row port for REST deletes and the sweep. Policy is not
//! decided here.

use std::{marker::PhantomData, path::PathBuf, sync::OnceLock};

use rbit_actions::{
    archives::{ArchiveFiles, ArchiveUploads, Received},
    caches::{CacheFacts, CacheRecorded, CacheRecords, CacheRetention, CacheSaveStore, CacheTaskStore, NewCacheEntry},
};
use rbit_core::{
    RepoId, Timestamp,
    actions::{CacheEntry, CacheEntryId, Run, RunId, Task, TaskId},
};
use rbit_storage::sqlite::actions_caches::{CacheRecorded as Recorded, NewCacheEntry as EntryRow};
use rbit_workflow::spec::CacheScopes;
use tokio::io::AsyncRead;

use super::{
    ActionsService, archive_files,
    runners::{CachedRunner, RunnerError},
};
use crate::ServiceError;

fn facts(entry: &CacheEntry) -> CacheFacts<'_> {
    CacheFacts { id: entry.id.0, repository: entry.repo_id.0, scope: &entry.scope, size: entry.size }
}

/// Implements [`CacheRecords`] for a port whose errors convert from [`ServiceError`].
macro_rules! cache_records {
    (impl[$($generic:ident)?] $port:ty, $error:ty) => {
        impl$(<$generic>)? CacheRecords for $port {
            type Entry = CacheEntry;

            fn facts<'e>(&self, entry: &'e CacheEntry) -> CacheFacts<'e> {
                facts(entry)
            }

            fn location(&self, entry: &CacheEntry) -> PathBuf {
                self.service.cache_path(entry)
            }

            async fn delete(&self, entry: &CacheEntry) -> Result<(), $error> {
                self.service.ctx().database.actions_caches().delete(entry.id).await.map_err(ServiceError::from)?;
                Ok(())
            }

            async fn stored_bytes(&self, repository: i64) -> Result<u64, $error> {
                self.service.ctx().database.actions_caches().stored_bytes(RepoId(repository)).await.map_err(|error| <$error>::from(ServiceError::from(error)))
            }

            async fn least_used(&self, repository: i64, limit: u32) -> Result<Vec<CacheEntry>, $error> {
                self.service.ctx().database.actions_caches().least_used(RepoId(repository), limit).await.map_err(|error| <$error>::from(ServiceError::from(error)))
            }
        }
    };
}

/// One runner's cache requests through its task `task`, with save body `B`.
pub(super) struct CacheTaskPort<'a, B> {
    service: &'a ActionsService,
    runner: &'a CachedRunner,
    task: TaskId,
    /// The task's run, once leased: the save log names it.
    run: OnceLock<RunId>,
    body: PhantomData<fn(B)>,
}

impl<'a, B> CacheTaskPort<'a, B> {
    pub(super) fn new(service: &'a ActionsService, runner: &'a CachedRunner, task: TaskId) -> Self {
        Self { service, runner, task, run: OnceLock::new(), body: PhantomData }
    }

    pub(super) fn run_id(&self) -> Option<RunId> {
        self.run.get().copied()
    }
}

impl<B> ArchiveFiles for CacheTaskPort<'_, B> {
    type Error = RunnerError;
    type Location = PathBuf;

    async fn remove(&self, location: &PathBuf) {
        archive_files::remove_file(location).await;
    }
}

cache_records!(impl[B] CacheTaskPort<'_, B>, RunnerError);

impl<B> CacheTaskStore for CacheTaskPort<'_, B> {
    type Target = (Task, Run, CacheScopes);
    type Opened = tokio::fs::File;

    async fn lease(&self) -> Result<(Task, Run, CacheScopes), RunnerError> {
        let target = self.service.cache_task(self.runner, self.task).await?;
        let _ = self.run.set(target.1.id);
        Ok(target)
    }

    fn repository(&self, target: &(Task, Run, CacheScopes)) -> i64 {
        target.1.repo_id.0
    }

    fn scopes<'t>(&self, target: &'t (Task, Run, CacheScopes)) -> &'t CacheScopes {
        &target.2
    }

    async fn exact(&self, repository: i64, scope: &str, key: &str, version: &str) -> Result<Option<CacheEntry>, RunnerError> {
        Ok(self.service.ctx().database.actions_caches().exact(RepoId(repository), scope, key, version).await?)
    }

    async fn newest_with_prefix(
        &self,
        repository: i64,
        scope: &str,
        prefix: &str,
        version: &str,
    ) -> Result<Option<CacheEntry>, RunnerError> {
        Ok(self.service.ctx().database.actions_caches().newest_with_prefix(RepoId(repository), scope, prefix, version).await?)
    }

    async fn find(&self, id: i64) -> Result<Option<CacheEntry>, RunnerError> {
        Ok(self.service.ctx().database.actions_caches().find(CacheEntryId(id)).await?)
    }

    async fn open(&self, entry: &CacheEntry) -> Result<tokio::fs::File, RunnerError> {
        tokio::fs::File::open(self.service.cache_path(entry)).await.map_err(|_| RunnerError::not_found("the cache entry's file is gone"))
    }

    async fn touch(&self, entry: &CacheEntry) -> Result<(), RunnerError> {
        Ok(self.service.ctx().database.actions_caches().touch(entry.id, Timestamp::now()).await?)
    }
}

impl<B: AsyncRead + Unpin + Send> ArchiveUploads for CacheTaskPort<'_, B> {
    type Body = B;
    type Temporary = PathBuf;

    async fn receive(&self, body: B, limit: u64) -> Result<Option<Received<PathBuf>>, RunnerError> {
        Ok(archive_files::receive_upload(&self.service.caches_dir(), self.task, body, limit).await?)
    }

    async fn discard(&self, temporary: &PathBuf) {
        archive_files::discard(temporary).await;
    }

    async fn place(&self, temporary: &PathBuf, location: &PathBuf) -> Result<(), RunnerError> {
        Ok(archive_files::place(&self.service.caches_dir(), temporary, location).await?)
    }
}

impl<B: AsyncRead + Unpin + Send> CacheSaveStore for CacheTaskPort<'_, B> {
    async fn still_leased(&self, target: &(Task, Run, CacheScopes)) -> Result<(), RunnerError> {
        self.service.leased_task(self.runner, target.0.id).await.map(drop)
    }

    async fn record(&self, new: &NewCacheEntry<'_>, replace: bool) -> Result<CacheRecorded<CacheEntry>, RunnerError> {
        let row = EntryRow {
            repo: RepoId(new.repository),
            scope: new.scope,
            key: new.key,
            version: new.version,
            size: new.size,
            sha256: new.sha256,
            now: Timestamp::now(),
        };
        Ok(match self.service.ctx().database.actions_caches().record(&row, replace).await? {
            Recorded::Stored { entry, replaced } => CacheRecorded::Stored { entry: *entry, replaced: replaced.map(|replaced| *replaced) },
            Recorded::Exists => CacheRecorded::Exists,
        })
    }
}

/// Stored entries, for REST deletes and the sweep.
pub(super) struct CacheRows<'a> {
    service: &'a ActionsService,
}

impl<'a> CacheRows<'a> {
    pub(super) fn new(service: &'a ActionsService) -> Self {
        Self { service }
    }
}

impl ArchiveFiles for CacheRows<'_> {
    type Error = ServiceError;
    type Location = PathBuf;

    async fn remove(&self, location: &PathBuf) {
        archive_files::remove_file(location).await;
    }
}

cache_records!(impl[] CacheRows<'_>, ServiceError);

impl CacheRetention for CacheRows<'_> {
    async fn unused_since(&self, cutoff: i64, limit: u32) -> Result<Vec<CacheEntry>, ServiceError> {
        Ok(self.service.ctx().database.actions_caches().unused_since(Timestamp(cutoff), limit).await?)
    }
}
