//! The built-in `rbit/upload-artifact` and `rbit/download-artifact` (§12.2, §12.7), and
//! their `actions/*` aliases.
//!
//! * Upload packs the files the `path` lines select (paths or glob patterns inside the
//!   workspace, `!` excluding: `cache::select`) into one tar.gz in the task's temp
//!   directory, then sends it with its SHA-256 through the task, with `retention-days` and
//!   `overwrite`. Names starting with a dot are left out unless `include-hidden-files`, as
//!   in `actions/upload-artifact` v4.4 and later, so `path: .` never takes `.git` along.
//!   `if-no-files-found` decides what nothing to upload means: a warning (the default), an
//!   error, or nothing. `compression-level` is the gzip level.
//! * Download fetches the run's artifacts — one by `name`, those matching `pattern` (`*` and
//!   `?`), or all — checks each against its digest and unpacks it into `path` (the
//!   workspace by default). Several artifacts each get a directory of their name unless
//!   `merge-multiple` is set or one was asked for by name.
//!
//! Archives keep paths relative to the root `actions/upload-artifact` uses: the least common
//! ancestor of the lines' search paths (each pattern's literal start: `dist` for
//! `dist/*.zip`), or the directory of a single file, so `path: dist` stores `dist`'s files
//! at the top. Unpacking never writes outside the destination or through a link
//! (`cache::extract`); a `path` that goes through a link is refused. The archives themselves are packed and downloaded into the task's private
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
    cache::{Area, Roots, Selection, extract, lines, resolve, select, write_archive_at},
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
        Err(error) => return job.fail(&error.replace("cache paths", "artifact paths")),
    };
    if resolved.iter().any(|line| line.area != Area::Workspace) {
        return job.fail("artifact paths are relative to the workspace");
    }
    let workspace = workspace_roots(job);
    let (hidden, windows) = (inputs.flag("include-hidden-files"), job.backend.windows());
    let selected = {
        let (resolved, workspace) = (resolved.clone(), workspace.clone());
        tokio::task::spawn_blocking(move || select(&resolved, &workspace, hidden, windows)).await
    };
    let selected = match selected {
        Ok(Ok(selected)) => selected,
        Ok(Err(error)) => return job.fail(&format!("cannot find the artifact's files: {error}")),
        Err(error) => return job.fail(&format!("cannot find the artifact's files: {error}")),
    };
    if selected.files.is_empty() {
        let message = format!("No files were found with the provided path: {}. No artifacts will be uploaded.", paths.join(", "));
        return match inputs.get("if-no-files-found") {
            "error" => job.fail(&message),
            "ignore" => Outcome::success(BTreeMap::new()),
            _ => {
                job.reporter.log(&format!("##[warning]{message}"));
                Outcome::success(BTreeMap::new())
            }
        };
    }
    let (roots, files) = archive_root(&workspace.workspace, &selected);
    let level = inputs.get("compression-level").parse().unwrap_or(6);
    let archive = match job.private_dir() {
        Ok(dir) => dir.join(format!("artifact-{}.tar.gz", crate::client::idempotency_key())),
        Err(error) => return job.fail(&format!("cannot pack the artifact: {error}")),
    };
    job.reporter.log(&format!(
        "Packing {} file(s) from `{}`",
        files.len(),
        if roots.workspace == workspace.workspace {
            ".".to_owned()
        } else {
            roots.workspace.strip_prefix(&workspace.workspace).unwrap_or(&roots.workspace).to_string_lossy().replace('\\', "/")
        }
    ));
    let packed = {
        let (archive, files, roots) = (archive.clone(), files, roots.clone());
        tokio::task::spawn_blocking(move || {
            write_archive_at(&archive, &files, &roots, level).and_then(|size| Ok((size, file_digest(&archive)?)))
        })
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

/// The archive's root, as `actions/upload-artifact` finds it, and the selected files
/// relative to it. With several search paths, their least common ancestor; with one that
/// is the only file, its directory; otherwise the one search path. The workspace when the
/// root is reached through a link.
fn archive_root(workspace: &Path, selected: &Selection) -> (Roots, Vec<(Area, PathBuf)>) {
    let search: Vec<&str> = selected.search_paths.iter().map(|(_, path)| path.as_str()).collect();
    let base = match search.as_slice() {
        [one] if selected.files.len() == 1 && Path::new(one) == selected.files[0].1 => {
            Path::new(one).parent().map(Path::to_path_buf).unwrap_or_default()
        }
        [one] => PathBuf::from(one),
        many => PathBuf::from(rbit_workflow::glob::common_ancestor(many.iter().copied())),
    };
    let base = Some(base).filter(|base| base.as_os_str().is_empty() || !crosses_link(workspace, base)).unwrap_or_default();
    let files =
        selected.files.iter().filter_map(|(area, relative)| Some((*area, relative.strip_prefix(&base).ok()?.to_path_buf()))).collect();
    (Roots { workspace: workspace.join(&base), home: None }, files)
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
    // Actions: workflow_run. `run-id` reads another run of this repository; another
    // repository's runs are out of reach of the runner's credential.
    let repository = inputs.get("repository").trim();
    let own = job.spec.contexts.github.get("repository").and_then(serde_json::Value::as_str).unwrap_or_default();
    if !repository.is_empty() && !repository.eq_ignore_ascii_case(own) {
        return job.fail(&format!("rbit downloads artifacts of this repository's runs only, not of `{repository}`"));
    }
    let run = match inputs.get("run-id").trim() {
        "" => None,
        text => match text.parse::<i64>() {
            Ok(run) if run > 0 => Some(run),
            _ => return job.fail(&format!("`run-id` must be a run's number id, not `{text}`")),
        },
    };
    let listed = match server.artifacts(job.spec.task_id, run).await {
        Ok(list) => list.artifacts,
        Err(error) => return job.fail(&format!("cannot list the run's artifacts: {error}")),
    };
    let wanted: Vec<_> = listed
        .into_iter()
        .filter(|artifact| if name.is_empty() { pattern.is_empty() || matches(&pattern, &artifact.name) } else { artifact.name == name })
        .collect();
    if wanted.is_empty() {
        let what = if name.is_empty() { "matching artifacts".to_owned() } else { format!("an artifact `{name}`") };
        let whose = run.map_or_else(|| "this run".to_owned(), |run| format!("run {run}"));
        return job.fail(&format!("{whose} has no {what}"));
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
        let digest = match server.download_artifact(job.spec.task_id, run, &artifact.name, &archive).await {
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
        for file in ["dist/app.txt", "dist/linux/app", "docs/guide.md", "README.md"] {
            fs::create_dir_all(workspace.join(file).parent().unwrap()).unwrap();
            fs::write(workspace.join(file), file).unwrap();
        }
        let roots = Roots { workspace: workspace.to_path_buf(), home: None };
        let root = |paths: &[&str]| {
            let lines: Vec<String> = paths.iter().map(|path| (*path).to_owned()).collect();
            let selected = select(&resolve(&lines, &roots, "/__w/app", false).unwrap(), &roots, false, false).unwrap();
            let (base, files) = archive_root(workspace, &selected);
            let relative: Vec<String> = files.into_iter().map(|(_, path)| path.to_string_lossy().replace('\\', "/")).collect();
            (base.workspace.strip_prefix(workspace).unwrap().to_string_lossy().replace('\\', "/"), relative)
        };
        assert_eq!(root(&["dist"]), ("dist".into(), vec!["app.txt".into(), "linux/app".into()]), "a directory's files go at the top");
        assert_eq!(root(&["dist/app.txt"]), ("dist".into(), vec!["app.txt".into()]), "so does a single file");
        assert_eq!(root(&["dist/app.txt", "dist/linux"]), ("dist".into(), vec!["app.txt".into(), "linux/app".into()]));
        assert_eq!(root(&["dist", "docs"]).0, "");
        assert_eq!(root(&["README.md", "dist"]).0, "");
        assert_eq!(root(&["dist/*.txt"]), ("dist".into(), vec!["app.txt".into()]), "a pattern's literal start");
        assert_eq!(root(&["dist/**/app*", "!dist/linux"]), ("dist".into(), vec!["app.txt".into()]), "exclusions do not move the root");
        assert_eq!(root(&["dist", "missing/file"]).0, "", "every search path counts, found or not, as on GitHub");
    }
}
