//! Workflow artifacts (§12.7 of `docs/design/actions.md`): files a job uploads, one tar.gz
//! each, for later jobs of the run and for people to download.
//!
//! * Files live at `<data_dir>/actions/artifacts/<run id>/<artifact id>.tar.gz`, outside
//!   backups like logs. An upload streams to a temporary file beside them while its size
//!   and SHA-256 are counted, is refused past `[actions] max_artifact_bytes` or the
//!   repository's `max_artifact_storage_bytes`, must match the digest the runner sent, and
//!   only then is recorded and moved into place.
//! * Names are 1 to 128 of `[A-Za-z0-9._-]`, unique per run attempt; `overwrite` replaces.
//! * An artifact is kept `artifact_retention_days`, or fewer when the workflow asks; the
//!   sweeper deletes expired ones with their files.
//! * Runners upload and list only through their own running task; people download from
//!   a run they may read. Untrusted runs may upload too, within the same quota.
//!
//! Workstream A (artifacts) owns this file.

use std::{
    io,
    path::{Path, PathBuf},
};

use rbit_core::{
    Timestamp,
    actions::{Artifact, ArtifactId, RunId, Task, valid_artifact_name},
};
use rbit_storage::sqlite::actions_artifacts::{NewArtifact, Recorded};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::{ActionsService, runners::CachedRunner, runners::RunnerError};
use crate::{RepoContext, ServiceError};

const SECONDS_PER_DAY: i64 = 86_400;
/// Most expired artifacts the sweeper removes in one pass.
const SWEEP_ARTIFACTS: u32 = 256;

/// What a runner sends with an upload.
#[derive(Debug, Clone)]
pub struct Upload<'a> {
    pub name: &'a str,
    /// The tar.gz's SHA-256, as 64 hex digits.
    pub sha256: &'a str,
    /// `retention-days`; it can only lower the instance's.
    pub retention_days: Option<u32>,
    pub overwrite: bool,
}

/// `hex` as 32 bytes.
fn digest_from_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(digest)
}

/// The newest attempt's artifact of each name.
fn latest_per_name(artifacts: Vec<Artifact>) -> Vec<Artifact> {
    let mut seen = std::collections::BTreeSet::new();
    let mut latest: Vec<Artifact> = artifacts.into_iter().filter(|artifact| seen.insert(artifact.name.clone())).collect();
    latest.sort_by(|a, b| a.name.cmp(&b.name));
    latest
}

impl ActionsService {
    /// `<data_dir>/actions/artifacts`.
    fn artifacts_dir(&self) -> PathBuf {
        self.ctx().config.storage.actions_dir().join("artifacts")
    }

    /// Where artifact `artifact`'s file is.
    fn artifact_path(&self, artifact: &Artifact) -> PathBuf {
        self.artifacts_dir().join(artifact.run_id.0.to_string()).join(format!("{}.tar.gz", artifact.id.0))
    }

    /// Task `id` of `runner`, running.
    async fn running_task(&self, runner: &CachedRunner, id: rbit_core::actions::TaskId) -> Result<Task, RunnerError> {
        let task = self.ctx().database.actions_runs().task(id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        if task.runner_id != Some(runner.id) {
            return Err(RunnerError::forbidden("this task belongs to another runner"));
        }
        if task.status.is_done() {
            return Err(RunnerError::conflict("the task has ended"));
        }
        Ok(task)
    }

    /// Stores what `body` uploads as artifact `upload.name` of task `task`'s run.
    pub async fn upload_artifact(
        &self,
        runner: &CachedRunner,
        task: rbit_core::actions::TaskId,
        upload: &Upload<'_>,
        mut body: impl AsyncRead + Unpin,
    ) -> Result<Artifact, RunnerError> {
        if !valid_artifact_name(upload.name) {
            return Err(RunnerError::bad_request("artifact names are 1 to 128 letters, digits, '.', '_' or '-'"));
        }
        let expected =
            digest_from_hex(upload.sha256).ok_or_else(|| RunnerError::bad_request("the content digest must be 64 hex digits"))?;
        let task = self.running_task(runner, task).await?;
        let database = &self.ctx().database;
        let job = database.actions_runs().job(task.job_id).await?.ok_or_else(|| RunnerError::not_found("there is no such job"))?;
        let run = database.actions_runs().run(job.run_id).await?.ok_or_else(|| RunnerError::not_found("there is no such run"))?;
        let config = self.config();
        let stored = database.actions_artifacts().stored_bytes(run.repo_id).await?;
        let room = config.max_artifact_storage_bytes.saturating_sub(stored).min(config.max_artifact_bytes);
        if room == 0 {
            return Err(RunnerError::too_large("the repository's artifact storage is full"));
        }

        let dir = self.artifacts_dir();
        tokio::fs::create_dir_all(dir.join("tmp")).await.map_err(ServiceError::internal)?;
        let temporary = dir.join("tmp").join(format!("{}-{}.part", task.id.0, super::tokens::random_name()?));
        let received = receive(&mut body, &temporary, room).await;
        let (size, digest) = match received {
            Ok(Some(received)) => received,
            Ok(None) => {
                let _ = tokio::fs::remove_file(&temporary).await;
                let message = if room < config.max_artifact_bytes {
                    "the artifact does not fit in the repository's artifact storage"
                } else {
                    "the artifact is larger than this instance allows"
                };
                return Err(RunnerError::too_large(message));
            }
            Err(error) => {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(ServiceError::internal(error).into());
            }
        };
        if digest != expected {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(RunnerError::bad_request("the artifact does not match its content digest"));
        }

        let now = Timestamp::now();
        let days = upload.retention_days.map_or(config.artifact_retention_days, |days| days.clamp(1, config.artifact_retention_days));
        let new = NewArtifact {
            run: run.id,
            repo: run.repo_id,
            attempt: run.attempt,
            job: job.id,
            name: upload.name,
            size,
            sha256: digest,
            now,
            expires_at: Timestamp(now.0 + i64::from(days) * SECONDS_PER_DAY),
        };
        let store = database.actions_artifacts();
        let (artifact, replaced) = match store.record(&new, upload.overwrite).await? {
            Recorded::Stored { artifact, replaced } => (*artifact, replaced),
            Recorded::Exists => {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(RunnerError::conflict(format!(
                    "this run already has an artifact `{}`; set `overwrite: true` to replace it",
                    upload.name
                )));
            }
        };
        // The replaced file goes first: SQLite may give the new row the old one's id, and so
        // the same path.
        if let Some(replaced) = &replaced {
            remove_file(&self.artifact_path(replaced)).await;
        }
        let path = self.artifact_path(&artifact);
        let placed = async {
            tokio::fs::create_dir_all(path.parent().unwrap_or(&dir)).await?;
            tokio::fs::rename(&temporary, &path).await
        }
        .await;
        if let Err(error) = placed {
            let _ = tokio::fs::remove_file(&temporary).await;
            store.delete(artifact.id).await?;
            return Err(ServiceError::internal(error).into());
        }
        tracing::info!(run = run.id.0, artifact = %artifact.name, size, "artifact uploaded");
        Ok(artifact)
    }

    /// The artifacts task `task`'s run has so far, the newest attempt's of each name.
    pub async fn task_artifacts(&self, runner: &CachedRunner, task: rbit_core::actions::TaskId) -> Result<Vec<Artifact>, RunnerError> {
        let task = self.running_task(runner, task).await?;
        let job =
            self.ctx().database.actions_runs().job(task.job_id).await?.ok_or_else(|| RunnerError::not_found("there is no such job"))?;
        Ok(latest_per_name(self.ctx().database.actions_artifacts().of_run(job.run_id, Timestamp::now()).await?))
    }

    /// Artifact `name` of task `task`'s run, and its file.
    pub async fn open_task_artifact(
        &self,
        runner: &CachedRunner,
        task: rbit_core::actions::TaskId,
        name: &str,
    ) -> Result<(Artifact, tokio::fs::File), RunnerError> {
        let artifact = self
            .task_artifacts(runner, task)
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
        Ok(latest_per_name(artifacts.into_iter().filter(|artifact| artifact.repo_id == repo.repo.repo.id).collect()))
    }

    /// Artifact `id` of `repo` for a reader, and its file.
    pub async fn open_artifact(&self, repo: &RepoContext, id: ArtifactId) -> Result<(Artifact, tokio::fs::File), ServiceError> {
        self.require_pages(repo).await?;
        let artifact =
            self.ctx().database.actions_artifacts().find(repo.repo.repo.id, id, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;
        let file = tokio::fs::File::open(self.artifact_path(&artifact)).await.map_err(|_| ServiceError::NotFound)?;
        Ok((artifact, file))
    }

    /// Deletes artifacts that expired by `now`, with their files; how many.
    pub(super) async fn expire_artifacts(&self, now: Timestamp) -> Result<usize, ServiceError> {
        let store = self.ctx().database.actions_artifacts();
        let expired = store.expired(now, SWEEP_ARTIFACTS).await?;
        for artifact in &expired {
            remove_file(&self.artifact_path(artifact)).await;
            store.delete(artifact.id).await?;
        }
        Ok(expired.len())
    }
}

/// Copies `body` into a new file at `path`, counting its size and SHA-256; `None` once it
/// passes `limit` bytes.
async fn receive(body: &mut (impl AsyncRead + Unpin), path: &Path, limit: u64) -> io::Result<Option<(u64, [u8; 32])>> {
    let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(path).await?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = body.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > limit {
            return Ok(None);
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).await?;
    }
    file.sync_all().await?;
    Ok(Some((size, hasher.finalize().into())))
}

/// Removes `path`, logging anything but its absence.
async fn remove_file(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, path = %path.display(), "cannot remove an artifact file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_are_64_hex_digits() {
        let digest = Sha256::digest(b"x");
        let hex = digest.iter().fold(String::new(), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        });
        assert_eq!(digest_from_hex(&hex), Some(digest.into()));
        assert_eq!(digest_from_hex(&hex.to_uppercase()), Some(digest.into()));
        assert_eq!(digest_from_hex(&hex[..62]), None);
        assert_eq!(digest_from_hex(&"zz".repeat(32)), None);
    }
}
