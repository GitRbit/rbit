//! Editing files in the browser: creating, editing, renaming, deleting and uploading files,
//! each as exactly one commit.
//!
//! Milestone 3 workstream E (editing files in the browser) owns this file.
//!
//! Rules, in the order they are checked:
//! * the actor needs write access ([`ServiceError::Forbidden`]); pull mirrors are read-only
//!   (they only change by fetching; [`MIRROR_READ_ONLY`]). rbit has no archived
//!   repositories yet; when it does, [`EditService::read_only_reason`] is where they are
//!   refused;
//! * a commit lands on the branch the editor was opened on, or on a new branch starting at
//!   the same commit ([`CommitTarget`]). Committing to the branch passes the push gate
//!   ([`crate::ProtectionService::check_push`], as a push updating it would): a refusal is
//!   [`EditFailure::Protected`], so the form can offer the new branch and a pull request
//!   instead. A new branch needs a valid, unused name and passes the gate as a created ref;
//! * paths are [`FilePath`]s; the tree checks (no clobbering, no paths through files) are
//!   `rbit_git::edit`'s. Text is limited to `[repository] max_render_bytes` and written in
//!   the file's existing line-ending style ([`rbit_core::file_edit::prepare_text`]); files
//!   that are binary, larger than that or Git LFS pointers cannot be edited (they can still be renamed by
//!   uploading, deleted or replaced by an upload). Uploads are bounded by `[editor]`;
//! * the commit's author and committer are the actor with their noreply address
//!   (`[pulls] noreply_domain`), as for merges;
//! * the commit is one `update-ref` compare-and-swap against the commit the editor was
//!   opened at: a branch that moved meanwhile is [`EditFailure::BranchMoved`], never
//!   rebased.
//!
//! The git work is wrapped in [`crate::PushService`], exactly like a merge, so
//! `RepositoryPushed` reports the ref update and everything downstream (webhooks, commit
//! references and `fixes #n`, caches, mergeability of pull requests) reacts as to a push.

use std::time::Duration;

use rbit_core::{
    PushSource, RefUpdate, Timestamp, User,
    file_edit::{self, EditAction, FilePath, LineEnding, prepare_text, uploaded_file_name},
    pull::{CommitIdentity, noreply_domain},
};
use rbit_git::{
    RefKind,
    edit::{BlobId, Change, EditError, EditSpec, EditWorkspace, is_valid_branch_name},
    merge::MergeEnvironment,
};
use tokio::io::AsyncRead;

use crate::{PushService, RepoContext, ServiceContext, ServiceError, protection::PushActor, releases::human_bytes};

/// Why a pull mirror cannot be edited.
pub const MIRROR_READ_ONLY: &str = "This repository is a mirror and cannot be edited; change its upstream instead";

#[derive(Debug, Clone)]
pub struct EditService {
    ctx: ServiceContext,
    protection: crate::ProtectionService,
    pushes: PushService,
}

/// Where a commit goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitTarget {
    /// The branch the editor was opened on.
    Branch,
    /// A new branch with this name, starting at the commit the editor was opened at.
    NewBranch(String),
}

/// The commit form, as submitted.
#[derive(Debug, Clone)]
pub struct CommitInput {
    /// The branch the editor was opened on.
    pub branch: String,
    /// The commit the editor was opened at (the form carries it).
    pub parent: String,
    pub target: CommitTarget,
    /// Empty: the default title for the change.
    pub title: String,
    pub description: String,
}

/// How the actor may commit to a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditAccess {
    /// Directly to the branch (or to a new branch).
    Direct,
    /// Only to a new branch; the reason committing directly is refused.
    NewBranchOnly(String),
}

/// A commit that landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub commit: String,
    pub branch: String,
    /// The commit created `branch` (a [`CommitTarget::NewBranch`]).
    pub new_branch: bool,
}

/// Why an edit was not committed.
#[derive(Debug, thiserror::Error)]
pub enum EditFailure {
    /// The branch moved since the editor was opened (`current`: where it is now, `None`
    /// when it is gone); nothing was committed.
    #[error("the branch changed since you started editing")]
    BranchMoved { current: Option<String> },
    /// Branch protection refuses committing to the branch directly (the reason).
    #[error("{0}")]
    Protected(String),
    /// The input cannot be committed; the message says why and is safe to show.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Service(#[from] ServiceError),
}

impl From<rbit_storage::StorageError> for EditFailure {
    fn from(error: rbit_storage::StorageError) -> Self {
        Self::Service(error.into())
    }
}

impl From<rbit_git::GitError> for EditFailure {
    fn from(error: rbit_git::GitError) -> Self {
        Self::Service(error.into())
    }
}

/// Files being uploaded: their blobs are already in the workspace, the commit is not made
/// yet ([`EditService::finish_upload`]).
#[derive(Debug)]
pub struct Upload {
    workspace: EditWorkspace,
    files: Vec<(String, BlobId)>,
    max_bytes: u64,
    max_files: usize,
}

impl Upload {
    /// Streams one uploaded file into the upload. `name` is the browser's file name (only
    /// its last component counts); an empty name (no file chosen) is ignored.
    pub async fn add(&mut self, name: &str, body: impl AsyncRead + Unpin) -> Result<(), EditFailure> {
        let name = uploaded_file_name(name);
        if name.is_empty() {
            return Ok(());
        }
        FilePath::parse(name).map_err(|error| EditFailure::Invalid(format!("{name}: {error}")))?;
        if self.files.iter().any(|(existing, _)| existing == name) {
            return Err(EditFailure::Invalid(format!("{name} was chosen twice")));
        }
        if self.files.len() >= self.max_files {
            return Err(EditFailure::Invalid(format!("at most {} files can be uploaded at once", self.max_files)));
        }
        let blob = match self.workspace.write_blob(body, self.max_bytes).await {
            Ok(blob) => blob,
            Err(EditError::TooLarge(limit)) => {
                return Err(EditFailure::Invalid(format!("{name} is too large: files can be at most {}", human_bytes(limit))));
            }
            Err(error) => return Err(ServiceError::internal(error).into()),
        };
        self.files.push((name.to_owned(), blob));
        Ok(())
    }

    /// The names of the files added so far.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(name, _)| name.as_str())
    }
}

impl EditService {
    pub(crate) fn new(ctx: ServiceContext, protection: crate::ProtectionService, pushes: PushService) -> Self {
        Self { ctx, protection, pushes }
    }

    fn environment(&self) -> MergeEnvironment {
        let config = &self.ctx.config;
        MergeEnvironment {
            git_binary: config.repository.git_binary.clone(),
            tmp_dir: config.storage.tmp_dir(),
            timeout: Duration::from_secs(config.pulls.merge_timeout_seconds.max(1)),
        }
    }

    /// Largest text a file may have to be edited (and new or edited text may have).
    pub fn max_text_bytes(&self) -> usize {
        self.ctx.config.repository.max_render_bytes
    }

    /// Why nobody may edit `repo` even with write access (a mirror), or `None`.
    pub async fn read_only_reason(&self, repo: &RepoContext) -> Result<Option<&'static str>, ServiceError> {
        Ok(self.ctx.database.mirrors().is_mirror(repo.repo.repo.id).await?.then_some(MIRROR_READ_ONLY))
    }

    /// Write access and a repository that is not read-only.
    async fn writable(&self, repo: &RepoContext) -> Result<(), EditFailure> {
        if !repo.access.can_write() {
            return Err(ServiceError::Forbidden.into());
        }
        match self.read_only_reason(repo).await? {
            Some(reason) => Err(EditFailure::Invalid(reason.to_owned())),
            None => Ok(()),
        }
    }

    /// Whether `actor` may edit `repo` at all (the buttons and forms are shown).
    pub async fn can_edit(&self, repo: &RepoContext) -> Result<bool, ServiceError> {
        Ok(repo.access.can_write() && self.read_only_reason(repo).await?.is_none())
    }

    /// How `actor` may commit to `branch` (an existing branch of `repo`).
    pub async fn access(&self, actor: &User, repo: &RepoContext, branch: &str) -> Result<EditAccess, EditFailure> {
        self.writable(repo).await?;
        Ok(match self.protection.branch_update_refusal(repo, actor, branch).await? {
            None => EditAccess::Direct,
            Some(reason) => EditAccess::NewBranchOnly(reason),
        })
    }

    /// The suggested name for a new branch: the first free `{username}-patch-{n}`.
    pub async fn suggested_branch(&self, actor: &User, repo: &RepoContext) -> Result<String, ServiceError> {
        let branches = self.branches(repo).await?;
        Ok(file_edit::patch_branch_name(actor.name().as_str(), |name| branches.iter().any(|branch| branch.0 == name)))
    }

    /// `(name, tip)` of every branch.
    async fn branches(&self, repo: &RepoContext) -> Result<Vec<(String, String)>, ServiceError> {
        let refs = repo.reader.refs().await?;
        Ok(refs.into_iter().filter(|r| r.kind == RefKind::Branch).map(|r| (r.name, r.target)).collect())
    }

    async fn branch_tip(&self, repo: &RepoContext, branch: &str) -> Result<Option<String>, ServiceError> {
        Ok(self.branches(repo).await?.into_iter().find(|(name, _)| name == branch).map(|(_, tip)| tip))
    }

    // ------------------------------------------------------------------ operations

    /// Creates the text file `path` with `text` (a textarea's value; written with LF).
    pub async fn create_file(
        &self,
        actor: &User,
        repo: &RepoContext,
        input: &CommitInput,
        path: &str,
        text: &str,
    ) -> Result<Edited, EditFailure> {
        let checked = self.check(actor, repo, input).await?;
        let path = parse_path(path)?;
        let content = self.text(text, LineEnding::Lf)?;
        let workspace = self.workspace(repo).await?;
        let blob = Self::blob(&workspace, &content).await?;
        let message = message(input, EditAction::Create(path.as_str()))?;
        self.commit(actor, repo, input, &checked, &workspace, &[Change::Create { path, blob }], &message).await
    }

    /// Replaces the text of file `path` with `text`, keeping its mode and line-ending
    /// style, and moves it to `new_path` when that differs.
    pub async fn update_file(
        &self,
        actor: &User,
        repo: &RepoContext,
        input: &CommitInput,
        path: &str,
        new_path: &str,
        text: &str,
    ) -> Result<Edited, EditFailure> {
        let checked = self.check(actor, repo, input).await?;
        let from = parse_path(path)?;
        let to = if new_path.trim().is_empty() { from.clone() } else { parse_path(new_path)? };
        let existing = self.editable_text(repo, &input.parent, from.as_str()).await?;
        let content = self.text(text, LineEnding::detect(&existing))?;
        let workspace = self.workspace(repo).await?;
        let (change, action) = if from == to {
            (Change::Update { path: from.clone(), blob: Self::blob(&workspace, &content).await? }, EditAction::Update(from.as_str()))
        } else {
            let blob = if content == existing { None } else { Some(Self::blob(&workspace, &content).await?) };
            (Change::Move { from: from.clone(), to: to.clone(), blob }, EditAction::Rename { from: from.as_str(), to: to.as_str() })
        };
        let message = message(input, action)?;
        self.commit(actor, repo, input, &checked, &workspace, &[change], &message).await
    }

    /// Deletes file `path`.
    pub async fn delete_file(&self, actor: &User, repo: &RepoContext, input: &CommitInput, path: &str) -> Result<Edited, EditFailure> {
        let checked = self.check(actor, repo, input).await?;
        let path = parse_path(path)?;
        let workspace = self.workspace(repo).await?;
        let message = message(input, EditAction::Delete(path.as_str()))?;
        self.commit(actor, repo, input, &checked, &workspace, &[Change::Delete { path }], &message).await
    }

    /// Starts an upload into `repo`: files are added with [`Upload::add`] as they arrive,
    /// then committed with [`Self::finish_upload`].
    pub async fn start_upload(&self, repo: &RepoContext) -> Result<Upload, EditFailure> {
        self.writable(repo).await?;
        let config = &self.ctx.config.editor;
        Ok(Upload {
            workspace: self.workspace(repo).await?,
            files: Vec::new(),
            max_bytes: config.max_upload_bytes,
            max_files: config.max_upload_files.max(1),
        })
    }

    /// Commits the uploaded files into `directory` (empty: the root), replacing files of
    /// the same name.
    pub async fn finish_upload(
        &self,
        actor: &User,
        repo: &RepoContext,
        input: &CommitInput,
        upload: Upload,
        directory: &str,
    ) -> Result<Edited, EditFailure> {
        let checked = self.check(actor, repo, input).await?;
        if upload.files.is_empty() {
            return Err(EditFailure::Invalid("choose at least one file to upload".to_owned()));
        }
        let mut changes = Vec::with_capacity(upload.files.len());
        for (name, blob) in &upload.files {
            let path = FilePath::join(directory, name).map_err(|error| EditFailure::Invalid(format!("{name}: {error}")))?;
            changes.push(Change::Put { path, blob: blob.clone() });
        }
        let message = message(input, EditAction::Upload)?;
        self.commit(actor, repo, input, &checked, &upload.workspace, &changes, &message).await
    }

    /// The content of file `path` at `parent` if the browser may edit it: text no larger
    /// than [`Self::max_text_bytes`] that is not a Git LFS pointer.
    pub async fn editable_text(&self, repo: &RepoContext, parent: &str, path: &str) -> Result<Vec<u8>, EditFailure> {
        let view = match repo.reader.blob(parent.to_owned(), path.to_owned(), self.max_text_bytes()).await {
            Ok(view) => view,
            Err(rbit_git::GitError::NotFound(_)) => return Err(EditFailure::Invalid(format!("there is no file called {path}"))),
            Err(error) => return Err(error.into()),
        };
        if view.text().is_none() {
            return Err(EditFailure::Invalid(format!("{path} is binary or too large to edit in the browser")));
        }
        let data = view.data.unwrap_or_default();
        // Editing the pointer would replace the object with its own description.
        if rbit_core::lfs::LfsPointer::parse(&data).is_some() {
            return Err(EditFailure::Invalid(format!("{path} is stored with Git LFS and cannot be edited in the browser")));
        }
        Ok(data)
    }

    // ------------------------------------------------------------------ committing

    /// Everything decided before any object is written: access, the branch, the target and
    /// the push gate.
    async fn check(&self, actor: &User, repo: &RepoContext, input: &CommitInput) -> Result<Checked, EditFailure> {
        self.writable(repo).await?;
        let Some(tip) = self.branch_tip(repo, &input.branch).await? else {
            return Err(ServiceError::NotFound.into());
        };
        let update = match &input.target {
            CommitTarget::Branch => {
                RefUpdate { name: format!("refs/heads/{}", input.branch), before: Some(tip.clone()), after: Some(tip.clone()) }
            }
            CommitTarget::NewBranch(name) => {
                if name.is_empty() {
                    return Err(EditFailure::Invalid("enter a name for the new branch".to_owned()));
                }
                if !is_valid_branch_name(name) {
                    return Err(EditFailure::Invalid(format!("{name} is not a valid branch name")));
                }
                if self.branch_tip(repo, name).await?.is_some() {
                    return Err(EditFailure::Invalid(format!("a branch called {name} already exists")));
                }
                RefUpdate { name: format!("refs/heads/{name}"), before: None, after: Some(tip.clone()) }
            }
        };
        if input.target == CommitTarget::Branch && input.parent != tip {
            // Refused before any work; the lease would refuse it anyway.
            return Err(EditFailure::BranchMoved { current: Some(tip) });
        }
        let actor_for_gate = PushActor { user: Some(actor), deploy_key: false, access: repo.access };
        let verdict = self.protection.check_push(repo, &actor_for_gate, &[update]).await?;
        if let Some(refusal) = verdict.refusals.into_iter().next() {
            return Err(match input.target {
                CommitTarget::Branch => EditFailure::Protected(refusal.reason),
                CommitTarget::NewBranch(_) => EditFailure::Invalid(refusal.reason),
            });
        }
        Ok(Checked { identity: self.identity(actor) })
    }

    /// The actor as a commit identity: always their noreply address, like merges (rbit never
    /// writes a real email address into a commit it makes).
    fn identity(&self, actor: &User) -> CommitIdentity {
        let config = &self.ctx.config;
        let domain = noreply_domain(&config.pulls.noreply_domain, &config.server.public_url);
        CommitIdentity::noreply(actor.name().as_str(), actor.account.display_name.as_deref(), &domain)
    }

    fn text(&self, text: &str, style: LineEnding) -> Result<Vec<u8>, EditFailure> {
        let content = prepare_text(text, style);
        if content.len() > self.max_text_bytes() {
            let limit = human_bytes(u64::try_from(self.max_text_bytes()).unwrap_or(u64::MAX));
            return Err(EditFailure::Invalid(format!("files edited in the browser can be at most {limit}")));
        }
        Ok(content)
    }

    async fn workspace(&self, repo: &RepoContext) -> Result<EditWorkspace, EditFailure> {
        let lease = self.ctx.repositories.lease(repo.repo.repo.id).await;
        let workspace = EditWorkspace::open(&self.environment(), &repo.disk_path).await.map_err(ServiceError::internal)?;
        Ok(workspace.holding(lease))
    }

    async fn blob(workspace: &EditWorkspace, content: &[u8]) -> Result<BlobId, EditFailure> {
        workspace.write_blob(content, u64::MAX).await.map_err(|error| ServiceError::internal(error).into())
    }

    /// Makes the commit and reports it like a push.
    #[expect(clippy::too_many_arguments, reason = "one commit's inputs, all needed together")]
    async fn commit(
        &self,
        actor: &User,
        repo: &RepoContext,
        input: &CommitInput,
        checked: &Checked,
        workspace: &EditWorkspace,
        changes: &[Change],
        message: &str,
    ) -> Result<Edited, EditFailure> {
        let new_branch = match &input.target {
            CommitTarget::Branch => None,
            CommitTarget::NewBranch(name) => Some(name.as_str()),
        };
        let spec = EditSpec {
            branch: &input.branch,
            parent: &input.parent,
            new_branch,
            changes,
            message,
            author: &checked.identity,
            at: Timestamp::now(),
        };
        let pending = self.pushes.begin(repo, PushSource::User(actor.id())).await;
        let committed = match workspace.commit(&spec).await {
            Ok(committed) => committed,
            Err(EditError::Rejected(message)) => return Err(EditFailure::Invalid(message)),
            Err(EditError::NothingChanged) => return Err(EditFailure::Invalid("there are no changes to commit".to_owned())),
            Err(error @ EditError::BranchExists(_)) => return Err(EditFailure::Invalid(error.to_string())),
            Err(EditError::BranchMoved) => {
                return Err(EditFailure::BranchMoved { current: self.branch_tip(repo, &input.branch).await? });
            }
            Err(error) => return Err(ServiceError::internal(error).into()),
        };
        self.pushes.complete(pending).await;
        tracing::info!(actor = %actor.name(), repo = %repo.repo.full_name(), branch = %committed.branch, commit = %committed.commit, "files edited in the browser");
        Ok(Edited { commit: committed.commit, branch: committed.branch, new_branch: new_branch.is_some() })
    }
}

/// What [`EditService::check`] established.
#[derive(Debug)]
struct Checked {
    identity: CommitIdentity,
}

fn parse_path(path: &str) -> Result<FilePath, EditFailure> {
    FilePath::parse(path).map_err(|error| EditFailure::Invalid(error.to_string()))
}

fn message(input: &CommitInput, action: EditAction<'_>) -> Result<String, EditFailure> {
    file_edit::commit_message(&input.title, &input.description, action).map_err(EditFailure::Invalid)
}
