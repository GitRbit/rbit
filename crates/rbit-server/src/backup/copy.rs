//! Copying what an inventory lists: repositories through Git's pack protocol, stored
//! files as they are.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use anyhow::{Context, bail};
use rbit_core::{AccountId, RepoId};
use rbit_git::RepositoryStorage;
use tokio::{process::Command, sync::Semaphore, task::JoinSet};

use super::inventory::{Blob, Kind};

/// Repositories copied at the same time.
const COPY_CONCURRENCY: usize = 4;
/// Environment variables passed through to Git; everything else is cleared.
const INHERITED_ENV: [&str; 5] = ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"];

#[derive(Debug)]
pub struct Copied<Id = i64> {
    pub copied: u64,
    pub missing: Vec<Id>,
}

impl<Id> Default for Copied<Id> {
    fn default() -> Self {
        Self { copied: 0, missing: Vec::new() }
    }
}

pub async fn copy_repositories(
    git: &Path,
    source: &RepositoryStorage,
    target: &RepositoryStorage,
    repositories: &[(RepoId, AccountId)],
) -> anyhow::Result<Copied> {
    let permits = Arc::new(Semaphore::new(COPY_CONCURRENCY));
    let mut copies = JoinSet::new();
    let mut result = Copied::default();
    for &(repo, owner) in repositories {
        let from = source.path_for(owner, repo);
        if !from.join("HEAD").is_file() {
            tracing::warn!(repo = repo.0, path = %from.display(), "repository is in the database but not on disk; skipped");
            result.missing.push(repo.0);
            continue;
        }
        let to = target.path_for(owner, repo);
        let git = git.to_owned();
        let permit = Arc::clone(&permits).acquire_owned().await?;
        copies.spawn(async move {
            let _permit = permit;
            mirror(&git, &from, &to).await.with_context(|| format!("cannot copy repository {} ({})", repo.0, from.display()))
        });
    }
    let mut failures = Vec::new();
    while let Some(copy) = copies.join_next().await {
        match copy? {
            Ok(()) => result.copied += 1,
            Err(error) => {
                tracing::error!("{error:#}");
                failures.push(error);
            }
        }
    }
    if let Some(first) = failures.into_iter().next() {
        return Err(first.context("backup incomplete: at least one repository could not be copied"));
    }
    Ok(result)
}

/// What [`copy_blobs`] did with one kind of stored file.
#[derive(Debug, Default)]
pub struct CopiedBlobs {
    pub attachments: Copied,
    pub lfs: Copied<String>,
    pub artifacts: Copied,
    pub caches: Copied,
    pub logs: Copied,
}

impl CopiedBlobs {
    fn note(&mut self, blob: &Blob, copied: bool) {
        let id = || blob.id.parse::<i64>().unwrap_or_default();
        let counter = match blob.kind {
            Kind::Lfs => {
                if copied {
                    self.lfs.copied += 1;
                } else {
                    self.lfs.missing.push(blob.id.clone());
                }
                return;
            }
            Kind::Attachment => &mut self.attachments,
            Kind::Artifact => &mut self.artifacts,
            Kind::Cache => &mut self.caches,
            Kind::Log => &mut self.logs,
        };
        if copied {
            counter.copied += 1;
        } else if !counter.missing.contains(&id()) {
            counter.missing.push(id());
        }
    }
}

/// Copies each of `blobs` from under `source` to the same place under `target`. Stored
/// files never change once in place (a live log only grows past what the snapshot
/// records), so a copy is consistent; one deleted since the snapshot is reported missing.
pub async fn copy_blobs(source: &Path, target: &Path, blobs: &[Blob]) -> anyhow::Result<CopiedBlobs> {
    // Always present, so restoring can copy them unconditionally.
    for dir in ["attachments", "lfs"] {
        tokio::fs::create_dir_all(target.join(dir)).await?;
    }
    let mut result = CopiedBlobs::default();
    for blob in blobs {
        let to = target.join(&blob.path);
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let copied = match tokio::fs::copy(source.join(&blob.path), &to).await {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => match &blob.compressed {
                Some(compressed) => decompress(&source.join(compressed), &to).await?,
                None => false,
            },
            Err(error) => return Err(error).with_context(|| format!("cannot copy {}", source.join(&blob.path).display())),
        };
        if !copied {
            tracing::warn!(kind = ?blob.kind, id = %blob.id, path = %blob.path.display(), "in the database but not on disk; skipped");
        }
        result.note(blob, copied);
    }
    Ok(result)
}

/// Writes the lines of the gzip file `from` to `to`; `false` when `from` is gone too.
async fn decompress(from: &Path, to: &Path) -> anyhow::Result<bool> {
    let (from, to): (PathBuf, PathBuf) = (from.to_owned(), to.to_owned());
    tokio::task::spawn_blocking(move || {
        let file = match std::fs::File::open(&from) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error).with_context(|| format!("cannot open {}", from.display())),
        };
        let mut output = std::fs::File::create(&to).with_context(|| format!("cannot create {}", to.display()))?;
        std::io::copy(&mut flate2::read::GzDecoder::new(file), &mut output).with_context(|| format!("cannot read {}", from.display()))?;
        Ok(true)
    })
    .await?
}

/// Mirrors one bare repository through the pack protocol, then makes the copy
/// indistinguishable from the original: same `HEAD`, no `origin` remote.
async fn mirror(git: &Path, from: &Path, to: &Path) -> anyhow::Result<()> {
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    run_git(git, |command| {
        command.args(["clone", "--mirror", "--no-local", "--quiet"]).arg(from).arg(to);
    })
    .await?;
    // `clone` points HEAD at the source's HEAD branch only if it exists; copy the
    // symbolic ref itself so empty repositories keep their default branch too.
    tokio::fs::copy(from.join("HEAD"), to.join("HEAD")).await?;
    // Not `git remote remove`: with a mirror refspec it would delete every ref.
    run_git(git, |command| {
        command.arg("--git-dir").arg(to).args(["config", "--remove-section", "remote.origin"]);
    })
    .await
}

pub async fn run_git(git: &Path, configure: impl FnOnce(&mut Command)) -> anyhow::Result<()> {
    let mut command = Command::new(git);
    command.env_clear();
    for key in INHERITED_ENV {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure(&mut command);
    let output = command.output().await.with_context(|| format!("cannot run {}", git.display()))?;
    if !output.status.success() {
        bail!("git exited with {}: {}", output.status, String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}
