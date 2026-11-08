//! Private, durable files for backups and restores.
//!
//! A backup is complete when its manifest exists, and a restore when its marker is gone,
//! so everything written before must have reached the disk first: neither `VACUUM INTO`
//! nor `git clone` nor `std::fs::copy` syncs what it writes. [`sync_tree`] syncs every file
//! and (on Unix, where directory entries are synced separately) every directory below a
//! root before that last step, and [`sync_dir`] the directory holding a renamed entry.

use std::path::{Path, PathBuf};

use anyhow::Context;
use tokio::io::AsyncWriteExt;

/// Creates `dir` (and missing parents), accessible only to its owner on Unix.
pub async fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = tokio::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(dir).await
}

/// Creates the new file `path`, readable only by its owner on Unix.
pub async fn create_private_file(path: &Path) -> anyhow::Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path).await.with_context(|| format!("cannot create {}", path.display()))
}

/// Writes `contents` to the new private file `path` and syncs it.
pub async fn write_private(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let mut file = create_private_file(path).await?;
    file.write_all(contents).await?;
    file.sync_all().await.with_context(|| format!("cannot sync {}", path.display()))
}

/// Writes `contents` to `path` through a synced temporary file renamed into place, then
/// syncs the directory, so `path` is either absent or complete after a power loss.
pub async fn publish(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let staging = path.with_extension("tmp");
    write_private(&staging, contents).await?;
    tokio::fs::rename(&staging, path).await.with_context(|| format!("cannot rename {}", staging.display()))?;
    sync_dir(path.parent().unwrap_or(Path::new("."))).await
}

/// Syncs every file and directory below `root`, and `root` itself.
pub async fn sync_tree(root: &Path) -> anyhow::Result<()> {
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut pending: Vec<PathBuf> = vec![root];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).with_context(|| format!("cannot list {}", dir.display()))? {
                let entry = entry?;
                let path = entry.path();
                if entry.file_type()?.is_dir() {
                    pending.push(path);
                } else {
                    sync_file(&path)?;
                }
            }
            sync_dir_blocking(&dir)?;
        }
        Ok(())
    })
    .await?
}

/// Syncs the directory `dir`, so entries created or renamed in it survive a power loss.
/// Windows has no directory handle to flush; NTFS journals the entries themselves.
pub async fn sync_dir(dir: &Path) -> anyhow::Result<()> {
    let dir = dir.to_owned();
    tokio::task::spawn_blocking(move || sync_dir_blocking(&dir)).await?
}

fn sync_file(path: &Path) -> anyhow::Result<()> {
    // Git writes read-only objects, which Windows cannot open for writing; reading suffices
    // elsewhere, and `FlushFileBuffers` on Windows needs write access.
    let file = if cfg!(windows) { std::fs::OpenOptions::new().write(true).open(path) } else { std::fs::File::open(path) };
    let file = match file {
        Ok(file) => file,
        Err(error) if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied => {
            return sync_read_only(path);
        }
        Err(error) => return Err(error).with_context(|| format!("cannot open {}", path.display())),
    };
    file.sync_all().with_context(|| format!("cannot sync {}", path.display()))
}

/// Syncs a read-only file on Windows by lifting the attribute for the flush.
fn sync_read_only(path: &Path) -> anyhow::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    #[expect(clippy::permissions_set_readonly_false, reason = "restored right after the flush; Windows only")]
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions.clone())?;
    let synced = std::fs::OpenOptions::new().write(true).open(path).and_then(|file| file.sync_all());
    permissions.set_readonly(true);
    std::fs::set_permissions(path, permissions)?;
    synced.with_context(|| format!("cannot sync {}", path.display()))
}

#[cfg(unix)]
fn sync_dir_blocking(dir: &Path) -> anyhow::Result<()> {
    std::fs::File::open(dir).and_then(|dir| dir.sync_all()).with_context(|| format!("cannot sync {}", dir.display()))
}

#[cfg(not(unix))]
#[expect(clippy::unnecessary_wraps, reason = "the Unix version can fail")]
fn sync_dir_blocking(_dir: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_tree_with_read_only_files_is_synced_and_kept_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("object");
        std::fs::write(&file, b"x").unwrap();
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).unwrap();
        sync_tree(dir.path()).await.unwrap();
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());

        publish(&dir.path().join("manifest.json"), b"{}").await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("manifest.json")).unwrap(), b"{}");
        assert!(!dir.path().join("manifest.tmp").exists());
    }
}
