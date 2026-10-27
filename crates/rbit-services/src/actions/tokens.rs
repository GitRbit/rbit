//! Task tokens (`rbat_…`) as credentials for Git over HTTP, LFS and the API (§4.8, §11.3,
//! §11.4, §11.11).
//!
//! * [`ActionsService::authenticate_task`] turns a presented token into a [`TaskGrant`]:
//!   one digest and one lookup of a *running* task with that digest (tokens of stopped and
//!   cancelled tasks are cleared, so they find nothing), re-checked on every use against
//!   the repository's existence and its Actions switch. What the grant may do is decided
//!   then, from the repository's default token permission, the job's `permissions:` and the
//!   run's trust (`rbit_core::actions::token_permissions`): untrusted runs read only.
//! * [`ActionsService::open_for_task`] opens a repository for a grant: its own at the
//!   granted `contents` level, any other only when public and only to read.
//! * Pushes made with a task token never start runs (`PushSource::Task`), get the
//!   deploy-key treatment under branch protection (never an allowed pusher, never a
//!   bypass), and are refused when they move or delete an existing tag or bring new
//!   commits that change workflow files ([`ActionsService::task_push_refusals`]). The
//!   second needs the pushed objects before git stores them, so a task's push is spooled
//!   to disk ([`ActionsService::spool_task_push`]), its pack indexed into a quarantine that
//!   borrows the repository's objects, and the new commits inspected there.
//!
//! With Actions disabled there is no service, so callers refuse task tokens without any
//! lookup.
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::{
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    task::{Context, Poll},
};

use rbit_auth::TaskTokenSecret;
use rbit_core::{
    RefUpdate, RepoAccess, RepoId,
    actions::{JobId, PermissionLevel, RunId, RunTrust, TaskId, TokenPermissions, token_permissions},
    protection::Refusal,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};

use super::ActionsService;
use crate::{RepoContext, ServiceError};

/// Where workflows live, in the order rbit looks (§12.1): the directories discovery reads,
/// so the two never drift. A task may not change them.
pub const WORKFLOW_DIRECTORIES: [&str; 3] = super::triggers::WORKFLOW_DIRS;
/// The largest push a task may make, spooled to disk before git sees it.
pub const MAX_TASK_PUSH_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// What a running task's token grants, decided when it is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskGrant {
    pub task: TaskId,
    pub job: JobId,
    pub run: RunId,
    /// The task's own repository: the only one it may write to.
    pub repo: RepoId,
    pub permissions: TokenPermissions,
}

impl TaskGrant {
    /// Access to its own repository's contents (Git, LFS, releases).
    pub fn contents(&self) -> RepoAccess {
        match self.permissions.contents {
            PermissionLevel::None => RepoAccess::None,
            PermissionLevel::Read => RepoAccess::Read,
            PermissionLevel::Write => RepoAccess::Write,
        }
    }

    /// Whether it may comment on some of its own repository's threads: an endpoint that
    /// comments asks no less of a task's token.
    pub fn can_comment(&self) -> bool {
        self.permissions.issues == PermissionLevel::Write || self.permissions.pull_requests == PermissionLevel::Write
    }

    // Actions: task tokens.
    /// Whether it may comment on a thread of `kind`: an issue needs `issues: write`, a pull
    /// request `pull-requests: write` or `issues: write` (its conversation is an issue's).
    pub fn can_comment_on(&self, kind: rbit_core::IssueKind) -> bool {
        match kind {
            rbit_core::IssueKind::Issue => self.permissions.issues == PermissionLevel::Write,
            rbit_core::IssueKind::Pull => self.can_comment(),
        }
    }
}

impl ActionsService {
    /// The grant of a presented task token, or `None` when it is malformed, unknown, its
    /// task stopped, its repository is gone or has Actions switched off.
    pub async fn authenticate_task(&self, secret: &str) -> Result<Option<TaskGrant>, ServiceError> {
        let Some(token) = TaskTokenSecret::parse(secret) else { return Ok(None) };
        let database = &self.ctx().database;
        let Some((task, job)) = database.actions_runs().running_task_by_token(&token.digest()).await? else { return Ok(None) };
        let Some(settings) = database.actions_settings().get(job.repo_id).await?.filter(|settings| settings.enabled) else {
            return Ok(None);
        };
        if database.admin().find_repository(job.repo_id).await?.is_none() {
            return Ok(None);
        }
        let Some(run) = database.actions_runs().run(job.run_id).await? else { return Ok(None) };
        let declared = job.permissions.as_deref().and_then(|json| serde_json::from_str::<TokenPermissions>(json).ok());
        let trust = if run.trusted { RunTrust::Trusted } else { RunTrust::Untrusted { awaits_approval: false } };
        let permissions = token_permissions(settings.token_permission, None, declared, trust);
        Ok(Some(TaskGrant { task: task.id, job: job.id, run: run.id, repo: job.repo_id, permissions }))
    }

    /// Opens `owner/name` (an old name too) for a task: its own repository at the granted
    /// `contents` level, any other public one read-only; everything else is `NotFound`.
    pub async fn open_for_task(&self, grant: &TaskGrant, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        self.open_as_task(grant, owner, name, grant.contents()).await
    }

    // Actions: task tokens.
    /// [`Self::open_for_task`] for commenting: a task that may comment (`issues` or
    /// `pull-requests: write`) reads its own repository's threads whatever its `contents`
    /// permission, as `permissions: {pull-requests: write}` alone allows on GitHub.
    pub async fn open_for_task_comment(&self, grant: &TaskGrant, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        let own = if grant.can_comment() { grant.contents().max(RepoAccess::Read) } else { grant.contents() };
        self.open_as_task(grant, owner, name, own).await
    }

    /// Opens `owner/name` with `own` access when it is the task's repository.
    async fn open_as_task(&self, grant: &TaskGrant, owner: &str, name: &str, own: RepoAccess) -> Result<RepoContext, ServiceError> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        let store = self.ctx().database.repositories();
        let repo = match store.find_by_path(owner, name).await? {
            Some(repo) => repo,
            None => store.find_by_old_path(owner, name).await?.ok_or(ServiceError::NotFound)?,
        };
        let public = if repo.repo.visibility.is_public() { RepoAccess::Read } else { RepoAccess::None };
        let access = if repo.repo.id == grant.repo { own.max(public) } else { public };
        if !access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let mut ctx = RepoContext::for_system(repo, &self.ctx().repositories);
        ctx.access = access;
        Ok(ctx)
    }

    /// Writes a task's push request (`body`, whose command section is the first
    /// `pack_offset` bytes) to disk, so it can be inspected before git stores anything and
    /// then replayed. At most [`MAX_TASK_PUSH_BYTES`].
    pub async fn spool_task_push(&self, mut body: impl AsyncRead + Unpin, pack_offset: u64) -> Result<SpooledPush, ServiceError> {
        let dir = self.ctx().config.storage.tmp_dir().join("actions-pushes").join(random_name()?);
        tokio::fs::create_dir_all(dir.join("objects").join("pack")).await.map_err(ServiceError::internal)?;
        let push = SpooledPush { request: dir.join("request"), dir, pack_offset };
        let mut file = tokio::fs::File::create(&push.request).await.map_err(ServiceError::internal)?;
        let copied = tokio::io::copy(&mut (&mut body).take(MAX_TASK_PUSH_BYTES + 1), &mut file).await.map_err(ServiceError::internal)?;
        if copied > MAX_TASK_PUSH_BYTES {
            return Err(ServiceError::Invalid(format!("a workflow task may push at most {MAX_TASK_PUSH_BYTES} bytes at once")));
        }
        file.flush().await.map_err(ServiceError::internal)?;
        Ok(push)
    }

    /// Why a task's push must be refused, per ref: it moves or deletes an existing tag, or
    /// its new commits change a workflow file (§11.4). `pack` is false when the push only
    /// deletes. Empty when it may go ahead.
    pub async fn task_push_refusals(
        &self,
        repo: &RepoContext,
        updates: &[RefUpdate],
        push: &SpooledPush,
        pack: bool,
    ) -> Result<Vec<Refusal>, ServiceError> {
        let mut refusals = Vec::new();
        for update in updates.iter().filter(|update| update.name.starts_with("refs/tags/") && update.before.is_some()) {
            refusals
                .push(Refusal { name: update.name.clone(), reason: "a workflow task may not move or delete an existing tag".to_owned() });
        }
        if !pack {
            return Ok(refusals);
        }
        let objects = push.dir.join("objects");
        let mut index = self.git(repo, &objects);
        index.args(["index-pack", "--stdin", "--fix-thin"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = index.spawn().map_err(ServiceError::internal)?;
        let mut stdin = child.stdin.take().ok_or_else(|| ServiceError::Invalid("git index-pack has no input".to_owned()))?;
        let mut request = tokio::fs::File::open(&push.request).await.map_err(ServiceError::internal)?;
        tokio::io::AsyncSeekExt::seek(&mut request, std::io::SeekFrom::Start(push.pack_offset)).await.map_err(ServiceError::internal)?;
        // A broken pipe means git stopped reading; its exit status says why.
        let _ = tokio::io::copy(&mut request, &mut stdin).await;
        drop(stdin);
        let indexed = child.wait_with_output().await.map_err(ServiceError::internal)?;
        if !indexed.status.success() {
            tracing::info!(repo = %repo.repo.full_name(), stderr = %String::from_utf8_lossy(&indexed.stderr), "task push: the pack does not index");
            return Ok(updates
                .iter()
                .map(|update| Refusal { name: update.name.clone(), reason: "the pushed pack could not be read".to_owned() })
                .collect());
        }
        for update in updates {
            let Some(after) = &update.after else { continue };
            if !is_object_id(after) {
                continue;
            }
            let mut log = self.git(repo, &objects);
            log.args(["log", "--format=%H", "--name-only", "--diff-merges=dense-combined", after, "--not", "--all", "--"]);
            log.args(WORKFLOW_DIRECTORIES).stdin(Stdio::null()).stderr(Stdio::piped());
            let output = log.output().await.map_err(ServiceError::internal)?;
            if !output.status.success() {
                return Err(ServiceError::Invalid(format!("the pushed commits of {} could not be read", update.name)));
            }
            if !String::from_utf8_lossy(&output.stdout).trim().is_empty() {
                refusals.push(Refusal {
                    name: update.name.clone(),
                    reason: "a workflow task may not create, change or delete workflow files".to_owned(),
                });
            }
        }
        Ok(refusals)
    }

    /// `git --git-dir <repo>` with the pushed objects of `quarantine` visible beside the
    /// repository's own, a scrubbed environment and no user configuration.
    fn git(&self, repo: &RepoContext, quarantine: &Path) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.ctx().config.repository.git_binary);
        command.env_clear();
        for key in ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OBJECT_DIRECTORY", quarantine)
            .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", repo.disk_path.join("objects"))
            .arg("--git-dir")
            .arg(&repo.disk_path)
            .kill_on_drop(true);
        command
    }
}

/// A task's push request on disk, removed when dropped.
#[derive(Debug)]
pub struct SpooledPush {
    dir: PathBuf,
    request: PathBuf,
    pack_offset: u64,
}

impl SpooledPush {
    /// The whole request again, from its first byte, for git; the spool goes when the
    /// reader is dropped.
    pub async fn replay(self) -> Result<SpooledReader, ServiceError> {
        let file = tokio::fs::File::open(&self.request).await.map_err(ServiceError::internal)?;
        Ok(SpooledReader { file, _push: self })
    }
}

impl Drop for SpooledPush {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.dir)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, dir = %self.dir.display(), "could not remove a spooled task push");
        }
    }
}

/// Reads a spooled push; the file closes before the spool is removed.
#[derive(Debug)]
pub struct SpooledReader {
    file: tokio::fs::File,
    _push: SpooledPush,
}

impl AsyncRead for SpooledReader {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_read(cx, buf)
    }
}

fn is_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(super) fn random_name() -> Result<String, ServiceError> {
    let mut bytes = [0u8; 12];
    getrandom::fill(&mut bytes).map_err(|error| ServiceError::internal(std::io::Error::other(error.to_string())))?;
    Ok(bytes.iter().fold(String::with_capacity(24), |mut name, byte| {
        use std::fmt::Write as _;
        let _ = write!(name, "{byte:02x}");
        name
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(contents: PermissionLevel, issues: PermissionLevel) -> TaskGrant {
        TaskGrant {
            task: TaskId(1),
            job: JobId(1),
            run: RunId(1),
            repo: RepoId(1),
            permissions: TokenPermissions { contents, issues, pull_requests: PermissionLevel::None },
        }
    }

    #[test]
    fn grants_map_permissions_to_access() {
        assert_eq!(grant(PermissionLevel::Write, PermissionLevel::None).contents(), RepoAccess::Write);
        assert_eq!(grant(PermissionLevel::Read, PermissionLevel::None).contents(), RepoAccess::Read);
        assert_eq!(grant(PermissionLevel::None, PermissionLevel::None).contents(), RepoAccess::None);
        assert!(grant(PermissionLevel::Read, PermissionLevel::Write).can_comment());
        assert!(!grant(PermissionLevel::Write, PermissionLevel::Read).can_comment());
        assert!(is_object_id(&"a".repeat(40)) && !is_object_id("HEAD"));
        // Actions: task tokens. Issues need `issues: write`; pull requests either.
        let pulls_only = TaskGrant {
            permissions: TokenPermissions {
                pull_requests: PermissionLevel::Write,
                ..grant(PermissionLevel::Read, PermissionLevel::Read).permissions
            },
            ..grant(PermissionLevel::Read, PermissionLevel::Read)
        };
        assert!(pulls_only.can_comment_on(rbit_core::IssueKind::Pull) && !pulls_only.can_comment_on(rbit_core::IssueKind::Issue));
        let issues = grant(PermissionLevel::Read, PermissionLevel::Write);
        assert!(issues.can_comment_on(rbit_core::IssueKind::Issue) && issues.can_comment_on(rbit_core::IssueKind::Pull));
    }
}
