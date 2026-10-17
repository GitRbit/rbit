//! Committing file changes made in the browser (create, edit, rename, delete, upload) without
//! a working tree; the commit reaches the repository only through one atomic,
//! lease-protected ref update.
//!
//! Milestone 3 workstream E (editing files in the browser) owns this file.
//!
//! # How an edit is committed
//!
//! The approach of [`crate::merge`], with its helpers:
//!
//! 1. An [`EditWorkspace`] is a scratch repository under `<tmp_dir>/merges/<random>` whose
//!    only object source is the repository (an [`Overlay`]). New file contents are written
//!    into it as blobs with `git hash-object -w --no-filters --stdin`, streamed and bounded
//!    ([`EditWorkspace::write_blob`]): uploads never sit in memory, and no
//!    `.gitattributes` filter or line-ending conversion touches them.
//! 2. [`EditWorkspace::commit`] checks every [`Change`] against the tree of the commit the
//!    editor started from (read with gitoxide): a new file may not clobber anything, an
//!    edited, moved or deleted file must exist and be a file, no path may run through a
//!    file, and no two changes may write the same path. Refusals are
//!    [`EditError::Rejected`] with a message for the user. Edited and moved files keep their
//!    mode (executable, symlink).
//! 3. The new tree is built with plumbing on a scratch index file inside the workspace
//!    (`GIT_INDEX_FILE`): `read-tree <parent>`, one `update-index -z --index-info` with
//!    every removal and write, `write-tree`. Directories left empty disappear by
//!    themselves. A tree equal to the parent's is [`EditError::NothingChanged`].
//! 4. `git commit-tree` makes one commit with the parent as its only parent; author and
//!    committer are the given identity at the given time.
//! 5. The repository fetches the commit and moves the branch with `update-ref <new>
//!    <parent>` ([`crate::merge`]'s hand-over): a branch that moved since the editor
//!    opened is [`EditError::BranchMoved`], never silently rebased. A commit to a new
//!    branch expects the branch not to exist ([`EditError::BranchExists`]).
//!
//! Every `git` is [`crate::merge::hardened`] (no hooks, no system or global
//! configuration, no `autocrlf`); revisions are validated full ids or validated branch
//! names; paths are [`FilePath`]s and travel NUL-terminated on standard input, never as
//! arguments. Committing is bounded by the environment's timeout; writing blobs is
//! bounded by size instead (an upload's duration is the web layer's business). The
//! workspace directory is removed when the workspace is dropped.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use rbit_core::{Timestamp, file_edit::FilePath, pull::CommitIdentity};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::{
    GitError, RepositoryLease,
    merge::{Git, MergeEnvironment, MergeError, Signature, expect_success, failure, hand_over},
    overlay::{MERGES_DIR, Overlay},
    pull_refs::{branch_ref, validate_id},
};

/// The scratch index inside a workspace.
const INDEX_FILE: &str = "rbit-edit-index";
/// Bytes copied into `git hash-object` at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// A blob written into an [`EditWorkspace`] (a validated full object id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobId(String);

impl BlobId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether `name` is a valid branch name (what `refs/heads/<name>` allows, and not an option).
pub fn is_valid_branch_name(name: &str) -> bool {
    branch_ref(name).is_ok()
}

/// One change of a web edit.
#[derive(Debug, Clone)]
pub enum Change {
    /// A new file; nothing may exist at `path`.
    Create { path: FilePath, blob: BlobId },
    /// New content for the existing file `path` (its mode is kept).
    Update { path: FilePath, blob: BlobId },
    /// Moves the existing file `from` to `to` (nothing may exist there), keeping its mode,
    /// with new content when `blob` is given.
    Move { from: FilePath, to: FilePath, blob: Option<BlobId> },
    /// Removes the existing file `path`.
    Delete { path: FilePath },
    /// Writes `path` whether or not a file is there (uploads replace files of the same
    /// name); it may not be a directory.
    Put { path: FilePath, blob: BlobId },
}

/// One commit to make.
#[derive(Debug, Clone)]
pub struct EditSpec<'a> {
    /// The branch the editor was opened on.
    pub branch: &'a str,
    /// The commit the editor started from: the new commit's parent and, unless
    /// `new_branch` is set, what the lease expects `branch` to point at.
    pub parent: &'a str,
    /// Commit to this new branch (starting at `parent`) instead of `branch`.
    pub new_branch: Option<&'a str>,
    pub changes: &'a [Change],
    /// The full commit message.
    pub message: &'a str,
    /// Author and committer.
    pub author: &'a CommitIdentity,
    pub at: Timestamp,
}

/// A commit that landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub commit: String,
    /// The branch it landed on.
    pub branch: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// The changes do not fit the tree; the message says why and is safe to show.
    #[error("{0}")]
    Rejected(String),
    /// The result is the parent's tree.
    #[error("there are no changes to commit")]
    NothingChanged,
    /// The branch no longer points at the parent; nothing was changed.
    #[error("the branch changed since editing started")]
    BranchMoved,
    /// The new branch exists already; nothing was changed.
    #[error("a branch called {0} already exists")]
    BranchExists(String),
    /// A blob was larger than the limit (in bytes) it was written with.
    #[error("files can be at most {0} bytes")]
    TooLarge(u64),
    #[error("committing took longer than {0:?}")]
    TimedOut(Duration),
    #[error("git failed: {0}")]
    Failed(String),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl From<std::io::Error> for EditError {
    fn from(error: std::io::Error) -> Self {
        Self::Git(GitError::Io(error))
    }
}

impl From<MergeError> for EditError {
    fn from(error: MergeError) -> Self {
        match error {
            MergeError::BaseMoved => Self::BranchMoved,
            MergeError::TimedOut(limit) => Self::TimedOut(limit),
            MergeError::Failed(message) => Self::Failed(message),
            MergeError::Git(error) => Self::Git(error),
            other => Self::Failed(other.to_string()),
        }
    }
}

/// A scratch repository reading the objects of one repository, to write blobs into and
/// commit from (see the module docs). Removed when dropped.
#[derive(Debug)]
pub struct EditWorkspace {
    scratch: Overlay,
    binary: PathBuf,
    repo: PathBuf,
    timeout: Duration,
}

impl EditWorkspace {
    /// A workspace for the bare repository at `repo`.
    pub async fn open(env: &MergeEnvironment, repo: &Path) -> Result<Self, EditError> {
        let scratch = Overlay::create_in(&env.tmp_dir.join(MERGES_DIR), &[repo]).await?;
        Ok(Self { scratch, binary: env.git_binary.clone(), repo: repo.to_owned(), timeout: env.timeout })
    }

    /// Keeps `lease` (on the repository) until the workspace is dropped, so the repository
    /// is not moved while the workspace reads its objects.
    #[must_use]
    pub fn holding(mut self, lease: RepositoryLease) -> Self {
        self.scratch = self.scratch.holding(lease);
        self
    }

    fn git(&self) -> Git<'_> {
        Git { binary: &self.binary, dir: self.scratch.path() }
    }

    /// Writes `content` as a blob, streaming; more than `max_bytes` is
    /// [`EditError::TooLarge`] (and nothing is written).
    pub async fn write_blob(&self, mut content: impl AsyncRead + Unpin, max_bytes: u64) -> Result<BlobId, EditError> {
        let mut command = self.git().command();
        command.args(["hash-object", "-w", "--no-filters", "--stdin"]).stdin(Stdio::piped());
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().ok_or_else(|| EditError::Failed("hash-object has no standard input".to_owned()))?;
        let mut buffer = vec![0_u8; CHUNK_BYTES];
        let mut total: u64 = 0;
        loop {
            let read = content.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > max_bytes {
                // Dropping the child kills it; the half-written object never appears.
                return Err(EditError::TooLarge(max_bytes));
            }
            stdin.write_all(&buffer[..read]).await?;
        }
        stdin.shutdown().await?;
        drop(stdin);
        let output = expect_success(child.wait_with_output().await?, "hash-object")?;
        let id = String::from_utf8_lossy(&output).trim().to_owned();
        validate_id(&id)?;
        Ok(BlobId(id))
    }

    /// Commits `spec` (see the module docs).
    pub async fn commit(&self, spec: &EditSpec<'_>) -> Result<Committed, EditError> {
        tokio::time::timeout(self.timeout, self.commit_inner(spec)).await.map_err(|_| EditError::TimedOut(self.timeout))?
    }

    async fn commit_inner(&self, spec: &EditSpec<'_>) -> Result<Committed, EditError> {
        validate_id(spec.parent)?;
        branch_ref(spec.branch)?;
        let branch = spec.new_branch.unwrap_or(spec.branch);
        let target = branch_ref(branch)?;
        let expected = match spec.new_branch {
            Some(_) => "0".repeat(spec.parent.len()),
            None => spec.parent.to_owned(),
        };

        let entries = self.lookup(spec.parent, spec.changes).await?;
        let lines = plan(spec.changes, |path| entries.get(path).cloned().flatten())?;
        let tree = self.write_tree(spec.parent, &lines).await?;
        let git = self.git();
        let parent_tree = git.run(["rev-parse", "--verify", "--end-of-options", &format!("{}^{{tree}}", spec.parent)]).await?;
        if String::from_utf8_lossy(&parent_tree).trim() == tree {
            return Err(EditError::NothingChanged);
        }
        let signature = Signature::new(spec.author, spec.at);
        let commit = git.commit_tree(&tree, &[spec.parent], spec.message.as_bytes(), &signature, &signature).await?;

        let base = Git { binary: &self.binary, dir: &self.repo };
        match hand_over(&git, &base, &target, &commit, &expected, "rbit: edit files in the browser").await {
            Ok(()) => Ok(Committed { commit, branch: branch.to_owned() }),
            Err(MergeError::BaseMoved) if spec.new_branch.is_some() => Err(EditError::BranchExists(branch.to_owned())),
            Err(error) => Err(error.into()),
        }
    }

    /// The entries of the parent's tree at every path the changes name and at each of
    /// their ancestors (`None`: nothing there).
    async fn lookup(&self, parent: &str, changes: &[Change]) -> Result<BTreeMap<String, Option<Entry>>, EditError> {
        let mut paths = BTreeSet::new();
        for path in changes.iter().flat_map(Change::paths) {
            paths.insert(path.as_str().to_owned());
            paths.extend(path.ancestors().map(str::to_owned));
        }
        let (repo, parent) = (self.repo.clone(), parent.to_owned());
        let found = tokio::task::spawn_blocking(move || {
            let repo = gix::open_opts(&repo, gix::open::Options::isolated()).map_err(GitError::backend)?;
            let id = gix::ObjectId::from_hex(parent.as_bytes()).map_err(GitError::backend)?;
            let tree = repo.find_commit(id).map_err(|_| GitError::NotFound("commit"))?.tree().map_err(GitError::backend)?;
            let mut found = BTreeMap::new();
            for path in paths {
                let entry = tree.lookup_entry_by_path(&path).map_err(GitError::backend)?;
                let entry =
                    entry.map(|entry| Entry { mode: Mode::from_gix(entry.mode().kind()), id: entry.object_id().to_hex().to_string() });
                found.insert(path, entry);
            }
            Ok::<_, GitError>(found)
        })
        .await
        .map_err(GitError::from)??;
        Ok(found)
    }

    /// The tree of `parent` with `lines` applied, built on the scratch index.
    async fn write_tree(&self, parent: &str, lines: &[IndexLine]) -> Result<String, EditError> {
        let index = self.scratch.path().join(INDEX_FILE);
        let git = self.git();
        let indexed = |command: &mut tokio::process::Command| {
            command.env("GIT_INDEX_FILE", &index);
        };

        let mut read = git.command();
        indexed(&mut read);
        read.args(["read-tree", "--end-of-options", parent]);
        expect_success(read.output().await?, "read-tree")?;

        let mut update = git.command();
        indexed(&mut update);
        update.args(["update-index", "-z", "--add", "--remove", "--index-info"]).stdin(Stdio::piped());
        let mut child = update.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            let mut input = Vec::new();
            for line in lines {
                line.write_to(&mut input);
            }
            stdin.write_all(&input).await?;
            stdin.shutdown().await?;
        }
        let output = child.wait_with_output().await?;
        if !output.status.success() {
            // Our checks passed, so Git refused a path itself (e.g. one it considers unsafe).
            return Err(match failure(&output, "update-index") {
                MergeError::Failed(message) if message.contains("nvalid path") => {
                    EditError::Rejected("Git refuses one of these paths".to_owned())
                }
                other => other.into(),
            });
        }

        let mut write = git.command();
        indexed(&mut write);
        write.arg("write-tree");
        let tree = String::from_utf8_lossy(&expect_success(write.output().await?, "write-tree")?).trim().to_owned();
        validate_id(&tree)?;
        Ok(tree)
    }
}

impl Change {
    /// Every path the change names.
    fn paths(&self) -> impl Iterator<Item = &FilePath> {
        let (first, second) = match self {
            Self::Create { path, .. } | Self::Update { path, .. } | Self::Delete { path } | Self::Put { path, .. } => (path, None),
            Self::Move { from, to, .. } => (from, Some(to)),
        };
        std::iter::once(first).chain(second)
    }
}

/// A tree entry's kind, as far as edits care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    File,
    Executable,
    Symlink,
    Directory,
    Submodule,
}

impl Mode {
    fn from_gix(kind: gix::object::tree::EntryKind) -> Self {
        use gix::object::tree::EntryKind;
        match kind {
            EntryKind::Tree => Self::Directory,
            EntryKind::Blob => Self::File,
            EntryKind::BlobExecutable => Self::Executable,
            EntryKind::Link => Self::Symlink,
            EntryKind::Commit => Self::Submodule,
        }
    }

    fn is_file(self) -> bool {
        matches!(self, Self::File | Self::Executable | Self::Symlink)
    }

    fn octal(self) -> &'static str {
        match self {
            Self::File => "100644",
            Self::Executable => "100755",
            Self::Symlink => "120000",
            Self::Directory => "040000",
            Self::Submodule => "160000",
        }
    }
}

/// An existing tree entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    mode: Mode,
    id: String,
}

/// One record for `update-index --index-info`: a removal or a write.
#[derive(Debug, Clone, PartialEq, Eq)]
enum IndexLine {
    Remove(String),
    Write { mode: Mode, id: String, path: String },
}

impl IndexLine {
    /// `<mode> <id>\t<path>\0` (mode `0` and the null id remove the path).
    fn write_to(&self, out: &mut Vec<u8>) {
        let (mode, id, path) = match self {
            Self::Remove(path) => ("0", "0".repeat(40), path),
            Self::Write { mode, id, path } => (mode.octal(), id.clone(), path),
        };
        out.extend_from_slice(format!("{mode} {id}\t{path}").as_bytes());
        out.push(0);
    }
}

fn rejected(message: String) -> EditError {
    EditError::Rejected(message)
}

/// Checks `changes` against the parent's tree (`existing` answers what is at a path) and
/// turns them into index records: removals first, then writes (see the module docs).
fn plan(changes: &[Change], existing: impl Fn(&str) -> Option<Entry>) -> Result<Vec<IndexLine>, EditError> {
    // What leaves the tree: deleted files and the sources of moves.
    let mut removed: BTreeMap<&str, Entry> = BTreeMap::new();
    for change in changes {
        let (Change::Delete { path } | Change::Move { from: path, .. }) = change else { continue };
        let entry = match existing(path.as_str()) {
            Some(entry) if entry.mode.is_file() => entry,
            Some(entry) if entry.mode == Mode::Directory => return Err(rejected(format!("{path} is a directory, not a file"))),
            Some(_) => return Err(rejected(format!("{path} is a submodule, not a file"))),
            None => return Err(rejected(format!("there is no file called {path}"))),
        };
        if removed.insert(path.as_str(), entry).is_some() {
            return Err(rejected(format!("{path} is changed twice")));
        }
    }

    // What is written, and where.
    let mut written: BTreeMap<&str, (Mode, String)> = BTreeMap::new();
    for change in changes {
        let (path, write) = match change {
            Change::Create { path, blob } => {
                free(path, &existing, &removed)?;
                (path, (Mode::File, blob.as_str().to_owned()))
            }
            Change::Update { path, blob } => match existing(path.as_str()) {
                Some(entry) if entry.mode.is_file() => (path, (entry.mode, blob.as_str().to_owned())),
                Some(_) => return Err(rejected(format!("{path} is not a file"))),
                None => return Err(rejected(format!("there is no file called {path}"))),
            },
            Change::Move { from, to, blob } => {
                let source = removed.get(from.as_str()).cloned().ok_or_else(|| rejected(format!("there is no file called {from}")))?;
                if from != to {
                    free(to, &existing, &removed)?;
                }
                (to, (source.mode, blob.as_ref().map_or(source.id, |blob| blob.as_str().to_owned())))
            }
            Change::Put { path, blob } => match existing(path.as_str()) {
                // An upload replacing a symlink writes a regular file.
                Some(entry) if entry.mode.is_file() && entry.mode != Mode::Symlink => (path, (entry.mode, blob.as_str().to_owned())),
                Some(entry) if entry.mode.is_file() => (path, (Mode::File, blob.as_str().to_owned())),
                Some(_) => return Err(rejected(format!("{path} is a directory"))),
                None => (path, (Mode::File, blob.as_str().to_owned())),
            },
            Change::Delete { .. } => continue,
        };
        if written.insert(path.as_str(), write).is_some() {
            return Err(rejected(format!("{path} is written twice")));
        }
    }

    // No written path may run through a file: an existing one that stays, or a written one.
    for path in changes.iter().flat_map(Change::paths).filter(|path| written.contains_key(path.as_str())) {
        for ancestor in path.ancestors() {
            if written.contains_key(ancestor) {
                return Err(rejected(format!("{ancestor} cannot be both a file and a directory")));
            }
            match existing(ancestor) {
                Some(entry) if entry.mode == Mode::Directory => {}
                Some(entry) if entry.mode.is_file() && removed.contains_key(ancestor) => {}
                Some(entry) if entry.mode.is_file() => {
                    return Err(rejected(format!("{ancestor} is a file, so it cannot contain {}", path.file_name())));
                }
                Some(_) => return Err(rejected(format!("{ancestor} is a submodule"))),
                None => {}
            }
        }
    }

    let mut lines: Vec<IndexLine> =
        removed.keys().filter(|path| !written.contains_key(*path)).map(|path| IndexLine::Remove((*path).to_owned())).collect();
    lines.extend(written.into_iter().map(|(path, (mode, id))| IndexLine::Write { mode, id, path: path.to_owned() }));
    Ok(lines)
}

/// Nothing is at `path` (or only a file this edit removes).
fn free(path: &FilePath, existing: &impl Fn(&str) -> Option<Entry>, removed: &BTreeMap<&str, Entry>) -> Result<(), EditError> {
    match existing(path.as_str()) {
        None => Ok(()),
        Some(_) if removed.contains_key(path.as_str()) => Ok(()),
        Some(entry) if entry.mode == Mode::Directory => Err(rejected(format!("a directory called {path} already exists"))),
        Some(_) => Err(rejected(format!("a file called {path} already exists"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(path: &str) -> FilePath {
        FilePath::parse(path).unwrap()
    }

    fn blob(digit: char) -> BlobId {
        BlobId(digit.to_string().repeat(40))
    }

    /// `README` (file), `run.sh` (executable), `link` (symlink), `docs/a.md`, `lib` (submodule).
    fn tree(path: &str) -> Option<Entry> {
        let entry = |mode| Some(Entry { mode, id: "f".repeat(40) });
        match path {
            "README" | "docs/a.md" => entry(Mode::File),
            "run.sh" => entry(Mode::Executable),
            "link" => entry(Mode::Symlink),
            "docs" => entry(Mode::Directory),
            "lib" => entry(Mode::Submodule),
            _ => None,
        }
    }

    fn message(result: Result<Vec<IndexLine>, EditError>) -> String {
        match result {
            Err(EditError::Rejected(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn writes_keep_modes_and_removals_come_first() {
        let changes = [
            Change::Update { path: path("run.sh"), blob: blob('1') },
            Change::Move { from: path("link"), to: path("new/dir/link"), blob: None },
            Change::Delete { path: path("docs/a.md") },
            Change::Create { path: path("docs/b.md"), blob: blob('2') },
        ];
        let lines = plan(&changes, tree).unwrap();
        assert_eq!(
            lines,
            [
                IndexLine::Remove("docs/a.md".into()),
                IndexLine::Remove("link".into()),
                IndexLine::Write { mode: Mode::File, id: "2".repeat(40), path: "docs/b.md".into() },
                IndexLine::Write { mode: Mode::Symlink, id: "f".repeat(40), path: "new/dir/link".into() },
                IndexLine::Write { mode: Mode::Executable, id: "1".repeat(40), path: "run.sh".into() },
            ]
        );
        let mut bytes = Vec::new();
        lines[0].write_to(&mut bytes);
        assert_eq!(bytes, format!("0 {}\tdocs/a.md\0", "0".repeat(40)).as_bytes());
    }

    #[test]
    fn nothing_is_clobbered() {
        assert_eq!(message(plan(&[Change::Create { path: path("README"), blob: blob('1') }], tree)), "a file called README already exists");
        assert_eq!(
            message(plan(&[Change::Create { path: path("docs"), blob: blob('1') }], tree)),
            "a directory called docs already exists"
        );
        let onto = Change::Move { from: path("README"), to: path("run.sh"), blob: None };
        assert_eq!(message(plan(&[onto], tree)), "a file called run.sh already exists");
        assert_eq!(message(plan(&[Change::Put { path: path("docs"), blob: blob('1') }], tree)), "docs is a directory");
        // An upload replaces a file of the same name (keeping its mode) but a symlink becomes a file.
        let put = plan(&[Change::Put { path: path("run.sh"), blob: blob('1') }, Change::Put { path: path("link"), blob: blob('2') }], tree);
        assert_eq!(
            put.unwrap(),
            [
                IndexLine::Write { mode: Mode::File, id: "2".repeat(40), path: "link".into() },
                IndexLine::Write { mode: Mode::Executable, id: "1".repeat(40), path: "run.sh".into() },
            ]
        );
    }

    #[test]
    fn edited_moved_and_deleted_paths_must_be_files() {
        assert_eq!(message(plan(&[Change::Update { path: path("nope"), blob: blob('1') }], tree)), "there is no file called nope");
        assert_eq!(message(plan(&[Change::Delete { path: path("docs") }], tree)), "docs is a directory, not a file");
        assert_eq!(message(plan(&[Change::Delete { path: path("lib") }], tree)), "lib is a submodule, not a file");
        assert_eq!(message(plan(&[Change::Update { path: path("docs"), blob: blob('1') }], tree)), "docs is not a file");
        let twice = [Change::Delete { path: path("README") }, Change::Delete { path: path("README") }];
        assert_eq!(message(plan(&twice, tree)), "README is changed twice");
    }

    #[test]
    fn paths_never_run_through_files() {
        let through = Change::Create { path: path("README/x"), blob: blob('1') };
        assert_eq!(message(plan(&[through], tree)), "README is a file, so it cannot contain x");
        let both = [Change::Create { path: path("a"), blob: blob('1') }, Change::Create { path: path("a/b"), blob: blob('2') }];
        assert_eq!(message(plan(&both, tree)), "a cannot be both a file and a directory");
        let twice = [Change::Create { path: path("a"), blob: blob('1') }, Change::Put { path: path("a"), blob: blob('2') }];
        assert_eq!(message(plan(&twice, tree)), "a is written twice");
        assert_eq!(message(plan(&[Change::Create { path: path("lib/x"), blob: blob('1') }], tree)), "lib is a submodule");
        // A file that is moved away frees its name for a directory.
        let replaced = [Change::Move { from: path("README"), to: path("README/index.md"), blob: None }];
        assert_eq!(
            plan(&replaced, tree).unwrap(),
            [IndexLine::Remove("README".into()), IndexLine::Write { mode: Mode::File, id: "f".repeat(40), path: "README/index.md".into() }]
        );
    }

    #[test]
    fn renaming_onto_itself_is_an_update() {
        let same = [Change::Move { from: path("README"), to: path("README"), blob: Some(blob('3')) }];
        assert_eq!(plan(&same, tree).unwrap(), [IndexLine::Write { mode: Mode::File, id: "3".repeat(40), path: "README".into() }]);
    }
}
