//! Recording accepted pushes.
//!
//! Both Git transports (smart HTTP and SSH) call [`PushService::begin`] before running
//! `git receive-pack` and [`PushService::complete`] once it succeeded; so does everything
//! else that changes refs (mirror fetches, merges, browser edits, release tags, deleting a
//! merged head branch). Completing a push marks the repository as pushed and publishes
//! [`DomainEvent::RepositoryPushed`] with the refs that changed, computed from snapshots
//! taken before and after (`receive-pack` runs without hooks, so this is how rbit learns
//! what a push did), and with the [`PushSource`] the caller named.
//!
//! Recording never fails the push: the objects are already accepted, so problems reading
//! refs or updating `pushed_at` are logged and the event is still published.

use rbit_core::{PushSource, RepoId};
use rbit_git::{RefSnapshot, RepositoryReader};

use crate::{DomainEvent, RepoContext, ServiceContext};

#[derive(Debug, Clone)]
pub struct PushService {
    ctx: ServiceContext,
}

/// A push in progress: where it came from, where it goes, and the refs as they were before.
#[derive(Debug)]
pub struct PendingPush {
    repo: RepoId,
    source: PushSource,
    reader: RepositoryReader,
    before: Option<RefSnapshot>,
}

impl PushService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Snapshots the refs of `repo` before `git receive-pack` starts. `source` is who is
    /// accountable for the push; the event reports it unchanged.
    pub async fn begin(&self, repo: &RepoContext, source: PushSource) -> PendingPush {
        let before = match repo.reader.ref_snapshot().await {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                tracing::warn!(%error, repo = %repo.repo.full_name(), "cannot read refs before a push; its updates will not be reported");
                None
            }
        };
        PendingPush { repo: repo.repo.repo.id, source, reader: repo.reader.clone(), before }
    }

    /// Records a push that `git receive-pack` accepted and publishes
    /// [`DomainEvent::RepositoryPushed`]. Do not call it for failed pushes.
    pub async fn complete(&self, push: PendingPush) {
        if let Err(error) = self.ctx.database.repositories().mark_pushed(push.repo).await {
            tracing::warn!(%error, repo = push.repo.0, "failed to record push");
        }
        let updates = match (&push.before, push.reader.ref_snapshot().await) {
            (Some(before), Ok(after)) => before.updates_to(&after),
            (None, Ok(_)) => Vec::new(),
            (_, Err(error)) => {
                tracing::warn!(%error, repo = push.repo.0, "cannot read refs after a push; its updates will not be reported");
                Vec::new()
            }
        };
        self.ctx.events.publish(DomainEvent::RepositoryPushed { repo: push.repo, source: push.source, updates });
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, process::Command, sync::Arc};

    use rbit_core::{Config, PushSource, Visibility};
    use rbit_storage::{Database, KvStore};

    use crate::{CreateRepository, DomainEvent, Services, SignUp};

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    #[tokio::test]
    async fn completing_a_push_publishes_the_ref_updates_and_marks_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();

        let signup = SignUp { username: "ada".into(), email: "ada@example.com".into(), password: "correct horse battery".into() };
        let user = services.accounts.create_user(signup, None).await.unwrap();
        let input = CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&user, input).await.unwrap();
        let repo = services.repositories.open(Some(&user), "ada", "demo").await.unwrap();
        let mut events = services.events.subscribe();

        let pending = services.pushes.begin(&repo, PushSource::User(user.id())).await;
        let work = tempfile::tempdir().unwrap();
        git(work.path(), &["init", "-q"]);
        std::fs::write(work.path().join("README.md"), "# Demo\n").unwrap();
        git(work.path(), &["add", "."]);
        git(work.path(), &["commit", "-q", "-m", "Initial commit"]);
        git(work.path(), &["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
        services.pushes.complete(pending).await;

        let DomainEvent::RepositoryPushed { repo: pushed, source, updates } = events.recv().await.unwrap() else {
            panic!("expected a push event");
        };
        assert_eq!((pushed, source), (repo.repo.repo.id, PushSource::User(user.id())));
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].branch(), Some("main"));
        assert!(updates[0].is_created() && updates[0].after.as_ref().is_some_and(|id| id.len() == 40));

        let reopened = services.repositories.open(Some(&user), "ada", "demo").await.unwrap();
        assert!(reopened.repo.repo.pushed_at.is_some(), "pushed_at is recorded");
    }
}
