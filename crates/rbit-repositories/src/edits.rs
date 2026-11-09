//! Browser file edits: who may commit, where, and what is refused before any object is
//! written.
//!
//! Rules, in the order they are checked:
//! * the actor needs write access (forbidden otherwise); archived repositories and pull
//!   mirrors are read-only (archived is decided first, without a query);
//! * the editor's branch must exist (not found otherwise);
//! * a new branch needs a non-empty, valid and unused name;
//! * committing to the editor's branch requires it not to have moved since the editor
//!   was opened, refused before any work;
//! * the push gate sees exactly one ref update: the branch kept at its tip, or the new
//!   branch created at it. A refusal for the editor's branch is a protection refusal (the
//!   form can offer a new branch); for a new branch it is an ordinary validation error.
//!
//! Path parsing, text limits, blob writing and the compare-and-swap commit stay with
//! the caller's Git workspace; the commit itself still refuses a moved branch.

use std::future::Future;

use rbit_identity_models::User;

use crate::Repository;

/// Why a pull mirror cannot be edited.
pub const MIRROR_READ_ONLY: &str = "This repository is a mirror and cannot be edited; change its upstream instead";

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

/// The one ref update an edit asks the push gate about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchUpdate {
    /// Full ref name, `refs/heads/...`.
    pub name: String,
    /// `None` creates the branch.
    pub before: Option<String>,
    pub after: String,
}

/// Why an edit is refused before committing; port failures keep their identity.
#[derive(Debug)]
pub enum EditRefusal<E> {
    Forbidden,
    NotFound,
    /// `current`: where the branch is now.
    BranchMoved {
        current: Option<String>,
    },
    Protected(String),
    Invalid(String),
    Port(E),
}

/// Repository facts and the push gate, as the caller already computes them.
pub trait EditPort: Sync {
    type Error: Send;
    fn is_mirror(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn branch_tip(&self, branch: &str) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    /// Git's branch-name rules.
    fn valid_branch_name(&self, name: &str) -> bool;
    /// The first refusal reason of the push gate for this update, if any.
    fn push_refusal(&self, actor: &User, update: BranchUpdate) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    /// Why committing directly to `branch` is refused, if it is.
    fn branch_update_refusal(&self, actor: &User, branch: &str) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
}

/// Edit decisions for one loaded repository.
pub struct RepositoryEdits<'a, P> {
    port: &'a P,
    repo: &'a Repository,
    can_write: bool,
}

impl<P> std::fmt::Debug for RepositoryEdits<'_, P> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("RepositoryEdits").field("repo", &self.repo.id).field("can_write", &self.can_write).finish_non_exhaustive()
    }
}

impl<'a, P: EditPort> RepositoryEdits<'a, P> {
    /// `can_write` is the loaded request's existing write authority.
    pub fn new(port: &'a P, repo: &'a Repository, can_write: bool) -> Self {
        Self { port, repo, can_write }
    }

    /// Why nobody may edit the repository even with write access, or `None`.
    pub async fn read_only_reason(&self) -> Result<Option<&'static str>, P::Error> {
        if self.repo.is_archived() {
            return Ok(Some(crate::ARCHIVED_READ_ONLY));
        }
        Ok(self.port.is_mirror().await?.then_some(MIRROR_READ_ONLY))
    }

    /// Whether the actor may edit at all (the buttons and forms are shown).
    pub async fn can_edit(&self) -> Result<bool, P::Error> {
        Ok(self.can_write && self.read_only_reason().await?.is_none())
    }

    /// Write access and a repository that is not read-only.
    pub async fn writable(&self) -> Result<(), EditRefusal<P::Error>> {
        if !self.can_write {
            return Err(EditRefusal::Forbidden);
        }
        match self.read_only_reason().await.map_err(EditRefusal::Port)? {
            Some(reason) => Err(EditRefusal::Invalid(reason.to_owned())),
            None => Ok(()),
        }
    }

    /// How the actor may commit to `branch`.
    pub async fn access(&self, actor: &User, branch: &str) -> Result<EditAccess, EditRefusal<P::Error>> {
        self.writable().await?;
        Ok(match self.port.branch_update_refusal(actor, branch).await.map_err(EditRefusal::Port)? {
            None => EditAccess::Direct,
            Some(reason) => EditAccess::NewBranchOnly(reason),
        })
    }

    /// Everything decided before any object is written: access, the branch, the target
    /// and the push gate.
    pub async fn check(&self, actor: &User, input: &CommitInput) -> Result<(), EditRefusal<P::Error>> {
        self.writable().await?;
        let Some(tip) = self.port.branch_tip(&input.branch).await.map_err(EditRefusal::Port)? else {
            return Err(EditRefusal::NotFound);
        };
        let update = match &input.target {
            CommitTarget::Branch => {
                BranchUpdate { name: format!("refs/heads/{}", input.branch), before: Some(tip.clone()), after: tip.clone() }
            }
            CommitTarget::NewBranch(name) => {
                if name.is_empty() {
                    return Err(EditRefusal::Invalid("enter a name for the new branch".to_owned()));
                }
                if !self.port.valid_branch_name(name) {
                    return Err(EditRefusal::Invalid(format!("{name} is not a valid branch name")));
                }
                if self.port.branch_tip(name).await.map_err(EditRefusal::Port)?.is_some() {
                    return Err(EditRefusal::Invalid(format!("a branch called {name} already exists")));
                }
                BranchUpdate { name: format!("refs/heads/{name}"), before: None, after: tip.clone() }
            }
        };
        if input.target == CommitTarget::Branch && input.parent != tip {
            // Refused before any work; the commit's lease would refuse it anyway.
            return Err(EditRefusal::BranchMoved { current: Some(tip) });
        }
        if let Some(reason) = self.port.push_refusal(actor, update).await.map_err(EditRefusal::Port)? {
            return Err(match input.target {
                CommitTarget::Branch => EditRefusal::Protected(reason),
                CommitTarget::NewBranch(_) => EditRefusal::Invalid(reason),
            });
        }
        Ok(())
    }
}

/// Admits one more uploaded file called `name` (already a valid path component).
pub fn admit_upload<'n>(mut chosen: impl Iterator<Item = &'n str>, name: &str, count: usize, max_files: usize) -> Result<(), String> {
    if chosen.any(|existing| existing == name) {
        return Err(format!("{name} was chosen twice"));
    }
    if count >= max_files {
        return Err(format!("at most {max_files} files can be uploaded at once"));
    }
    Ok(())
}

/// An upload commit needs at least one file.
pub fn require_upload(count: usize) -> Result<(), String> {
    if count == 0 { Err("choose at least one file to upload".to_owned()) } else { Ok(()) }
}
