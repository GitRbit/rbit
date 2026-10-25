//! `rbit/cache` (restore now, save after the job), `rbit/cache/restore` and
//! `rbit/cache/save`, and their `actions/cache` aliases (§12.5.6).
//!
//! The cache is local to the runner: entries live at
//! `<work_dir>/cache/<repository id>/<scope>/<sha256(key)>.tar.gz` (tar and gzip), with a
//! small `.json` beside each naming its key and paths, and are evicted least recently used
//! once all of them together exceed `cache.max_bytes`. A restore touches the entry it
//! uses, so "used" means read or written.
//!
//! Scopes come from the job specification and keep runs from poisoning each other's
//! caches: a run reads its own ref's scope, then the default branch's, and writes only its
//! own (`pull/<n>` for an untrusted run). A restore looks for the exact key in every scope
//! in order, then for each `restore-keys` prefix the newest matching entry, again scope by
//! scope. An entry only matches with the same `path` list. Entries are immutable: saving a
//! key that exists in the write scope is skipped. `rbit/cache` saves after the last step,
//! only when the job succeeded and the key was not an exact hit.
//!
//! Paths are listed one per line. They are relative to the workspace or, on the host
//! backend, start with `~/` for the runner user's home; container jobs can cache only
//! what is inside the mounted workspace. Glob patterns and exclusions are not supported
//! yet. Archives store symbolic links as links, never what they point to, and a restore
//! never writes through a symbolic link: a job cannot use the cache to write outside the
//! directories it names.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use rbit_workflow::spec::BuiltinKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{Inputs, Outcome};
use crate::{backend::Backend, contexts::hex, job::Job};

/// Longest cache key, as on GitHub.
pub const MAX_KEY_CHARS: usize = 512;

/// The runner's cache directory and its size limit.
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
    pub max_bytes: u64,
}

/// Where a cached path lives: `w/…` in the workspace, `h/…` in the runner user's home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roots {
    pub workspace: PathBuf,
    pub home: Option<PathBuf>,
}

/// The `.json` beside an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    key: String,
    paths: Vec<String>,
}

/// A restorable entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub archive: PathBuf,
    pub key: String,
    pub scope: String,
}

impl Store {
    fn scope_dir(&self, repository: i64, scope: &str) -> PathBuf {
        scope.split('/').fold(self.root.join(repository.to_string()), |dir, part| dir.join(part))
    }

    fn entry(&self, repository: i64, scope: &str, key: &str) -> (PathBuf, PathBuf) {
        let name = hex(&Sha256::digest(key.as_bytes()));
        let dir = self.scope_dir(repository, scope);
        (dir.join(format!("{name}.tar.gz")), dir.join(format!("{name}.json")))
    }

    /// The entry for `key`, else the newest for each prefix in `restore_keys`, searching
    /// `scopes` in order; only entries saved with the same `paths` match.
    pub fn find(&self, repository: i64, scopes: &[String], key: &str, restore_keys: &[String], paths: &[String]) -> Option<Found> {
        for scope in scopes {
            let (archive, meta) = self.entry(repository, scope, key);
            if archive.is_file() && read_meta(&meta).is_some_and(|meta| meta.key == key && meta.paths == paths) {
                return Some(Found { archive, key: key.to_owned(), scope: scope.clone() });
            }
        }
        for prefix in restore_keys {
            for scope in scopes {
                let Ok(entries) = fs::read_dir(self.scope_dir(repository, scope)) else { continue };
                let newest = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
                    .filter_map(|path| {
                        let meta = read_meta(&path)?;
                        let archive = path.with_extension("tar.gz");
                        let modified = fs::metadata(&archive).and_then(|metadata| metadata.modified()).ok()?;
                        (meta.key.starts_with(prefix.as_str()) && meta.paths == paths).then_some((modified, archive, meta.key))
                    })
                    .max_by_key(|(modified, ..)| *modified);
                if let Some((_, archive, key)) = newest {
                    return Some(Found { archive, key, scope: scope.clone() });
                }
            }
        }
        None
    }

    /// Whether `key` is already saved in `scope`.
    pub fn exists(&self, repository: i64, scope: &str, key: &str) -> bool {
        self.entry(repository, scope, key).0.is_file()
    }

    /// Archives `paths` (resolved against `roots`) as `key` in `scope`, then evicts. The
    /// archive's size.
    pub fn save(
        &self,
        repository: i64,
        scope: &str,
        key: &str,
        paths: &[String],
        resolved: &[(Area, PathBuf)],
        roots: &Roots,
    ) -> Result<u64, String> {
        let (archive, meta) = self.entry(repository, scope, key);
        let dir = archive.parent().unwrap_or(&self.root).to_path_buf();
        fs::create_dir_all(&dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        let temp = dir.join(format!(".{}.partial", crate::client::idempotency_key()));
        let written = write_archive(&temp, resolved, roots);
        let size = match written {
            Ok(size) if size > self.max_bytes => {
                let _ = fs::remove_file(&temp);
                return Err(format!("the cache entry would be {size} bytes, more than cache.max_bytes ({})", self.max_bytes));
            }
            Ok(size) => size,
            Err(error) => {
                let _ = fs::remove_file(&temp);
                return Err(format!("cannot write the cache entry: {error}"));
            }
        };
        let meta_text = serde_json::to_vec(&Meta { key: key.to_owned(), paths: paths.to_vec() }).unwrap_or_default();
        fs::write(&meta, meta_text)
            .and_then(|()| fs::rename(&temp, &archive))
            .map_err(|error| format!("cannot store the cache entry: {error}"))?;
        self.evict();
        Ok(size)
    }

    /// Removes the least recently used entries until the cache fits `max_bytes`.
    pub fn evict(&self) {
        let mut entries = Vec::new();
        collect_archives(&self.root, &mut entries);
        let mut total: u64 = entries.iter().map(|(_, size, _)| size).sum();
        entries.sort_by_key(|(used, ..)| *used);
        for (_, size, archive) in entries {
            if total <= self.max_bytes {
                break;
            }
            if fs::remove_file(&archive).is_ok() {
                let _ = fs::remove_file(archive.with_extension("").with_extension("json"));
                total = total.saturating_sub(size);
            }
        }
    }
}

fn read_meta(path: &Path) -> Option<Meta> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn collect_archives(dir: &Path, entries: &mut Vec<(SystemTime, u64, PathBuf)>) {
    let Ok(children) = fs::read_dir(dir) else { return };
    for child in children.flatten() {
        let path = child.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else { continue };
        if metadata.is_dir() {
            collect_archives(&path, entries);
        } else if metadata.is_file() && path.to_string_lossy().ends_with(".tar.gz") {
            entries.push((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), metadata.len(), path));
        }
    }
}

/// Marks an entry as just used.
fn touch(archive: &Path) {
    if let Ok(file) = File::options().append(true).open(archive) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Which root a cached path is under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Workspace,
    Home,
}

impl Area {
    const fn prefix(self) -> &'static str {
        match self {
            Self::Workspace => "w",
            Self::Home => "h",
        }
    }
}

/// The `path` input's lines, resolved: each a root and a path inside it.
pub fn resolve(lines: &[String], roots: &Roots, workspace_view: &str, container: bool) -> Result<Vec<(Area, PathBuf)>, String> {
    let mut resolved = Vec::new();
    for line in lines {
        if line.starts_with('!') || line.contains(['*', '?', '[']) {
            return Err(format!("`{line}`: glob patterns and exclusions are not supported by rbit's cache yet; list the paths"));
        }
        let (area, relative) = if let Some(rest) = line.strip_prefix("~/").or_else(|| (line == "~").then_some("")) {
            if container {
                return Err(format!("`{line}`: container jobs can cache only paths inside the workspace"));
            }
            if roots.home.is_none() {
                return Err(format!("`{line}`: the runner user has no home directory"));
            }
            (Area::Home, PathBuf::from(rest))
        } else if let Some(rest) = line.strip_prefix(workspace_view).filter(|rest| rest.is_empty() || rest.starts_with(['/', '\\'])) {
            (Area::Workspace, PathBuf::from(rest.trim_start_matches(['/', '\\'])))
        } else if Path::new(line).is_absolute() || line.starts_with('/') {
            let inside = roots.home.as_ref().and_then(|home| Path::new(line).strip_prefix(home).ok().map(Path::to_path_buf));
            match inside {
                Some(rest) if !container => (Area::Home, rest),
                _ => return Err(format!("`{line}` is outside the workspace; cache paths are relative to it")),
            }
        } else {
            (Area::Workspace, PathBuf::from(line))
        };
        if !relative.components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir)) {
            return Err(format!("`{line}` leaves its directory"));
        }
        resolved.push((area, relative));
    }
    Ok(resolved)
}

fn root(area: Area, roots: &Roots) -> Option<&Path> {
    match area {
        Area::Workspace => Some(&roots.workspace),
        Area::Home => roots.home.as_deref(),
    }
}

/// Whether `relative`, or any directory on the way to it under `root`, is a symbolic link.
fn crosses_link(root: &Path, relative: &Path) -> bool {
    let mut path = root.to_path_buf();
    relative.components().any(|component| {
        path.push(component);
        fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    })
}

/// Writes the tar.gz of `resolved` to `out`; its size. Paths that do not exist are
/// skipped; links are stored as links.
pub(super) fn write_archive(out: &Path, resolved: &[(Area, PathBuf)], roots: &Roots) -> io::Result<u64> {
    let file = File::create(out)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    for (area, relative) in resolved {
        let Some(base) = root(*area, roots) else { continue };
        let source = base.join(relative);
        if !source.exists() || crosses_link(base, relative) {
            continue;
        }
        let name = Path::new(area.prefix()).join(relative);
        if source.is_dir() {
            builder.append_dir_all(&name, &source)?;
        } else {
            builder.append_path_with_name(&source, &name)?;
        }
    }
    builder.into_inner()?.finish()?.sync_all()?;
    Ok(fs::metadata(out)?.len())
}

/// Restores an archive. Every entry must name a path inside its root; nothing is written
/// through a symbolic link, and links are only created on Unix.
pub(super) fn extract(archive: &Path, roots: &Roots) -> io::Result<u64> {
    let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
    let mut tar = tar::Archive::new(decoder);
    let mut restored = 0;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut components = path.components();
        let area = match components.next() {
            Some(Component::Normal(first)) if first == "w" => Area::Workspace,
            Some(Component::Normal(first)) if first == "h" => Area::Home,
            _ => continue,
        };
        let relative: PathBuf = components.collect();
        let Some(base) = root(area, roots) else { continue };
        if relative.as_os_str().is_empty() || !relative.components().all(|component| matches!(component, Component::Normal(_))) {
            continue;
        }
        if let Some(parent) = relative.parent()
            && crosses_link(base, parent)
        {
            continue;
        }
        let target = base.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            fs::remove_file(&target)?;
        }
        match entry.header().entry_type() {
            tar::EntryType::Directory => fs::create_dir_all(&target)?,
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                let mut file = File::create(&target)?;
                io::copy(&mut entry, &mut file)?;
                #[cfg(unix)]
                if let Ok(mode) = entry.header().mode() {
                    use std::os::unix::fs::PermissionsExt as _;
                    let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777));
                }
                restored += 1;
            }
            #[cfg(unix)]
            tar::EntryType::Symlink => {
                if let Some(link) = entry.link_name()? {
                    if target.exists() {
                        let _ = fs::remove_file(&target);
                    }
                    std::os::unix::fs::symlink(link, &target)?;
                }
            }
            _ => {}
        }
    }
    Ok(restored)
}

/// The runner user's home directory.
fn home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"].iter().find_map(std::env::var_os).map(PathBuf::from).filter(|home| home.is_absolute())
}

/// A save that `rbit/cache` leaves for after the last step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSave {
    pub key: String,
    pub paths: Vec<String>,
}

pub(super) fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect()
}

fn roots(job: &Job) -> Roots {
    Roots { workspace: job.dirs.workspace.clone(), home: matches!(job.backend, Backend::Host(_)).then(home).flatten() }
}

/// Runs a cache step.
pub async fn cache(job: &mut Job, kind: BuiltinKind, inputs: &Inputs) -> Outcome {
    let key = inputs.get("key").trim().to_owned();
    if key.chars().count() > MAX_KEY_CHARS || key.contains(',') {
        return job.fail(&format!("the cache key must be at most {MAX_KEY_CHARS} characters without commas"));
    }
    let paths = lines(inputs.get("path"));
    if let Err(error) = resolve(&paths, &roots(job), &job.backend.view().workspace, matches!(job.backend, Backend::Container(_))) {
        return job.fail(&error);
    }
    if kind == BuiltinKind::CacheSave {
        return match save(job, &key, &paths).await {
            Ok(()) => Outcome::success(BTreeMap::new()),
            Err(error) => job.fail(&error),
        };
    }
    let restore_keys = lines(inputs.get("restore-keys"));
    let (store, scopes, repository, roots) = (job.cache.clone(), job.spec.cache.read.clone(), job.spec.cache.repository_id, roots(job));
    let found = store.find(repository, &scopes, &key, &restore_keys, &paths);
    let mut outputs = BTreeMap::from([("cache-primary-key".to_owned(), key.clone())]);
    let Some(found) = found else {
        if inputs.flag("fail-on-cache-miss") {
            return job.fail(&format!("no cache entry for `{key}`, and `fail-on-cache-miss` is set"));
        }
        job.reporter.log(&format!("No cache entry for `{key}`"));
        outputs.insert("cache-hit".to_owned(), "false".to_owned());
        if kind == BuiltinKind::Cache {
            job.post_saves.push(PendingSave { key, paths });
        }
        return Outcome::success(outputs);
    };
    job.reporter.log(&format!("Restoring the cache entry `{}` from scope `{}`", found.key, found.scope));
    touch(&found.archive);
    let archive = found.archive.clone();
    match tokio::task::spawn_blocking(move || extract(&archive, &roots)).await {
        Ok(Ok(files)) => job.reporter.log(&format!("Restored {files} file(s)")),
        Ok(Err(error)) => return job.fail(&format!("cannot restore the cache entry: {error}")),
        Err(error) => return job.fail(&format!("cannot restore the cache entry: {error}")),
    }
    outputs.insert("cache-matched-key".to_owned(), found.key.clone());
    let hit = found.key == key;
    outputs.insert("cache-hit".to_owned(), hit.to_string());
    if kind == BuiltinKind::Cache && !hit {
        job.post_saves.push(PendingSave { key, paths });
    }
    Outcome::success(outputs)
}

/// Saves `paths` as `key` in the job's write scope.
pub async fn save(job: &Job, key: &str, paths: &[String]) -> Result<(), String> {
    let (repository, scope) = (job.spec.cache.repository_id, job.spec.cache.write.clone());
    if job.cache.exists(repository, &scope, key) {
        job.reporter.log(&format!("The cache entry `{key}` already exists in scope `{scope}`; it is not saved again"));
        return Ok(());
    }
    let roots = roots(job);
    let resolved = resolve(paths, &roots, &job.backend.view().workspace, matches!(job.backend, Backend::Container(_)))?;
    let (store, key_owned, paths_owned) = (job.cache.clone(), key.to_owned(), paths.to_vec());
    let size = tokio::task::spawn_blocking(move || store.save(repository, &scope, &key_owned, &paths_owned, &resolved, &roots))
        .await
        .map_err(|error| error.to_string())??;
    job.reporter.log(&format!("Saved the cache entry `{key}` ({size} bytes)"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path, max_bytes: u64) -> Store {
        Store { root: dir.join("cache"), max_bytes }
    }

    fn roots(dir: &Path) -> Roots {
        let roots = Roots { workspace: dir.join("ws"), home: Some(dir.join("home")) };
        fs::create_dir_all(&roots.workspace).unwrap();
        fs::create_dir_all(roots.home.as_ref().unwrap()).unwrap();
        roots
    }

    fn save(store: &Store, roots: &Roots, scope: &str, key: &str, paths: &[&str]) -> u64 {
        let paths: Vec<String> = paths.iter().map(|path| (*path).to_owned()).collect();
        let resolved = resolve(&paths, roots, "/__w/app", false).unwrap();
        store.save(5, scope, key, &paths, &resolved, roots).unwrap()
    }

    #[test]
    fn entries_round_trip_and_are_found_by_key_then_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let (store, roots) = (store(dir.path(), 1 << 30), roots(dir.path()));
        fs::create_dir_all(roots.workspace.join("target/debug")).unwrap();
        fs::write(roots.workspace.join("target/debug/app"), "binary").unwrap();
        fs::write(roots.home.as_ref().unwrap().join("registry"), "crates").unwrap();
        save(&store, &roots, "refs/heads/main", "cargo-abc", &["target", "~/registry"]);
        assert!(store.entry(5, "refs/heads/main", "cargo-abc").0.starts_with(dir.path().join("cache/5/refs/heads/main")));

        let paths = vec!["target".to_owned(), "~/registry".to_owned()];
        let scopes = vec!["refs/heads/feature".to_owned(), "refs/heads/main".to_owned()];
        let exact = store.find(5, &scopes, "cargo-abc", &[], &paths).unwrap();
        assert_eq!((exact.key.as_str(), exact.scope.as_str()), ("cargo-abc", "refs/heads/main"));
        let prefix = store.find(5, &scopes, "cargo-def", &["cargo-".to_owned()], &paths).unwrap();
        assert_eq!(prefix.key, "cargo-abc");
        assert!(store.find(5, &scopes, "cargo-abc", &[], &["target".to_owned()]).is_none(), "different paths never match");
        assert!(store.find(6, &scopes, "cargo-abc", &[], &paths).is_none(), "another repository");
        assert!(store.find(5, &["pull/3".to_owned()], "cargo-abc", &["cargo".to_owned()], &paths).is_none(), "another scope");

        fs::remove_dir_all(&roots.workspace).unwrap();
        fs::create_dir_all(&roots.workspace).unwrap();
        fs::remove_file(roots.home.as_ref().unwrap().join("registry")).unwrap();
        assert_eq!(extract(&exact.archive, &roots).unwrap(), 2);
        assert_eq!(fs::read_to_string(roots.workspace.join("target/debug/app")).unwrap(), "binary");
        assert_eq!(fs::read_to_string(roots.home.as_ref().unwrap().join("registry")).unwrap(), "crates");
    }

    #[test]
    fn the_least_recently_used_entries_go_first() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let noise: Vec<u8> = (0..200_000u32).flat_map(|i| i.wrapping_mul(2_654_435_761).to_le_bytes()).collect();
        fs::write(roots.workspace.join("blob"), &noise).unwrap();
        let big = store(dir.path(), u64::MAX);
        let size = save(&big, &roots, "s", "one", &["blob"]);
        let limited = store(dir.path(), size * 2 + size / 2);
        std::thread::sleep(std::time::Duration::from_millis(20));
        save(&limited, &roots, "s", "two", &["blob"]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        // Using `one` makes `two` the least recently used.
        touch(&limited.entry(5, "s", "one").0);
        std::thread::sleep(std::time::Duration::from_millis(20));
        save(&limited, &roots, "s", "three", &["blob"]);
        assert!(limited.exists(5, "s", "one") && limited.exists(5, "s", "three"));
        assert!(!limited.exists(5, "s", "two"));
        assert!(!limited.entry(5, "s", "two").1.exists(), "the entry's metadata goes too");
        let tiny = store(dir.path(), 10);
        let paths = vec!["blob".to_owned()];
        let resolved = resolve(&paths, &roots, "/__w/app", false).unwrap();
        assert!(tiny.save(5, "s", "four", &paths, &resolved, &roots).unwrap_err().contains("more than cache.max_bytes"));
    }

    #[test]
    fn paths_stay_inside_their_roots() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let lines = |list: &[&str]| list.iter().map(|line| (*line).to_owned()).collect::<Vec<_>>();
        let ok = resolve(&lines(&["target", "/__w/app/node_modules", "~/.cargo/registry"]), &roots, "/__w/app", false).unwrap();
        assert_eq!(
            ok,
            [(Area::Workspace, "target".into()), (Area::Workspace, "node_modules".into()), (Area::Home, PathBuf::from(".cargo/registry"))]
        );
        for (line, container, message) in [
            ("../escape", false, "leaves"),
            ("~/.cargo", true, "container jobs"),
            ("**/node_modules", false, "glob"),
            ("!target/tmp", false, "glob"),
            ("/etc", false, "outside the workspace"),
        ] {
            let error = resolve(&lines(&[line]), &roots, "/__w/app", container).unwrap_err();
            assert!(error.contains(message), "{line}: {error}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn restores_never_write_through_links() {
        let dir = tempfile::tempdir().unwrap();
        let (store, roots) = (store(dir.path(), 1 << 30), roots(dir.path()));
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(roots.workspace.join("out")).unwrap();
        fs::write(roots.workspace.join("out/file"), "cached").unwrap();
        std::os::unix::fs::symlink(&outside, roots.workspace.join("link")).unwrap();
        save(&store, &roots, "s", "k", &["out", "link"]);
        // A later job replaces the directory with a link to somewhere else.
        fs::remove_dir_all(roots.workspace.join("out")).unwrap();
        std::os::unix::fs::symlink(&outside, roots.workspace.join("out")).unwrap();
        let archive = store.entry(5, "s", "k").0;
        extract(&archive, &roots).unwrap();
        assert!(!outside.join("file").exists(), "nothing was written through the link");
    }
}
