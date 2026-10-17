//! Merging pull requests (merge commit, squash, rebase) and trial merges for
//! mergeability, on a temporary repository under `<tmp_dir>/merges`; the result reaches the
//! base repository only through one atomic, lease-protected ref update.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! # How a merge runs
//!
//! 1. A scratch repository is created under `<tmp_dir>/merges/<random>` whose only object
//!    source is the base repository (an [`Overlay`]: what `git clone --shared
//!    --no-checkout` gives, without copying refs or configuration). The pull request's
//!    head is in the base repository already (`refs/pull/{n}/head`).
//! 2. The merge is computed **without a working tree**: `git merge-tree --write-tree`
//!    merges trees in memory and writes the result's objects into the scratch repository;
//!    `git commit-tree` makes the commits. No file of the pull request is ever checked
//!    out, so `.gitattributes` filters, line-ending conversion, hooks and editors never
//!    come into play.
//!    * **merge**: one commit with the base tip and the head as parents;
//!    * **squash**: one commit with the base tip as parent and the merge's tree, authored
//!      by the pull request's author;
//!    * **rebase**: every non-merge commit of the head that the base lacks, oldest first,
//!      replayed with `merge-tree --merge-base=<its parent>` onto the moving tip; each
//!      keeps its author and message, the merger becomes the committer; commits that
//!      become empty are dropped (like `git rebase`).
//!
//!    A conflict ends the merge with the conflicting paths.
//! 3. The base repository fetches the result's objects from the scratch repository and
//!    then moves `refs/heads/<base>` with `update-ref <new> <expected base tip>`: an
//!    atomic compare-and-swap (the lease), refused as [`MergeError::BaseMoved`] when the
//!    branch moved meanwhile. Nothing else in the base repository changes.
//!
//! Every `git` here is [`hardened`]: the transports' scrubbed environment without system
//! or global configuration, no terminal prompts, `LC_ALL=C`, no replace refs, and
//! `-c core.autocrlf=false -c core.hooksPath=<null device> -c commit.gpgSign=false
//! -c rerere.enabled=false -c gc.auto=0 -c maintenance.auto=false -c core.fsmonitor=false`;
//! revisions are full ids (validated) or `refs/heads/<validated name>` after
//! `--end-of-options`; nothing from user input becomes an option. Identities and dates
//! come from the caller (noreply addresses, the service clock). The whole merge is bounded
//! by a timeout (children are killed on drop) and the scratch directory is removed on
//! every exit path.

use std::{
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::Duration,
};

use rbit_core::{Timestamp, issue::MergeStrategy, pull::CommitIdentity};
use tokio::{io::AsyncWriteExt, process::Command};

use crate::{
    GitError,
    archive::{git_command, null_device},
    overlay::{MERGES_DIR, Overlay},
    pull_refs::{branch_ref, validate_id},
};

/// Conflicting paths reported at most.
pub const MAX_REPORTED_CONFLICTS: usize = rbit_core::pull::MAX_CONFLICT_PATHS;
/// Commits a rebase replays at most.
pub const MAX_REBASED_COMMITS: usize = 1_000;

/// The scratch repository's ref holding the result while the base repository fetches it.
const RESULT_REF: &str = "refs/heads/rbit-merge-result";

/// A `git` command with the hardened environment and configuration of the module docs,
/// operating on `git_dir` if given.
pub(crate) fn hardened(git_binary: &Path, git_dir: Option<&Path>) -> Command {
    let mut command = git_command(git_binary, git_dir);
    command.env("LC_ALL", "C").env("GIT_TERMINAL_PROMPT", "0").env("GIT_NO_REPLACE_OBJECTS", "1").env("GIT_ASKPASS", "");
    let hooks = format!("core.hooksPath={}", null_device().to_string_lossy());
    for setting in [
        "core.autocrlf=false",
        hooks.as_str(),
        "commit.gpgSign=false",
        "rerere.enabled=false",
        "gc.auto=0",
        "maintenance.auto=false",
        "core.fsmonitor=false",
        "core.useReplaceRefs=false",
        "fetch.writeCommitGraph=false",
    ] {
        command.arg("-c").arg(setting);
    }
    command
}

/// Where merges run and how long they may take.
#[derive(Debug, Clone)]
pub struct MergeEnvironment {
    pub git_binary: PathBuf,
    /// `<data_dir>/tmp`; scratch repositories go to its `merges` subdirectory.
    pub tmp_dir: PathBuf,
    pub timeout: Duration,
}

/// One merge to perform in the base repository at `repo`.
#[derive(Debug, Clone)]
pub struct MergeSpec<'a> {
    pub repo: &'a Path,
    pub base_branch: &'a str,
    /// The base tip the merge is computed on and the lease expects.
    pub base_tip: &'a str,
    /// The head commit (already in the base repository).
    pub head: &'a str,
    pub strategy: MergeStrategy,
    /// The full message of the merge or squash commit (unused for rebases).
    pub message: &'a str,
    /// The author of a squash commit (the pull request's author); merge commits and
    /// rebased commits' committer are the merger.
    pub author: &'a CommitIdentity,
    pub merger: &'a CommitIdentity,
    /// Author and committer date of new commits (committer date of rebased ones).
    pub at: Timestamp,
}

/// A successful merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// The new tip of the base branch (the merge commit, the squash commit or the last
    /// rebased commit).
    pub commit: String,
}

/// The outcome of a trial merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrialMerge {
    Clean,
    /// The conflicting paths, at most [`MAX_REPORTED_CONFLICTS`], sorted.
    Conflict(Vec<String>),
}

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// The merge conflicts in these paths (sorted, at most [`MAX_REPORTED_CONFLICTS`]).
    #[error("the merge has conflicts")]
    Conflict(Vec<String>),
    /// The base branch moved after the merge was computed; nothing was changed.
    #[error("the base branch changed; try again")]
    BaseMoved,
    /// Head and base share no history.
    #[error("the branches have entirely different histories")]
    Unrelated,
    /// The head is already part of the base.
    #[error("the base branch already contains these changes")]
    NothingToMerge,
    #[error("the merge took longer than {0:?}")]
    TimedOut(Duration),
    #[error("git failed: {0}")]
    Failed(String),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl From<std::io::Error> for MergeError {
    fn from(error: std::io::Error) -> Self {
        Self::Git(GitError::Io(error))
    }
}

/// Merges `spec.head` into `spec.base_branch` of `spec.repo` with `spec.strategy`.
pub async fn merge(env: &MergeEnvironment, spec: &MergeSpec<'_>) -> Result<Merged, MergeError> {
    tokio::time::timeout(env.timeout, merge_inner(env, spec)).await.map_err(|_| MergeError::TimedOut(env.timeout))?
}

/// Computes whether `head` merges cleanly into `base` (commit ids in `repo`) without
/// changing anything in `repo`.
pub async fn trial(env: &MergeEnvironment, repo: &Path, base: &str, head: &str) -> Result<TrialMerge, MergeError> {
    validate_id(base)?;
    validate_id(head)?;
    let work = async {
        let scratch = Overlay::create_in(&env.tmp_dir.join(MERGES_DIR), &[repo]).await?;
        let git = Git { binary: &env.git_binary, dir: scratch.path() };
        if git.merge_base(base, head).await?.is_none() {
            return Err(MergeError::Unrelated);
        }
        match git.merge_tree(None, base, head).await? {
            Ok(_) => Ok(TrialMerge::Clean),
            Err(paths) => Ok(TrialMerge::Conflict(paths)),
        }
    };
    tokio::time::timeout(env.timeout, work).await.map_err(|_| MergeError::TimedOut(env.timeout))?
}

async fn merge_inner(env: &MergeEnvironment, spec: &MergeSpec<'_>) -> Result<Merged, MergeError> {
    validate_id(spec.base_tip)?;
    validate_id(spec.head)?;
    let target = branch_ref(spec.base_branch)?;
    let scratch = Overlay::create_in(&env.tmp_dir.join(MERGES_DIR), &[spec.repo]).await?;
    let git = Git { binary: &env.git_binary, dir: scratch.path() };

    if git.is_ancestor(spec.head, spec.base_tip).await? {
        return Err(MergeError::NothingToMerge);
    }
    let Some(merge_base) = git.merge_base(spec.base_tip, spec.head).await? else {
        return Err(MergeError::Unrelated);
    };

    let commit = match spec.strategy {
        MergeStrategy::Merge | MergeStrategy::Squash => {
            let tree = git.merge_tree(None, spec.base_tip, spec.head).await?.map_err(MergeError::Conflict)?;
            let (parents, author): (Vec<&str>, &CommitIdentity) = match spec.strategy {
                MergeStrategy::Merge => (vec![spec.base_tip, spec.head], spec.merger),
                _ => (vec![spec.base_tip], spec.author),
            };
            let author = Signature::new(author, spec.at);
            git.commit_tree(&tree, &parents, spec.message.as_bytes(), &author, &Signature::new(spec.merger, spec.at)).await?
        }
        MergeStrategy::Rebase => rebase(&git, spec, &merge_base).await?,
    };

    // Hand the result to the base repository: objects first, then the atomic ref update.
    git.run(["update-ref", "--no-deref", "--end-of-options", RESULT_REF, commit.as_str()]).await?;
    let base = Git { binary: &env.git_binary, dir: spec.repo };
    let mut fetch = base.command();
    fetch
        .args(["fetch", "--quiet", "--no-tags", "--no-write-fetch-head", "--no-recurse-submodules", "--"])
        .arg(scratch.path())
        .arg(RESULT_REF);
    expect_success(fetch.output().await?, "fetch")?;
    let mut update = base.command();
    update
        .args(["update-ref", "--no-deref", "-m", "rbit: merge pull request", "--end-of-options"])
        .arg(&target)
        .arg(&commit)
        .arg(spec.base_tip);
    let output = update.output().await?;
    if !output.status.success() {
        return match base.rev_parse(&target).await? {
            Some(current) if current != spec.base_tip => Err(MergeError::BaseMoved),
            None => Err(MergeError::BaseMoved),
            Some(_) => Err(failure(&output, "update-ref")),
        };
    }
    drop(scratch);
    Ok(Merged { commit })
}

/// Replays the head's commits onto the base tip; returns the new tip.
async fn rebase(git: &Git<'_>, spec: &MergeSpec<'_>, merge_base: &str) -> Result<String, MergeError> {
    let listing = git
        .run([
            "rev-list",
            "--reverse",
            "--topo-order",
            "--no-merges",
            &format!("--max-count={}", MAX_REBASED_COMMITS + 1),
            "--end-of-options",
            spec.head,
            &format!("^{}", spec.base_tip),
        ])
        .await?;
    let commits: Vec<String> = String::from_utf8_lossy(&listing).lines().map(str::to_owned).collect();
    if commits.len() > MAX_REBASED_COMMITS {
        return Err(MergeError::Failed(format!("more than {MAX_REBASED_COMMITS} commits cannot be rebased")));
    }
    let committer = Signature::new(spec.merger, spec.at);
    let mut tip = spec.base_tip.to_owned();
    for commit in commits {
        validate_id(&commit)?;
        let original = git.read_commit(&commit).await?;
        let parent = original.first_parent.as_deref().unwrap_or(merge_base);
        let tree = git.merge_tree(Some(parent), &tip, &commit).await?.map_err(MergeError::Conflict)?;
        if tree == git.tree_of(&tip).await? {
            continue; // Already in the base: the commit became empty.
        }
        tip = git.commit_tree(&tree, &[tip.as_str()], &original.message, &original.author, &committer).await?;
    }
    if tip == spec.base_tip {
        return Err(MergeError::NothingToMerge);
    }
    Ok(tip)
}

/// An author or committer: identity and date, as `git commit-tree` takes them.
#[derive(Debug, Clone)]
struct Signature {
    name: String,
    email: String,
    /// Git's internal date format: `@<seconds> <+hhmm>`.
    date: String,
}

impl Signature {
    fn new(identity: &CommitIdentity, at: Timestamp) -> Self {
        Self { name: identity.name.clone(), email: identity.email.clone(), date: format!("@{} +0000", at.seconds()) }
    }
}

/// What a rebase keeps of a commit.
struct OriginalCommit {
    first_parent: Option<String>,
    author: Signature,
    message: Vec<u8>,
}

/// `git` on one repository.
struct Git<'a> {
    binary: &'a Path,
    dir: &'a Path,
}

impl Git<'_> {
    fn command(&self) -> Command {
        let mut command = hardened(self.binary, Some(self.dir));
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command
    }

    /// Runs `args`, returning standard output; a non-zero status is an error.
    async fn run<'s>(&self, args: impl IntoIterator<Item = &'s str>) -> Result<Vec<u8>, MergeError> {
        let mut command = self.command();
        command.args(args);
        let output = command.output().await?;
        let what = "command";
        expect_success(output, what)
    }

    async fn rev_parse(&self, name: &str) -> Result<Option<String>, MergeError> {
        let mut command = self.command();
        command.args(["rev-parse", "--verify", "--quiet", "--end-of-options"]).arg(name);
        let output = command.output().await?;
        Ok(output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned()))
    }

    async fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, MergeError> {
        let mut command = self.command();
        command.args(["merge-base", "--is-ancestor", "--end-of-options", ancestor, descendant]);
        let output = command.output().await?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(failure(&output, "merge-base")),
        }
    }

    async fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>, MergeError> {
        let mut command = self.command();
        command.args(["merge-base", "--end-of-options", a, b]);
        let output = command.output().await?;
        match output.status.code() {
            Some(0) => {
                let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                validate_id(&id)?;
                Ok(Some(id))
            }
            Some(1) => Ok(None),
            _ => Err(failure(&output, "merge-base")),
        }
    }

    async fn tree_of(&self, commit: &str) -> Result<String, MergeError> {
        let out = self.run(["rev-parse", "--verify", "--end-of-options", &format!("{commit}^{{tree}}")]).await?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Merges `theirs` into `ours` in memory: `Ok(tree)` when clean, `Err(paths)` on
    /// conflicts.
    async fn merge_tree(&self, merge_base: Option<&str>, ours: &str, theirs: &str) -> Result<Result<String, Vec<String>>, MergeError> {
        let mut command = self.command();
        command.args(["merge-tree", "--write-tree", "--name-only", "-z", "--no-messages"]);
        if let Some(base) = merge_base {
            command.arg(format!("--merge-base={base}"));
        }
        command.args(["--end-of-options", ours, theirs]);
        let output = command.output().await?;
        let code = output.status.code();
        if !matches!(code, Some(0 | 1)) {
            return Err(failure(&output, "merge-tree"));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut fields = text.split('\0');
        let tree = fields.next().unwrap_or_default().trim().to_owned();
        validate_id(&tree)?;
        if code == Some(0) {
            return Ok(Ok(tree));
        }
        let mut paths: Vec<String> = fields.take_while(|field| !field.is_empty()).map(str::to_owned).collect();
        paths.sort();
        paths.dedup();
        paths.truncate(MAX_REPORTED_CONFLICTS);
        Ok(Err(paths))
    }

    async fn commit_tree(
        &self,
        tree: &str,
        parents: &[&str],
        message: &[u8],
        author: &Signature,
        committer: &Signature,
    ) -> Result<String, MergeError> {
        let mut command = self.command();
        command
            .env("GIT_AUTHOR_NAME", &author.name)
            .env("GIT_AUTHOR_EMAIL", &author.email)
            .env("GIT_AUTHOR_DATE", &author.date)
            .env("GIT_COMMITTER_NAME", &committer.name)
            .env("GIT_COMMITTER_EMAIL", &committer.email)
            .env("GIT_COMMITTER_DATE", &committer.date)
            .args(["commit-tree", "--no-gpg-sign", "-F", "-"]);
        for parent in parents {
            command.arg("-p").arg(parent);
        }
        command.args(["--end-of-options", tree]).stdin(Stdio::piped());
        let mut child = command.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(message).await?;
            stdin.shutdown().await?;
        }
        let output = expect_success(child.wait_with_output().await?, "commit-tree")?;
        let id = String::from_utf8_lossy(&output).trim().to_owned();
        validate_id(&id)?;
        Ok(id)
    }

    /// The first parent, author and raw message of `commit`.
    async fn read_commit(&self, commit: &str) -> Result<OriginalCommit, MergeError> {
        let raw = self.run(["cat-file", "commit", commit]).await?;
        let split = raw.windows(2).position(|pair| pair == b"\n\n");
        let (headers, message) = match split {
            Some(at) => (&raw[..at], raw[at + 2..].to_vec()),
            None => (&raw[..], Vec::new()),
        };
        let headers = String::from_utf8_lossy(headers);
        let mut first_parent = None;
        let mut author = None;
        for line in headers.lines() {
            if let Some(parent) = line.strip_prefix("parent ") {
                first_parent.get_or_insert_with(|| parent.trim().to_owned());
            } else if let Some(ident) = line.strip_prefix("author ") {
                author = parse_ident(ident);
            }
        }
        let author = author.ok_or_else(|| MergeError::Failed(format!("commit {commit} has no readable author")))?;
        Ok(OriginalCommit { first_parent, author, message })
    }
}

/// `Name <email> 1700000000 +0200` → a signature with Git's internal date format.
fn parse_ident(ident: &str) -> Option<Signature> {
    let (name, rest) = ident.split_once('<')?;
    let (email, date) = rest.split_once('>')?;
    let mut date = date.split_whitespace();
    let (seconds, zone) = (date.next()?, date.next().unwrap_or("+0000"));
    seconds.parse::<i64>().ok()?;
    Some(Signature { name: name.trim().to_owned(), email: email.trim().to_owned(), date: format!("@{seconds} {zone}") })
}

fn expect_success(output: Output, what: &str) -> Result<Vec<u8>, MergeError> {
    if output.status.success() { Ok(output.stdout) } else { Err(failure(&output, what)) }
}

fn failure(output: &Output, what: &str) -> MergeError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message: String = stderr.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(500).collect();
    MergeError::Failed(format!("{what} ({}): {message}", output.status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_parsed() {
        let signature = parse_ident("Ada Lovelace <ada@example.com> 1700000000 +0200").unwrap();
        assert_eq!(
            (signature.name.as_str(), signature.email.as_str(), signature.date.as_str()),
            ("Ada Lovelace", "ada@example.com", "@1700000000 +0200")
        );
        assert!(parse_ident("no email 1").is_none());
        assert!(parse_ident("A <a> not-a-date").is_none());
    }
}
