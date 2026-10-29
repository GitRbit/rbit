//! `rbit/cache` (restore now, save after the job), `rbit/cache/restore` and
//! `rbit/cache/save`, and their `actions/cache` aliases (§12.5.6).
//!
//! When the server keeps caches for all its runners (it says so when the runner declares
//! itself), entries go to the server and any runner of the repository restores them; the
//! server picks the scopes from the task's run, matches the version [`version`] derives
//! from the paths and the platform, and evicts. Otherwise the cache is local to the
//! runner, and what follows describes it: entries live at
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
//! Paths are listed one per line, as paths or glob patterns with `!` exclusions, the
//! syntax of GitHub's `@actions/glob` ([`rbit_workflow::glob`]). They are relative to the
//! workspace or, on the host backend, start with `~/` for the runner user's home; container
//! jobs can cache only what is inside the mounted workspace. Archives store symbolic links
//! as links, never what they point to (and pipes and device nodes as such, never their
//! content); selecting never walks into a link, and a restore never writes through a
//! symbolic link or into anything but a new file: a job cannot use the cache to write
//! outside the directories it names.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use rbit_workflow::{glob::GlobSet, spec::BuiltinKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{Inputs, Outcome};
use crate::{backend::Backend, contexts::hex, job::Job, untrusted::crosses_link};

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

    /// Whether `key` is already saved in `scope` with these `paths`. An entry of the key with
    /// other paths never matches [`Self::find`], so it is replaced rather than kept.
    pub fn exists(&self, repository: i64, scope: &str, key: &str, paths: &[String]) -> bool {
        let (archive, meta) = self.entry(repository, scope, key);
        archive.is_file() && read_meta(&meta).is_some_and(|meta| meta.key == key && meta.paths == paths)
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

/// One line of a `path` input, resolved: the root it is under, its pattern relative to that
/// root (`/`-separated; `**` for the root itself), and whether it excludes (`!`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathLine {
    pub area: Area,
    pub pattern: String,
    pub exclude: bool,
}

/// The `path` input's lines, resolved and checked; nothing is read yet ([`select`] walks).
/// A line is a path or a glob pattern (`rbit_workflow::glob`), relative to the workspace,
/// below the job's view of it, or on the host backend `~/…` or an absolute path in the
/// runner user's home.
pub fn resolve(lines: &[String], roots: &Roots, workspace_view: &str, container: bool) -> Result<Vec<PathLine>, String> {
    let mut resolved = Vec::new();
    for line in lines {
        let (exclude, body) = match line.strip_prefix('!') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, line.as_str()),
        };
        let (area, relative) = if let Some(rest) = body.strip_prefix("~/").or_else(|| (body == "~").then_some("")) {
            if container {
                return Err(format!("`{line}`: container jobs can cache only paths inside the workspace"));
            }
            if roots.home.is_none() {
                return Err(format!("`{line}`: the runner user has no home directory"));
            }
            (Area::Home, rest.to_owned())
        } else if let Some(rest) = body.strip_prefix(workspace_view).filter(|rest| rest.is_empty() || rest.starts_with(['/', '\\'])) {
            (Area::Workspace, rest.trim_start_matches(['/', '\\']).to_owned())
        } else if Path::new(body).is_absolute() || body.starts_with('/') {
            let inside = roots
                .home
                .as_ref()
                .and_then(|home| Path::new(body).strip_prefix(home).ok().map(|rest| rest.to_string_lossy().into_owned()));
            match inside {
                Some(rest) if !container => (Area::Home, rest),
                _ => return Err(format!("`{line}` is outside the workspace; paths are relative to it")),
            }
        } else {
            (Area::Workspace, body.to_owned())
        };
        let relative = relative.replace('\\', "/");
        let pattern = if relative.split('/').all(|part| part.is_empty() || part == ".") { "**".to_owned() } else { relative };
        GlobSet::compile([pattern.as_str()], false).map_err(|error| format!("`{line}`: {error}"))?;
        resolved.push(PathLine { area, pattern, exclude });
    }
    Ok(resolved)
}

/// What [`select`] found: files (and links) below their roots, and the directories or
/// files it searched, each pattern's literal start (an artifact's root is their least
/// common ancestor).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub files: Vec<(Area, PathBuf)>,
    pub search_paths: Vec<(Area, String)>,
}

/// The most files one `path` input selects.
pub const MAX_SELECTED_FILES: usize = 500_000;

/// The files `lines` select, GitHub's way: the last matching line decides, a directory brings
/// what is in it, `!` excludes. Nothing is reached through a symbolic link: a link is a file
/// of its own and never walked into. Names starting with a dot, and what is in such
/// directories, are left out unless `include_hidden` (as `actions/upload-artifact` does;
/// the cache keeps them).
pub fn select(lines: &[PathLine], roots: &Roots, include_hidden: bool, case_insensitive: bool) -> Result<Selection, String> {
    let mut selection = Selection::default();
    for area in [Area::Workspace, Area::Home] {
        let patterns: Vec<String> = lines
            .iter()
            .filter(|line| line.area == area)
            .map(|line| if line.exclude { format!("!{}", line.pattern) } else { line.pattern.clone() })
            .collect();
        let globs = GlobSet::compile(patterns.iter().map(String::as_str), case_insensitive).map_err(|error| error.to_string())?;
        if globs.is_empty() {
            continue;
        }
        let Some(base) = root(area, roots) else { continue };
        let mut found = std::collections::BTreeSet::new();
        for search in globs.search_paths() {
            selection.search_paths.push((area, search.clone()));
            let relative = PathBuf::from(&search);
            if !search.is_empty() && crosses_link(base, &relative) {
                continue;
            }
            let hidden = |name: &str| !include_hidden && name.starts_with('.');
            if relative.file_name().is_some_and(|name| hidden(&name.to_string_lossy())) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(base.join(&relative)) else { continue };
            if metadata.is_dir() {
                walk(base, &relative, &search, &globs, &hidden, &mut found)?;
            } else if globs.matches(&search) {
                found.insert(relative);
            }
        }
        selection.files.extend(found.into_iter().map(|path| (area, path)));
        if selection.files.len() > MAX_SELECTED_FILES {
            return Err(format!("the paths select more than {MAX_SELECTED_FILES} files"));
        }
    }
    Ok(selection)
}

fn walk(
    base: &Path,
    dir: &Path,
    name: &str,
    globs: &GlobSet,
    hidden: &dyn Fn(&str) -> bool,
    found: &mut std::collections::BTreeSet<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(base.join(dir)).map_err(|error| format!("cannot list {}: {error}", base.join(dir).display()))?;
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if hidden(&file_name) {
            continue;
        }
        let relative = dir.join(entry.file_name());
        let text = if name.is_empty() { file_name } else { format!("{name}/{file_name}") };
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if globs.may_contain(&text) {
                walk(base, &relative, &text, globs, hidden, found)?;
            }
        } else if globs.matches(&text) {
            if found.len() >= MAX_SELECTED_FILES {
                return Err(format!("the paths select more than {MAX_SELECTED_FILES} files"));
            }
            found.insert(relative);
        }
    }
    Ok(())
}

fn root(area: Area, roots: &Roots) -> Option<&Path> {
    match area {
        Area::Workspace => Some(&roots.workspace),
        Area::Home => roots.home.as_deref(),
    }
}

/// Writes the tar.gz of `resolved` to `out`; its size. Paths that do not exist are
/// skipped; links are stored as links.
pub(super) fn write_archive(out: &Path, resolved: &[(Area, PathBuf)], roots: &Roots) -> io::Result<u64> {
    write_archive_at(out, resolved, roots, 1)
}

/// [`write_archive`] at gzip `level` (0 to 9).
pub(super) fn write_archive_at(out: &Path, resolved: &[(Area, PathBuf)], roots: &Roots, level: u32) -> io::Result<u64> {
    let file = File::create(out)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::new(level.min(9)));
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
                // Whatever a job left in the way (a link, a pipe, a device node) is replaced,
                // never opened: writing into it would block, or reach the host.
                let mut file = crate::untrusted::create(&target, false)?;
                io::copy(&mut entry, &mut file)?;
                #[cfg(unix)]
                if let Ok(mode) = entry.header().mode() {
                    use std::os::unix::fs::PermissionsExt as _;
                    let _ = file.set_permissions(fs::Permissions::from_mode(mode & 0o777));
                }
                restored += 1;
            }
            #[cfg(unix)]
            tar::EntryType::Symlink => {
                if let Some(link) = entry.link_name()? {
                    if fs::symlink_metadata(&target).is_ok_and(|metadata| !metadata.is_dir()) {
                        fs::remove_file(&target)?;
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
    /// Whether it replaces an entry for `key` that could not be restored.
    pub replace: bool,
}

pub(super) fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect()
}

fn roots(job: &Job) -> Roots {
    Roots { workspace: job.dirs.workspace.clone(), home: matches!(job.backend, Backend::Host(_)).then(home).flatten() }
}

// Actions: shared cache.
/// The server, when it keeps caches for all runners.
fn shared(job: &Job) -> Option<crate::client::Client> {
    job.server.clone().filter(crate::client::Client::shared_cache)
}

/// What a restore from the server's cache must match besides the key: the paths, and where
/// the archive was made (a Windows archive, or one with home paths, unpacks wrongly
/// elsewhere).
pub fn version(paths: &[String], windows: bool, container: bool) -> String {
    let platform = match (windows, container) {
        (true, _) => "windows",
        (false, true) => "posix-container",
        (false, false) => "posix",
    };
    hex(&Sha256::digest(format!("{}\n{platform}", paths.join("\n")).as_bytes()))
}

fn job_version(job: &Job, paths: &[String]) -> String {
    version(paths, job.backend.windows(), matches!(job.backend, Backend::Container(_)))
}

/// The entry the server's cache has for `key`, downloaded into the job's private
/// directory and checked against its digest.
async fn fetch(
    job: &Job,
    server: &crate::client::Client,
    key: &str,
    restore_keys: &[String],
    paths: &[String],
) -> Result<Option<Found>, String> {
    let request = rbit_workflow::protocol::CacheLookupRequest {
        key: key.to_owned(),
        restore_keys: restore_keys.to_vec(),
        version: job_version(job, paths),
    };
    let Some(entry) = server.cache_lookup(job.spec.task_id, &request).await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let private = job.private_dir().map_err(|error| error.to_string())?;
    let archive = private.join(format!("cache-{}.tar.gz", crate::client::idempotency_key()));
    let sent = server.download_cache(job.spec.task_id, entry.id, &archive).await.map_err(|error| error.to_string())?;
    let checked = archive.clone();
    let actual = tokio::task::spawn_blocking(move || super::artifact::file_digest(&checked)).await.map_err(|error| error.to_string());
    if !matches!(&actual, Ok(Ok(digest)) if *digest == entry.sha256 && (sent.is_empty() || sent == entry.sha256)) {
        let _ = fs::remove_file(&archive);
        return Err("the downloaded entry does not match its digest".to_owned());
    }
    Ok(Some(Found { archive, key: entry.key, scope: entry.scope }))
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
        return match save(job, &key, &paths, false).await {
            Ok(()) => Outcome::success(BTreeMap::new()),
            Err(error) => job.fail(&error),
        };
    }
    let restore_keys = lines(inputs.get("restore-keys"));
    let (store, scopes, repository, roots) = (job.cache.clone(), job.spec.cache.read.clone(), job.spec.cache.repository_id, roots(job));
    let server = shared(job);
    let found = match &server {
        None => store.find(repository, &scopes, &key, &restore_keys, &paths),
        // As with GitHub's cache service, one that cannot be reached is a miss.
        Some(server) => fetch(job, server, &key, &restore_keys, &paths).await.unwrap_or_else(|error| {
            job.reporter.log(&format!("##[warning]cannot restore from the server's cache, so it is a miss: {error}"));
            None
        }),
    };
    let mut outputs = BTreeMap::from([("cache-primary-key".to_owned(), key.clone())]);
    let miss = |job: &mut Job, mut outputs: BTreeMap<String, String>, replace: bool| {
        if inputs.flag("fail-on-cache-miss") {
            return job.fail(&format!("no cache entry for `{key}`, and `fail-on-cache-miss` is set"));
        }
        outputs.insert("cache-hit".to_owned(), "false".to_owned());
        if kind == BuiltinKind::Cache {
            job.post_saves.push(PendingSave { key: key.clone(), paths: paths.clone(), replace });
        }
        Outcome::success(outputs)
    };
    let Some(found) = found else {
        job.reporter.log(&format!("No cache entry for `{key}`"));
        return miss(job, outputs, false);
    };
    job.reporter.log(&format!("Restoring the cache entry `{}` from scope `{}`", found.key, found.scope));
    let archive = found.archive.clone();
    let downloaded = server.is_some();
    // As with GitHub's cache, an entry that cannot be restored (another job's save evicted
    // it meanwhile, it is damaged) is a miss, not a failed job, and the job's save replaces
    // it.
    let error = match tokio::task::spawn_blocking(move || extract(&archive, &roots)).await {
        Ok(Ok(files)) => {
            job.reporter.log(&format!("Restored {files} file(s)"));
            if !downloaded {
                touch(&found.archive);
            }
            None
        }
        Ok(Err(error)) => Some(error.to_string()),
        Err(error) => Some(error.to_string()),
    };
    if downloaded {
        let _ = fs::remove_file(&found.archive);
    }
    if let Some(error) = error {
        job.reporter.log(&format!("##[warning]cannot restore the cache entry `{}`, so it is a miss: {error}", found.key));
        return miss(job, outputs, true);
    }
    outputs.insert("cache-matched-key".to_owned(), found.key.clone());
    let hit = found.key == key;
    outputs.insert("cache-hit".to_owned(), hit.to_string());
    if kind == BuiltinKind::Cache && !hit {
        job.post_saves.push(PendingSave { key, paths, replace: false });
    }
    Outcome::success(outputs)
}

/// Saves `paths` as `key` in the job's write scope; an entry already there stays unless
/// `replace`.
pub async fn save(job: &Job, key: &str, paths: &[String], replace: bool) -> Result<(), String> {
    if let Some(server) = shared(job) {
        return save_shared(job, &server, key, paths, replace).await;
    }
    let (repository, scope) = (job.spec.cache.repository_id, job.spec.cache.write.clone());
    if !replace && job.cache.exists(repository, &scope, key, paths) {
        job.reporter.log(&format!("The cache entry `{key}` already exists in scope `{scope}`; it is not saved again"));
        return Ok(());
    }
    let roots = roots(job);
    let lines = resolve(paths, &roots, &job.backend.view().workspace, matches!(job.backend, Backend::Container(_)))?;
    let (store, key_owned, paths_owned, windows) = (job.cache.clone(), key.to_owned(), paths.to_vec(), job.backend.windows());
    let size = tokio::task::spawn_blocking(move || {
        let selected = select(&lines, &roots, true, windows)?;
        store.save(repository, &scope, &key_owned, &paths_owned, &selected.files, &roots)
    })
    .await
    .map_err(|error| error.to_string())??;
    job.reporter.log(&format!("Saved the cache entry `{key}` ({size} bytes)"));
    Ok(())
}

// Actions: shared cache.
/// [`save`] to the server's cache.
async fn save_shared(job: &Job, server: &crate::client::Client, key: &str, paths: &[String], replace: bool) -> Result<(), String> {
    let version = job_version(job, paths);
    let write = &job.spec.cache.write;
    if !replace {
        // The write scope is the first one read, so an exact hit there means it is saved.
        let request =
            rbit_workflow::protocol::CacheLookupRequest { key: key.to_owned(), restore_keys: Vec::new(), version: version.clone() };
        let found =
            server.cache_lookup(job.spec.task_id, &request).await.map_err(|error| format!("cannot reach the server's cache: {error}"))?;
        if found.is_some_and(|entry| entry.key == key && entry.scope == *write) {
            job.reporter.log(&format!("The cache entry `{key}` already exists in scope `{write}`; it is not saved again"));
            return Ok(());
        }
    }
    let roots = roots(job);
    let lines = resolve(paths, &roots, &job.backend.view().workspace, matches!(job.backend, Backend::Container(_)))?;
    let private = job.private_dir().map_err(|error| error.to_string())?;
    let archive = private.join(format!("cache-{}.tar.gz", crate::client::idempotency_key()));
    let (path, windows, max_bytes) = (archive.clone(), job.backend.windows(), job.cache.max_bytes);
    let written = tokio::task::spawn_blocking(move || {
        let selected = select(&lines, &roots, true, windows)?;
        let size = write_archive(&path, &selected.files, &roots).map_err(|error| format!("cannot write the cache entry: {error}"))?;
        if size > max_bytes {
            return Err(format!("the cache entry would be {size} bytes, more than cache.max_bytes ({max_bytes})"));
        }
        let digest = super::artifact::file_digest(&path).map_err(|error| error.to_string())?;
        Ok((size, digest))
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|written| written);
    let saved = match written {
        Ok((size, digest)) => server
            .save_cache(job.spec.task_id, key, &version, replace, &digest, &archive)
            .await
            .map(|entry| (size, entry))
            .map_err(|error| format!("cannot save the cache entry `{key}`: {error}")),
        Err(error) => Err(error),
    };
    let _ = fs::remove_file(&archive);
    match saved? {
        (size, Some(entry)) => job.reporter.log(&format!("Saved the cache entry `{key}` ({size} bytes) in scope `{}`", entry.scope)),
        (_, None) => job.reporter.log(&format!("The cache entry `{key}` was saved meanwhile; it is not saved again")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_tell_paths_and_platforms_apart() {
        let paths = vec!["target".to_owned(), "~/.cargo".to_owned()];
        let linux = version(&paths, false, false);
        assert_eq!(linux.len(), 64);
        assert_eq!(linux, version(&paths, false, false));
        assert_ne!(linux, version(&paths, true, false));
        assert_ne!(linux, version(&paths, false, true));
        assert_ne!(linux, version(&paths[..1], false, false));
    }

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
        let lines = resolve(&paths, roots, "/__w/app", false).unwrap();
        let selected = select(&lines, roots, true, false).unwrap();
        store.save(5, scope, key, &paths, &selected.files, roots).unwrap()
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
        let blob = ["blob".to_owned()];
        assert!(limited.exists(5, "s", "one", &blob) && limited.exists(5, "s", "three", &blob));
        assert!(!limited.exists(5, "s", "two", &blob));
        assert!(!limited.exists(5, "s", "one", &["other".to_owned()]), "an entry with other paths is replaced, not kept");
        assert!(!limited.entry(5, "s", "two").1.exists(), "the entry's metadata goes too");
        let tiny = store(dir.path(), 10);
        let paths = vec!["blob".to_owned()];
        let selected = select(&resolve(&paths, &roots, "/__w/app", false).unwrap(), &roots, true, false).unwrap();
        assert!(tiny.save(5, "s", "four", &paths, &selected.files, &roots).unwrap_err().contains("more than cache.max_bytes"));
    }

    #[test]
    fn paths_stay_inside_their_roots() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let lines = |list: &[&str]| list.iter().map(|line| (*line).to_owned()).collect::<Vec<_>>();
        let ok = resolve(
            &lines(&["target", "/__w/app/node_modules", "~/.cargo/registry", "!target/tmp", "**/*.rlib", "."]),
            &roots,
            "/__w/app",
            false,
        )
        .unwrap();
        let line = |area, pattern: &str, exclude| PathLine { area, pattern: pattern.to_owned(), exclude };
        assert_eq!(
            ok,
            [
                line(Area::Workspace, "target", false),
                line(Area::Workspace, "node_modules", false),
                line(Area::Home, ".cargo/registry", false),
                line(Area::Workspace, "target/tmp", true),
                line(Area::Workspace, "**/*.rlib", false),
                line(Area::Workspace, "**", false),
            ]
        );
        for (line, container, message) in [
            ("../escape", false, "leave"),
            ("dist/../../x", false, "leave"),
            ("~/.cargo", true, "container jobs"),
            ("dist/[abc", false, "not closed"),
            ("/etc", false, "outside the workspace"),
        ] {
            let error = resolve(&lines(&[line]), &roots, "/__w/app", container).unwrap_err();
            assert!(error.contains(message), "{line}: {error}");
        }
    }

    #[test]
    fn selections_follow_githubs_globs() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let ws = &roots.workspace;
        for file in [
            "dist/a.zip",
            "dist/a.zip.sha256",
            "dist/b.txt",
            "dist/sub/c.zip",
            "dist/sub/skip.zip",
            "dist/.env",
            ".git/config",
            "docs/readme.md",
        ] {
            let path = ws.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, file).unwrap();
        }
        let pick = |list: &[&str], hidden: bool| {
            let lines: Vec<String> = list.iter().map(|line| (*line).to_owned()).collect();
            let selected = select(&resolve(&lines, &roots, "/__w/app", false).unwrap(), &roots, hidden, false).unwrap();
            let files: Vec<String> = selected.files.iter().map(|(_, path)| path.to_string_lossy().replace('\\', "/")).collect();
            let search: Vec<String> = selected.search_paths.into_iter().map(|(_, path)| path).collect();
            (files, search)
        };
        assert_eq!(pick(&["dist/*.zip*"], false), (vec!["dist/a.zip".into(), "dist/a.zip.sha256".into()], vec!["dist".into()]));
        let (files, search) = pick(&["dist/**/*.zip", "!dist/sub/skip.zip", "docs/readme.md"], false);
        assert_eq!(files, ["dist/a.zip", "dist/sub/c.zip", "docs/readme.md"]);
        assert_eq!(search, ["dist", "docs/readme.md"]);
        assert_eq!(
            pick(&["."], false).0,
            ["dist/a.zip", "dist/a.zip.sha256", "dist/b.txt", "dist/sub/c.zip", "dist/sub/skip.zip", "docs/readme.md"],
            "no hidden files"
        );
        let (all, _) = pick(&["."], true);
        assert!(all.contains(&".git/config".to_owned()) && all.contains(&"dist/.env".to_owned()), "{all:?}");
        assert_eq!(pick(&["dist", "!dist/sub"], false).0, ["dist/a.zip", "dist/a.zip.sha256", "dist/b.txt"]);
        assert_eq!(pick(&["nothing/**"], false).0, Vec::<String>::new());
        assert_eq!(pick(&[".git"], false).0, Vec::<String>::new(), "a hidden search path is hidden too, as on GitHub");
    }

    #[cfg(unix)]
    #[test]
    fn selections_never_walk_into_links() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret"), "x").unwrap();
        fs::create_dir_all(roots.workspace.join("dist")).unwrap();
        std::os::unix::fs::symlink(&outside, roots.workspace.join("dist/link")).unwrap();
        std::os::unix::fs::symlink(&outside, roots.workspace.join("via")).unwrap();
        let lines = resolve(&["dist/**".to_owned(), "via/**".to_owned()], &roots, "/__w/app", false).unwrap();
        let selected = select(&lines, &roots, true, false).unwrap();
        let files: Vec<_> = selected.files.iter().map(|(_, path)| path.clone()).collect();
        assert_eq!(files, [PathBuf::from("dist/link")], "the link itself, never what is behind it");
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

    #[cfg(unix)]
    #[test]
    fn restores_replace_pipes_and_links_instead_of_opening_them() {
        let dir = tempfile::tempdir().unwrap();
        let (store, roots) = (store(dir.path(), 1 << 30), roots(dir.path()));
        let outside = dir.path().join("credential");
        fs::write(&outside, "secret").unwrap();
        fs::write(roots.workspace.join("piped"), "cached").unwrap();
        fs::write(roots.workspace.join("linked"), "cached").unwrap();
        save(&store, &roots, "s", "k", &["piped", "linked"]);
        // A later job leaves a pipe and a link where the files go.
        let Some(mkfifo) = crate::process::find_program("mkfifo") else { return };
        fs::remove_file(roots.workspace.join("piped")).unwrap();
        assert!(std::process::Command::new(mkfifo).arg(roots.workspace.join("piped")).status().unwrap().success());
        fs::remove_file(roots.workspace.join("linked")).unwrap();
        std::os::unix::fs::symlink(&outside, roots.workspace.join("linked")).unwrap();
        let archive = store.entry(5, "s", "k").0;
        let (done, wait) = std::sync::mpsc::channel();
        let restore_roots = roots.clone();
        std::thread::spawn(move || done.send(extract(&archive, &restore_roots).map_err(|error| error.to_string())));
        let restored = wait.recv_timeout(std::time::Duration::from_secs(10)).expect("a pipe in the way does not block the restore");
        assert_eq!(restored.unwrap(), 2);
        for name in ["piped", "linked"] {
            let metadata = fs::symlink_metadata(roots.workspace.join(name)).unwrap();
            assert!(metadata.is_file(), "{name} is a new regular file");
            assert_eq!(fs::read_to_string(roots.workspace.join(name)).unwrap(), "cached");
        }
        assert_eq!(fs::read_to_string(&outside).unwrap(), "secret", "the link's target is untouched");
    }
}
