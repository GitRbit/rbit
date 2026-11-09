//! Workflow artifacts (§12.7 of `docs/design/actions.md`): the composition of
//! [`rbit_actions::artifacts`], which decides uploads, quotas, retention and removal.
//!
//! * Files live at `<data_dir>/actions/artifacts/<run id>/<artifact id>.tar.gz`, backed up
//!   like logs; [`super::artifact_ports`] implements the store's ports over them and
//!   `actions_artifacts`.
//! * Runners upload and list only through their own running task; people download from
//!   a run they may read. Untrusted runs may upload too, within the same quota.

use std::path::PathBuf;

pub use rbit_actions::artifacts::ArtifactUpload as Upload;
use rbit_actions::artifacts::{ArtifactLimits, ArtifactStorage, latest_per_name};
use rbit_core::{
    Timestamp, User,
    actions::{Artifact, ArtifactId, Job, RunId, Task},
};
use tokio::io::AsyncRead;

use super::{
    ActionsService,
    archive_files::answer,
    artifact_ports::{ArtifactRows, ArtifactUploadPort},
    runners::{CachedRunner, RunnerError},
};
use crate::{RepoContext, ServiceError};

/// The newest attempt's artifact of each name.
fn latest(artifacts: Vec<Artifact>) -> Vec<Artifact> {
    latest_per_name(artifacts, |artifact| artifact.name.as_str())
}

impl ActionsService {
    /// `<data_dir>/actions/artifacts`.
    pub(super) fn artifacts_dir(&self) -> PathBuf {
        self.ctx().config.storage.actions_dir().join("artifacts")
    }

    /// Where artifact `artifact`'s file is.
    pub(super) fn artifact_path(&self, artifact: &Artifact) -> PathBuf {
        self.artifacts_dir().join(artifact.run_id.0.to_string()).join(format!("{}.tar.gz", artifact.id.0))
    }

    /// Task `id` of `runner`, running.
    pub(super) async fn running_task(&self, runner: &CachedRunner, id: rbit_core::actions::TaskId) -> Result<Task, RunnerError> {
        let task = self.ctx().database.actions_runs().task(id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        if task.runner_id != Some(runner.id) {
            return Err(RunnerError::forbidden("this task belongs to another runner"));
        }
        if task.status.is_done() {
            return Err(RunnerError::conflict("the task has ended"));
        }
        Ok(task)
    }

    /// Task `id` of `runner` while it holds its job's lease: running, and the job's current
    /// attempt (a task left behind by a rerun would otherwise file under the new one).
    /// Uploads check it before and again after their body.
    pub(super) async fn leased_task(&self, runner: &CachedRunner, id: rbit_core::actions::TaskId) -> Result<(Task, Job), RunnerError> {
        let task = self.running_task(runner, id).await?;
        let job = self.ctx().database.actions_runs().job(task.job_id).await?;
        let job = job.ok_or_else(|| RunnerError::not_found("there is no such job"))?;
        if job.task_id != Some(task.id) {
            return Err(RunnerError::conflict("the job has been rerun; this task no longer belongs to it"));
        }
        Ok((task, job))
    }

    /// Stores what `body` uploads as artifact `upload.name` of task `task`'s run.
    pub async fn upload_artifact(
        &self,
        runner: &CachedRunner,
        task: rbit_core::actions::TaskId,
        upload: &Upload<'_>,
        body: impl AsyncRead + Unpin + Send,
    ) -> Result<Artifact, RunnerError> {
        let config = self.config();
        let limits = ArtifactLimits {
            max_artifact_bytes: config.max_artifact_bytes,
            max_storage_bytes: config.max_artifact_storage_bytes,
            retention_days: config.artifact_retention_days,
        };
        let port = ArtifactUploadPort::new(self, runner, task);
        let artifact = ArtifactStorage::upload(&port, &limits, upload, body).await.map_err(answer)?;
        tracing::info!(run = artifact.run_id.0, artifact = %artifact.name, size = artifact.size, "artifact uploaded");
        Ok(artifact)
    }

    /// The artifacts task `task`'s run has so far, the newest attempt's of each name.
    ///
    /// `run` (`actions/download-artifact`'s `run-id`) reads another run of the task's
    /// repository instead, as the task's token may on GitHub.
    pub async fn task_artifacts(
        &self,
        runner: &CachedRunner,
        task: rbit_core::actions::TaskId,
        run: Option<RunId>,
    ) -> Result<Vec<Artifact>, RunnerError> {
        let task = self.running_task(runner, task).await?;
        let store = self.ctx().database.actions_runs();
        let job = store.job(task.job_id).await?.ok_or_else(|| RunnerError::not_found("there is no such job"))?;
        let run = match run {
            None => job.run_id,
            Some(run) => {
                store.find_run(job.repo_id, run).await?.ok_or_else(|| RunnerError::not_found("this repository has no such run"))?.id
            }
        };
        let artifacts = self.ctx().database.actions_artifacts().of_run(run, Timestamp::now()).await?;
        Ok(latest(artifacts.into_iter().filter(|artifact| artifact.repo_id == job.repo_id).collect()))
    }

    /// Artifact `name` of task `task`'s run, and its file.
    pub async fn open_task_artifact(
        &self,
        runner: &CachedRunner,
        task: rbit_core::actions::TaskId,
        run: Option<RunId>,
        name: &str,
    ) -> Result<(Artifact, tokio::fs::File), RunnerError> {
        let artifact = self
            .task_artifacts(runner, task, run)
            .await?
            .into_iter()
            .find(|artifact| artifact.name == name)
            .ok_or_else(|| RunnerError::not_found(format!("this run has no artifact `{name}`")))?;
        let file = tokio::fs::File::open(self.artifact_path(&artifact))
            .await
            .map_err(|_| RunnerError::not_found("the artifact's file is gone"))?;
        Ok((artifact, file))
    }

    /// The artifacts of run `run` of `repo` for a reader, the newest attempt's of each name.
    pub async fn run_artifacts(&self, repo: &RepoContext, run: RunId) -> Result<Vec<Artifact>, ServiceError> {
        self.require_pages(repo).await?;
        let artifacts = self.ctx().database.actions_artifacts().of_run(run, Timestamp::now()).await?;
        Ok(latest(artifacts.into_iter().filter(|artifact| artifact.repo_id == repo.repo.repo.id).collect()))
    }

    /// Artifact `id` of `repo` for a reader, and its file.
    pub async fn open_artifact(&self, repo: &RepoContext, id: ArtifactId) -> Result<(Artifact, tokio::fs::File), ServiceError> {
        self.require_pages(repo).await?;
        let artifact =
            self.ctx().database.actions_artifacts().find(repo.repo.repo.id, id, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;
        let file = tokio::fs::File::open(self.artifact_path(&artifact)).await.map_err(|_| ServiceError::NotFound)?;
        Ok((artifact, file))
    }

    // Actions: REST API.
    /// A page of `repo`'s artifacts for a reader, newest first, only those named `name` when
    /// given, and how many there are in all.
    pub async fn repo_artifacts(
        &self,
        repo: &RepoContext,
        name: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Artifact>, u64), ServiceError> {
        self.require_pages(repo).await?;
        Ok(self.ctx().database.actions_artifacts().of_repo(repo.repo.repo.id, name, Timestamp::now(), offset, limit).await?)
    }

    /// Artifact `id` of `repo` for a reader.
    pub async fn artifact(&self, repo: &RepoContext, id: ArtifactId) -> Result<Artifact, ServiceError> {
        self.require_pages(repo).await?;
        let store = self.ctx().database.actions_artifacts();
        store.find(repo.repo.repo.id, id, Timestamp::now()).await?.ok_or(ServiceError::NotFound)
    }

    /// Deletes artifact `id` of `repo` with its file, for someone who may write to it.
    pub async fn delete_artifact(&self, actor: &User, repo: &RepoContext, id: ArtifactId) -> Result<(), ServiceError> {
        let artifact = self.artifact(repo, id).await?;
        if !repo.access.can_write() {
            return Err(ServiceError::Forbidden);
        }
        repo.ensure_writable()?;
        ArtifactStorage::remove(&ArtifactRows::new(self), &artifact).await?;
        tracing::info!(repo = %repo.repo.full_name(), artifact = artifact.id.0, name = %artifact.name, by = %actor.name(), "artifact deleted");
        Ok(())
    }

    /// Deletes artifacts that expired by `now`, with their files; how many.
    pub(super) async fn expire_artifacts(&self, now: Timestamp) -> Result<usize, ServiceError> {
        ArtifactStorage::expire(&ArtifactRows::new(self), now.0).await
    }
}
