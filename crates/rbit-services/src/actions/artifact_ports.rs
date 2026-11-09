//! The rbit-actions artifact store's ports over `actions_artifacts` and the files below
//! `<data_dir>/actions/artifacts`: one upload port per request (its task's lease, body and
//! temporary file) and one row port for removal and the sweep. Policy is not decided here.

use std::{marker::PhantomData, path::PathBuf};

use rbit_actions::{
    archives::{ArchiveFiles, ArchiveUploads, Received},
    artifacts::{ArtifactRecorded, ArtifactRecords, ArtifactRetention, ArtifactUploadStore, NewArtifact},
};
use rbit_core::{
    Timestamp,
    actions::{Artifact, Job, Run, Task, TaskId},
};
use rbit_storage::sqlite::actions_artifacts::{NewArtifact as ArtifactRow, Recorded};
use tokio::io::AsyncRead;

use super::{
    ActionsService, archive_files,
    runners::{CachedRunner, RunnerError},
};
use crate::ServiceError;

/// One runner's upload through its task `task`, with body `B`.
pub(super) struct ArtifactUploadPort<'a, B> {
    service: &'a ActionsService,
    runner: &'a CachedRunner,
    task: TaskId,
    body: PhantomData<fn(B)>,
}

impl<'a, B> ArtifactUploadPort<'a, B> {
    pub(super) fn new(service: &'a ActionsService, runner: &'a CachedRunner, task: TaskId) -> Self {
        Self { service, runner, task, body: PhantomData }
    }
}

impl<B> ArchiveFiles for ArtifactUploadPort<'_, B> {
    type Error = RunnerError;
    type Location = PathBuf;

    async fn remove(&self, location: &PathBuf) {
        archive_files::remove_file(location).await;
    }
}

impl<B: AsyncRead + Unpin + Send> ArchiveUploads for ArtifactUploadPort<'_, B> {
    type Body = B;
    type Temporary = PathBuf;

    async fn receive(&self, body: B, limit: u64) -> Result<Option<Received<PathBuf>>, RunnerError> {
        Ok(archive_files::receive_upload(&self.service.artifacts_dir(), self.task, body, limit).await?)
    }

    async fn discard(&self, temporary: &PathBuf) {
        archive_files::discard(temporary).await;
    }

    async fn place(&self, temporary: &PathBuf, location: &PathBuf) -> Result<(), RunnerError> {
        Ok(archive_files::place(&self.service.artifacts_dir(), temporary, location).await?)
    }
}

impl<B> ArtifactRecords for ArtifactUploadPort<'_, B> {
    type Artifact = Artifact;

    fn location(&self, artifact: &Artifact) -> PathBuf {
        self.service.artifact_path(artifact)
    }

    async fn delete(&self, artifact: &Artifact) -> Result<(), RunnerError> {
        self.service.ctx().database.actions_artifacts().delete(artifact.id).await?;
        Ok(())
    }
}

impl<B: AsyncRead + Unpin + Send> ArtifactUploadStore for ArtifactUploadPort<'_, B> {
    type Target = (Task, Job, Run);

    async fn lease(&self) -> Result<(Task, Job, Run), RunnerError> {
        let (task, job) = self.service.leased_task(self.runner, self.task).await?;
        let run = self.service.ctx().database.actions_runs().run(job.run_id).await?;
        let run = run.ok_or_else(|| RunnerError::not_found("there is no such run"))?;
        Ok((task, job, run))
    }

    async fn still_leased(&self, target: &(Task, Job, Run)) -> Result<(), RunnerError> {
        self.service.leased_task(self.runner, target.0.id).await.map(drop)
    }

    async fn stored_bytes(&self, target: &(Task, Job, Run)) -> Result<u64, RunnerError> {
        Ok(self.service.ctx().database.actions_artifacts().stored_bytes(target.2.repo_id).await?)
    }

    fn now(&self) -> i64 {
        Timestamp::now().0
    }

    async fn record(&self, new: &NewArtifact<'_, (Task, Job, Run)>, overwrite: bool) -> Result<ArtifactRecorded<Artifact>, RunnerError> {
        let (_, job, run) = new.target;
        let row = ArtifactRow {
            run: run.id,
            repo: run.repo_id,
            attempt: run.attempt,
            job: job.id,
            name: new.name,
            size: new.size,
            sha256: new.sha256,
            now: Timestamp(new.now),
            expires_at: Timestamp(new.expires_at),
        };
        Ok(match self.service.ctx().database.actions_artifacts().record(&row, overwrite).await? {
            Recorded::Stored { artifact, replaced } => {
                ArtifactRecorded::Stored { artifact: *artifact, replaced: replaced.map(|replaced| *replaced) }
            }
            Recorded::Exists => ArtifactRecorded::Exists,
        })
    }
}

/// Stored artifacts, for removal and the sweep.
pub(super) struct ArtifactRows<'a> {
    service: &'a ActionsService,
}

impl<'a> ArtifactRows<'a> {
    pub(super) fn new(service: &'a ActionsService) -> Self {
        Self { service }
    }
}

impl ArchiveFiles for ArtifactRows<'_> {
    type Error = ServiceError;
    type Location = PathBuf;

    async fn remove(&self, location: &PathBuf) {
        archive_files::remove_file(location).await;
    }
}

impl ArtifactRecords for ArtifactRows<'_> {
    type Artifact = Artifact;

    fn location(&self, artifact: &Artifact) -> PathBuf {
        self.service.artifact_path(artifact)
    }

    async fn delete(&self, artifact: &Artifact) -> Result<(), ServiceError> {
        self.service.ctx().database.actions_artifacts().delete(artifact.id).await?;
        Ok(())
    }
}

impl ArtifactRetention for ArtifactRows<'_> {
    async fn expired(&self, now: i64, limit: u32) -> Result<Vec<Artifact>, ServiceError> {
        Ok(self.service.ctx().database.actions_artifacts().expired(Timestamp(now), limit).await?)
    }
}
