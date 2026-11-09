//! Wiki data follows its repository.
//!
//! Wikis are keyed by the stable repository ID, both in the catalog and on disk
//! (`wikis/<repository>/<wiki>.git`), so renaming or transferring a repository keeps
//! its wiki, history and links unchanged while the destination's current permissions
//! apply at once (every read and change authorizes the current viewer in SQL).
//!
//! Deleting a repository is permanent, as for its Git repository and release files:
//! the catalog rows go with the repository row (foreign-key cascades), and this
//! subscriber then removes the wiki's objects. A repository ID can be reused, so only
//! wiki incarnations the catalog no longer knows are removed, never the current wiki of
//! a reused ID, nor a wiki whose accepted intents are still pending. A removal that
//! fails (or an event missed while the server was down) is retried by the sweep at the
//! next start, which applies the same rule to every repository directory.
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_core::RepoId;
use rbit_git::wikis::lifecycle::remove_repository;
use tokio::sync::broadcast::{Receiver, error::RecvError};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use super::{errors::WikiError, pages::WikiService};
use crate::{DomainEvent, EventBus, ServiceError};

/// How long shutdown waits for a removal in progress.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// The deletion subscriber and the start-up sweep.
#[derive(Debug, Default)]
pub(super) struct WikiBackground {
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl WikiService {
    /// Starts the subscriber that removes deleted repositories' wiki objects and sweeps
    /// what earlier deletions left. Called once at start-up; later calls do nothing.
    pub fn spawn_background(&self, events: &EventBus) {
        if self.background.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let receiver = events.subscribe();
        let service = self.clone();
        self.background.tasks.spawn(async move {
            service.sweep().await;
            service.listen(receiver).await;
        });
    }

    /// Stops the subscriber and waits (bounded) for a removal in progress.
    pub async fn shutdown(&self) {
        self.background.shutdown.cancel();
        self.background.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.background.tasks.wait()).await.is_err() {
            tracing::warn!("wiki clean-up did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn listen(&self, mut events: Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.background.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(DomainEvent::RepositoryDeleted { repo, .. }) => {
                    if let Err(error) = self.remove_repository_objects(repo).await {
                        tracing::warn!(%error, repo = repo.0, "cannot remove the wiki of a deleted repository; retried at the next start");
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "wiki clean-up fell behind; the next start removes what it missed");
                }
                Err(RecvError::Closed) => break,
            }
        }
    }

    /// Removes the wiki objects of `repository` that its catalog no longer knows; returns
    /// how many wiki stores were removed. Serialized with page changes.
    pub async fn remove_repository_objects(&self, repository: RepoId) -> Result<usize, WikiError> {
        if !self.root.join(repository.0.to_string()).exists() {
            return Ok(0);
        }
        let _one_change = self.writes.lock().await;
        let keep = self.catalog.retained_wikis(repository).await?;
        let identity = self.catalog.identity().await?;
        let root = Arc::clone(&self.root);
        let removed = tokio::task::spawn_blocking(move || remove_repository(&root, identity, repository, &keep))
            .await
            .map_err(|error| WikiError::Internal(ServiceError::internal(error)))??;
        if removed > 0 {
            tracing::info!(repo = repository.0, removed, "removed the wiki of a deleted repository");
        }
        Ok(removed)
    }

    /// Applies [`Self::remove_repository_objects`] to every repository directory.
    async fn sweep(&self) {
        let Ok(entries) = std::fs::read_dir(self.root.as_path()) else { return };
        let repositories: Vec<RepoId> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str().and_then(|name| name.parse::<i64>().ok()))
            .filter(|id| *id > 0)
            .map(RepoId)
            .collect();
        for repository in repositories {
            if self.background.shutdown.is_cancelled() {
                break;
            }
            if let Err(error) = self.remove_repository_objects(repository).await {
                tracing::warn!(%error, repo = repository.0, "cannot sweep the wiki objects of a deleted repository");
            }
        }
    }

    /// The dedicated object directory.
    pub fn root(&self) -> &Path {
        &self.root
    }
}
