//! Artifacts addressed by commit and name, and promoted instead of rebuilt (agent plan step
//! 4.6). A release attaches the files of an artifact CI already built for a commit, so
//! what ships is what was tested.

use std::path::PathBuf;

use rbit_core::actions::{Artifact, ArtifactId, Run, RunEvent};

use crate::{ActionsService, RepoContext, ServiceError};

/// Files an artifact may hold to be promoted.
pub const MAX_PROMOTED_FILES: usize = 20;
/// Artifacts of a name looked through for a commit's.
const SEARCHED: u32 = 100;

impl ActionsService {
    /// The newest artifact named `name` of a trusted run of commit `sha` (a full id, or a
    /// prefix of at least 7 digits naming one commit of `repo`) in `repo`. A pull request's
    /// run (a fork's code) or an untrusted one never ships: its artifact is not found here.
    pub async fn artifact_of_commit(&self, repo: &RepoContext, sha: &str, name: &str) -> Result<Artifact, ServiceError> {
        if sha.len() < 7 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ServiceError::Invalid("sha must be a commit id or a prefix of at least 7 digits".to_owned()));
        }
        // The commit itself, not whatever run's commit starts with the digits: a prefix is a
        // few minutes' work to collide with.
        let commit = repo.reader.commit_id(sha.to_ascii_lowercase()).await.map_err(|_| ServiceError::NotFound)?;
        let (artifacts, _) = self.repo_artifacts(repo, Some(name), 0, SEARCHED).await?;
        let store = self.ctx().database.actions_runs();
        for artifact in artifacts {
            if let Some(run) = store.run(artifact.run_id).await?
                && run.sha == commit
                && shippable(&run)
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
        let run = self.ctx().database.actions_runs().run(artifact.run_id).await?.ok_or(ServiceError::NotFound)?;
        if !shippable(&run) {
            return Err(ServiceError::Invalid(
                "an artifact of a pull request's or an untrusted run is not promoted to a release".to_owned(),
            ));
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos());
        let dir = self.ctx().config.storage.tmp_dir().join(format!("rbit-promote-{}-{}-{nanos:x}", artifact.id.0, std::process::id()));
        tokio::fs::create_dir_all(&dir).await.map_err(ServiceError::internal)?;
        let unpacked = Unpacked(dir.clone());
        let file = file.into_std().await;
        // Each file becomes a release asset, which may be no larger than this.
        let max_bytes = self.ctx().config.releases.max_asset_bytes;
        let files = tokio::task::spawn_blocking(move || rbit_actions::downloads::extract_files(file, &dir, MAX_PROMOTED_FILES, max_bytes))
            .await
            .map_err(ServiceError::internal)?
            .map_err(|error| ServiceError::Invalid(error.to_string()))?;
        Ok((artifact, unpacked, files))
    }
}

/// Whether `run`'s artifacts may ship: a trusted run of the repository's own code, not a
/// pull request's (whose head may be a fork's) nor one that waited for approval.
fn shippable(run: &Run) -> bool {
    ships(rbit_actions::trust::RunTrust::from_stored(run.trusted).is_trusted(), run.is_fork, run.event)
}

const fn ships(trusted: bool, is_fork: bool, event: RunEvent) -> bool {
    trusted && !is_fork && !matches!(event, RunEvent::PullRequest)
}

/// A directory of unpacked artifact files, deleted when dropped.
#[derive(Debug)]
pub struct Unpacked(PathBuf);

impl Drop for Unpacked {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_trusted_runs_of_the_repositorys_own_code_ship() {
        assert!(ships(true, false, RunEvent::Push));
        assert!(ships(true, false, RunEvent::WorkflowDispatch));
        assert!(!ships(true, false, RunEvent::PullRequest), "a pull request's head may be anyone's");
        assert!(!ships(true, true, RunEvent::Push), "a fork's");
        assert!(!ships(false, false, RunEvent::Push), "one that waited for approval");
    }
}
