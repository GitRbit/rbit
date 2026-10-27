//! The built-in `rbit/upload-artifact` and `rbit/download-artifact` (§12.2, §12.7), and
//! their `actions/*` aliases.
//!
//! * Upload packs the `path` lines (inside the workspace, as for the cache) into one tar.gz
//!   in the task's temp directory, then sends it with its SHA-256 through the task, with
//!   `retention-days` and `overwrite`. `if-no-files-found` decides what nothing to upload
//!   means: a warning (the default), an error, or nothing.
//! * Download fetches the run's artifacts — one by `name`, those matching `pattern` (`*` and
//!   `?`), or all — checks each against its digest and unpacks it into `path` (the
//!   workspace by default). Several artifacts each get a directory of their name unless
//!   `merge-multiple` is set or one was asked for by name.
//!
//! Archives keep paths relative to the deepest directory holding every uploaded path (as
//! `actions/upload-artifact` does: `path: dist` stores `dist`'s files at the top), and unpacking never writes outside the
//! destination or through a link (`cache::extract`); a `path` that goes through a link is
//! refused. The archives themselves are packed and downloaded into the task's private
//! directory, which no container mounts, so a job cannot plant a link where one is written. Without a server (`rbit-runner exec`)
//! both steps fail and say why.

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use rbit_workflow::protocol::{ArtifactQuery, valid_artifact_name};
use sha2::{Digest, Sha256};

use super::{
    Inputs, Outcome,
    cache::{Area, Roots, extract, lines, resolve, write_archive},
};
use crate::{backend::Backend, job::Job, untrusted::crosses_link};

fn hex(digest: &[u8]) -> String {
    digest.iter().fold(String::with_capacity(64), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The SHA-256 of the file at `path`, as hex.
fn file_digest(path: &Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Whether `name` matches `pattern`, where `*` is any run of characters and `?` any one.
pub fn matches(pattern: &str, name: &str) -> bool {
    fn go(pattern: &[char], name: &[char]) -> bool {
        match (pattern.split_first(), name.split_first()) {
            (None, None) => true,
            (Some(('*', rest)), _) => go(rest, name) || (!name.is_empty() && go(pattern, &name[1..])),
            (Some(('?', rest)), Some((_, tail))) => go(rest, tail),
            (Some((expected, rest)), Some((actual, tail))) => expected == actual && go(rest, tail),
            _ => false,
        }
    }
    go(&pattern.chars().collect::<Vec<_>>(), &name.chars().collect::<Vec<_>>())
}

/// The workspace, as the archive's root.
fn workspace_roots(job: &Job) -> Roots {
    Roots { workspace: job.dirs.workspace.clone(), home: None }
}

/// Runs `rbit/upload-artifact`.
pub async fn upload(job: &mut Job, inputs: &Inputs) -> Outcome {
    let Some(server) = job.server.clone() else {
        return job.fail("artifacts need an rbit server; `rbit-runner exec` cannot upload them");
    };
    let name = inputs.get("name").trim().to_owned();
    if !valid_artifact_name(&name) {
        return job.fail(&format!("`{name}` is not a valid artifact name: use 1 to 128 of `A-Z a-z 0-9 . _ -`, not `.` or `..`"));
    }
    let paths = lines(inputs.get("path"));
    let container = matches!(job.backend, Backend::Container(_));
    let resolved = match resolve(&paths, &workspace_roots(job), &job.backend.view().workspace, container) {
        Ok(resolved) => resolved,
        Err(error) => return job.fail(&error.replace("rbit's cache", "rbit's artifacts").replace("cache paths", "artifact paths")),
    };
    let workspace = workspace_roots(job);
    if !resolved.iter().any(|(_, relative)| workspace.workspace.join(relative).exists()) {
        let message = format!("no files were found for the artifact `{name}`");
        return match inputs.get("if-no-files-found") {
            "error" => job.fail(&message),
            "ignore" => Outcome::success(BTreeMap::new()),
            _ => {
                job.reporter.log(&format!("##[warning]{message}; nothing was uploaded"));
                Outcome::success(BTreeMap::new())
            }
        };
    }
    let (roots, resolved) = archive_root(&workspace.workspace, resolved);
    let archive = match job.private_dir() {
        Ok(dir) => dir.join(format!("artifact-{}.tar.gz", crate::client::idempotency_key())),
        Err(error) => return job.fail(&format!("cannot pack the artifact: {error}")),
    };
    let packed = {
        let (archive, resolved, roots) = (archive.clone(), resolved.clone(), roots.clone());
        tokio::task::spawn_blocking(move || write_archive(&archive, &resolved, &roots).and_then(|size| Ok((size, file_digest(&archive)?))))
            .await
    };
    let (size, digest) = match packed {
        Ok(Ok(packed)) => packed,
        Ok(Err(error)) => return job.fail(&format!("cannot pack the artifact: {error}")),
        Err(error) => return job.fail(&format!("cannot pack the artifact: {error}")),
    };
    let query = ArtifactQuery { retention_days: inputs.get("retention-days").trim().parse().ok(), overwrite: inputs.flag("overwrite") };
    job.reporter.log(&format!("Uploading the artifact `{name}` ({size} bytes)"));
    let uploaded = server.upload_artifact(job.spec.task_id, &name, &query, &digest, &archive).await;
    let _ = fs::remove_file(&archive);
    match uploaded {
        Ok(info) => {
            job.reporter.log(&format!("Uploaded `{}`: {} bytes, sha256 {}", info.name, info.size, info.sha256));
            Outcome::success(BTreeMap::from([("artifact-digest".to_owned(), info.sha256)]))
        }
        Err(error) => job.fail(&format!("the artifact `{name}` was not uploaded: {error}")),
    }
}

/// The archive's root and `resolved` relative to it: the deepest directory holding every
/// path that exists (a directory itself, a file its parent), as `actions/upload-artifact`
/// does, so `path: dist` stores `dist`'s files at the top. The workspace when that
/// directory is reached through a link.
fn archive_root(workspace: &Path, resolved: Vec<(Area, PathBuf)>) -> (Roots, Vec<(Area, PathBuf)>) {
    let mut common: Option<PathBuf> = None;
    for (_, relative) in &resolved {
        let source = workspace.join(relative);
        let directory = if source.is_dir() {
            relative.clone()
        } else if source.exists() {
            relative.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            continue;
        };
        common = Some(match common {
            None => directory,
            Some(common) => common.components().zip(directory.components()).take_while(|(a, b)| a == b).map(|(a, _)| a).collect(),
        });
    }
    let base = common.filter(|base| !crosses_link(workspace, base)).unwrap_or_default();
    let resolved =
        resolved.into_iter().filter_map(|(area, relative)| Some((area, relative.strip_prefix(&base).ok()?.to_path_buf()))).collect();
    (Roots { workspace: workspace.join(&base), home: None }, resolved)
}

/// Runs `rbit/download-artifact`.
pub async fn download(job: &mut Job, inputs: &Inputs) -> Outcome {
    let Some(server) = job.server.clone() else {
        return job.fail("artifacts need an rbit server; `rbit-runner exec` cannot download them");
    };
    let (name, pattern) = (inputs.get("name").trim().to_owned(), inputs.get("pattern").trim().to_owned());
    let destination = match destination(job, inputs.get("path").trim()) {
        Ok(destination) => destination,
        Err(error) => return job.fail(&error),
    };
    let listed = match server.artifacts(job.spec.task_id).await {
        Ok(list) => list.artifacts,
        Err(error) => return job.fail(&format!("cannot list the run's artifacts: {error}")),
    };
    let wanted: Vec<_> = listed
        .into_iter()
        .filter(|artifact| if name.is_empty() { pattern.is_empty() || matches(&pattern, &artifact.name) } else { artifact.name == name })
        .collect();
    if wanted.is_empty() {
        let what = if name.is_empty() { "matching artifacts".to_owned() } else { format!("an artifact `{name}`") };
        return job.fail(&format!("this run has no {what}"));
    }
    let separate = name.is_empty() && !inputs.flag("merge-multiple");
    let private = match job.private_dir() {
        Ok(dir) => dir,
        Err(error) => return job.fail(&format!("cannot download artifacts: {error}")),
    };
    for artifact in wanted {
        // The server never lists such a name; the directory it would make is checked anyway.
        if !valid_artifact_name(&artifact.name) {
            return job.fail(&format!("the run has an artifact with an invalid name, `{}`", artifact.name.escape_debug()));
        }
        let archive = private.join(format!("download-{}.tar.gz", crate::client::idempotency_key()));
        let digest = match server.download_artifact(job.spec.task_id, &artifact.name, &archive).await {
            Ok(digest) => digest,
            Err(error) => return job.fail(&format!("cannot download the artifact `{}`: {error}", artifact.name)),
        };
        let target = if separate { destination.join(&artifact.name) } else { destination.clone() };
        let unpacked = {
            let (archive, target, expected) = (archive.clone(), target.clone(), artifact.sha256.clone());
            let workspace = job.dirs.workspace.clone();
            tokio::task::spawn_blocking(move || {
                let actual = file_digest(&archive)?;
                if actual != expected || (!digest.is_empty() && digest != expected) {
                    return Err(std::io::Error::other("its content does not match its digest"));
                }
                if !inside(&workspace, &target) {
                    return Err(std::io::Error::other("its destination is outside the workspace or goes through a symbolic link"));
                }
                fs::create_dir_all(&target)?;
                extract(&archive, &Roots { workspace: target, home: None })
            })
            .await
        };
        let _ = fs::remove_file(&archive);
        match unpacked {
            Ok(Ok(files)) => job.reporter.log(&format!("Downloaded `{}`: {files} file(s) into {}", artifact.name, target.display())),
            Ok(Err(error)) => return job.fail(&format!("cannot unpack the artifact `{}`: {error}", artifact.name)),
            Err(error) => return job.fail(&format!("cannot unpack the artifact `{}`: {error}", artifact.name)),
        }
    }
    Outcome::success(BTreeMap::from([("download-path".to_owned(), destination.display().to_string())]))
}

/// Whether `target` is `workspace` or below it, by plain names (no `..`, which a lexical
/// prefix check would let through) and through no symbolic link.
fn inside(workspace: &Path, target: &Path) -> bool {
    target.strip_prefix(workspace).is_ok_and(|relative| {
        relative.components().all(|component| matches!(component, Component::Normal(_))) && !crosses_link(workspace, relative)
    })
}

/// Where `path` (relative to the workspace, or the workspace's own path) is on the host.
fn destination(job: &Job, path: &str) -> Result<PathBuf, String> {
    let workspace = job.dirs.workspace.clone();
    if path.is_empty() {
        return Ok(workspace);
    }
    let view = job.backend.view().workspace.clone();
    let relative = workspace_relative(&view, path);
    let relative = Path::new(relative);
    if relative.is_absolute()
        || !relative.components().all(|component| matches!(component, std::path::Component::Normal(_) | std::path::Component::CurDir))
    {
        return Err(format!("`{path}` is outside the workspace; download into a path inside it"));
    }
    if crosses_link(&workspace, relative) {
        return Err(format!("`{path}` goes through a symbolic link; download into a directory inside the workspace"));
    }
    Ok(workspace.join(relative))
}

/// `path` relative to the job's view of the workspace, `view`, when it is the workspace or
/// below it (`/__w/app2` is not inside `/__w/app`); otherwise `path` unchanged.
fn workspace_relative<'a>(view: &str, path: &'a str) -> &'a str {
    path.strip_prefix(view)
        .filter(|rest| rest.is_empty() || rest.starts_with(['/', '\\']))
        .map_or(path, |rest| rest.trim_start_matches(['/', '\\']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_stay_in_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        fs::create_dir_all(workspace.join("dist")).unwrap();
        assert!(inside(&workspace, &workspace) && inside(&workspace, &workspace.join("dist")));
        assert!(!inside(&workspace, &workspace.join("..")), "a lexical prefix is not enough");
        assert!(!inside(&workspace, &workspace.join("dist/../..")));
        assert!(!inside(&workspace, dir.path()));
    }

    #[test]
    fn patterns_match_like_globs() {
        assert!(matches("dist-*", "dist-linux"));
        assert!(matches("*", "anything"));
        assert!(matches("log-??", "log-01"));
        assert!(!matches("log-??", "log-1"));
        assert!(!matches("dist-*", "build-linux"));
        assert!(matches("a*b*c", "aXXbYc"));
    }

    #[test]
    fn only_paths_below_the_workspace_are_made_relative() {
        assert_eq!(workspace_relative("/__w/app", "/__w/app/dist"), "dist");
        assert_eq!(workspace_relative("/__w/app", "/__w/app"), "");
        assert_eq!(workspace_relative("/__w/app", "dist/out"), "dist/out");
        assert_eq!(workspace_relative("/__w/app", "/__w/app2/out"), "/__w/app2/out", "a sibling is not inside");
    }

    #[test]
    fn archives_are_rooted_where_their_paths_meet() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        fs::create_dir_all(workspace.join("dist/linux")).unwrap();
        fs::create_dir_all(workspace.join("docs")).unwrap();
        fs::write(workspace.join("dist/app.txt"), "app").unwrap();
        fs::write(workspace.join("dist/linux/app"), "bin").unwrap();
        fs::write(workspace.join("README.md"), "readme").unwrap();
        let root = |paths: &[&str]| {
            let resolved = paths.iter().map(|path| (Area::Workspace, PathBuf::from(path))).collect();
            let (roots, resolved) = archive_root(workspace, resolved);
            let relative: Vec<_> = resolved.into_iter().map(|(_, path)| path).collect();
            (roots.workspace.strip_prefix(workspace).unwrap().to_path_buf(), relative)
        };
        assert_eq!(root(&["dist"]), (PathBuf::from("dist"), vec![PathBuf::new()]), "a directory's files go at the top");
        assert_eq!(root(&["dist/app.txt"]), (PathBuf::from("dist"), vec![PathBuf::from("app.txt")]), "so does a file");
        assert_eq!(root(&["dist/app.txt", "dist/linux"]), (PathBuf::from("dist"), vec![PathBuf::from("app.txt"), PathBuf::from("linux")]));
        assert_eq!(root(&["dist", "docs"]), (PathBuf::new(), vec![PathBuf::from("dist"), PathBuf::from("docs")]));
        assert_eq!(root(&["README.md", "dist"]).0, PathBuf::new());
        assert_eq!(root(&["dist", "missing/file"]), (PathBuf::from("dist"), vec![PathBuf::new()]), "missing paths do not count");
    }
}
