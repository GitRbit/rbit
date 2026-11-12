//! Artifacts addressed by commit and name, and promoted instead of rebuilt (agent plan step
//! 4.6). A release attaches the files of an artifact CI already built for a commit, so
//! what ships is what was tested.

use std::path::PathBuf;

use rbit_core::actions::{Artifact, ArtifactId};

use crate::{ActionsService, RepoContext, ServiceError};

/// Files an artifact may hold to be promoted.
pub const MAX_PROMOTED_FILES: usize = 20;
/// Artifacts of a name looked through for a commit's.
const SEARCHED: u32 = 100;

impl ActionsService {
    /// The newest artifact named `name` of a run of commit `sha` (a full id or a prefix of
    /// at least 7 digits) in `repo`.
    pub async fn artifact_of_commit(&self, repo: &RepoContext, sha: &str, name: &str) -> Result<Artifact, ServiceError> {
        if sha.len() < 7 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ServiceError::Invalid("sha must be a commit id or a prefix of at least 7 digits".to_owned()));
        }
        let (artifacts, _) = self.repo_artifacts(repo, Some(name), 0, SEARCHED).await?;
        let store = self.ctx().database.actions_runs();
        for artifact in artifacts {
            if let Some(run) = store.run(artifact.run_id).await?
                && run.sha.starts_with(&sha.to_ascii_lowercase())
            {
                return Ok(artifact);
            }
        }
        Err(ServiceError::NotFound)
    }

    /// The files of artifact `id` of `repo`, written into a new directory under the
    /// server's temporary directory (deleted when the returned guard drops).
    pub async fn artifact_files(
        &self,
        repo: &RepoContext,
        id: ArtifactId,
    ) -> Result<(Artifact, Unpacked, Vec<(String, PathBuf)>), ServiceError> {
        let (artifact, file) = self.open_artifact(repo, id).await?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos());
        let dir = self.ctx().config.storage.tmp_dir().join(format!("rbit-promote-{}-{}-{nanos:x}", artifact.id.0, std::process::id()));
        tokio::fs::create_dir_all(&dir).await.map_err(ServiceError::internal)?;
        let unpacked = Unpacked(dir.clone());
        let file = file.into_std().await;
        let files = tokio::task::spawn_blocking(move || rbit_actions::downloads::extract_files(file, &dir, MAX_PROMOTED_FILES))
            .await
            .map_err(ServiceError::internal)?
            .map_err(|error| ServiceError::Invalid(error.to_string()))?;
        Ok((artifact, unpacked, files))
    }
}

/// A directory of unpacked artifact files, deleted when dropped.
#[derive(Debug)]
pub struct Unpacked(PathBuf);

impl Drop for Unpacked {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
