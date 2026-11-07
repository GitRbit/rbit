//! `uses:` → one of the built-in actions `rbit-runner` carries (§12.2 of
//! `docs/design/actions.md`), or an action the runner fetches ([`resolve_uses`]).
//!
//! A step's `uses:` names a built-in, by major version, or one of the `actions/*` versions it
//! stands in for, so common GitHub workflows run unchanged:
//!
//! | `uses:` | Alias of | `with:` |
//! |---|---|---|
//! | `rbit/checkout@v1` | `actions/checkout@v2` to `@v5` | `repository`, `ref`, `token`, `path`, `fetch-depth`, `fetch-tags`, `submodules`, `lfs`, `clean`, `persist-credentials` |
//! | `rbit/cache@v1` (also `/restore`, `/save`) | `actions/cache@v3` to `@v4` (also `/restore`, `/save`) | `key`, `path`, `restore-keys`, `fail-on-cache-miss` |
//! | `rbit/upload-artifact@v1` | `actions/upload-artifact@v3` to `@v4` | `name`, `path`, `if-no-files-found`, `retention-days`, `overwrite`, `include-hidden-files`, `compression-level` |
//! | `rbit/download-artifact@v1` | `actions/download-artifact@v3` to `@v4` | `name`, `path`, `pattern`, `merge-multiple`, `run-id`, `github-token`, `repository` |
//!
//! The aliases take any later major version too (`actions/checkout@v6`), and a commit SHA
//! (`actions/checkout@<40 hex digits>`), which security-minded workflows pin to.
//!
//! Any other `uses:` is an action the runner fetches and runs (Actions: third-party
//! actions): `owner/repo[/path]@ref` from the runner's action source (GitHub by default),
//! `./path` from the workspace, or `docker://image`. Their `with:` keys are free; the runner
//! checks them against the action's `action.yml`. Reusable workflows and a `uses:` with
//! `${{ }}` in it remain workflow errors. A `with:` key a built-in does not have is an error,
//! and so is a literal value it cannot take (`submodules: maybe`); values with expressions
//! are checked by the runner.

use std::collections::BTreeMap;

use crate::spec::{ActionSource, Builtin, BuiltinKind};

/// One `with:` input of a built-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    pub name: &'static str,
    /// What the runner uses when the step does not set it.
    pub default: Option<&'static str>,
    pub required: bool,
    pub kind: InputKind,
}

/// The literal values an input takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Text,
    /// `true` or `false`.
    Boolean,
    /// A whole number, at least the given one.
    Number(u64),
    /// One of these words.
    Choice(&'static [&'static str]),
}

const fn input(name: &'static str, default: Option<&'static str>, kind: InputKind) -> Input {
    Input { name, default, required: false, kind }
}

const fn required(name: &'static str) -> Input {
    Input { name, default: None, required: true, kind: InputKind::Text }
}

const CHECKOUT: [Input; 10] = [
    // Actions: checkout of another repository. `owner/name` on this instance, and the token
    // to fetch it (and push, with `persist-credentials`) instead of the task's.
    input("repository", None, InputKind::Text),
    input("token", None, InputKind::Text),
    input("ref", None, InputKind::Text),
    input("path", None, InputKind::Text),
    input("fetch-depth", Some("1"), InputKind::Number(0)),
    input("fetch-tags", Some("false"), InputKind::Boolean),
    input("submodules", Some("false"), InputKind::Choice(&["false", "true", "recursive"])),
    input("lfs", Some("false"), InputKind::Boolean),
    input("clean", Some("true"), InputKind::Boolean),
    input("persist-credentials", Some("true"), InputKind::Boolean),
];
const CACHE: [Input; 4] = [
    required("key"),
    required("path"),
    input("restore-keys", None, InputKind::Text),
    input("fail-on-cache-miss", Some("false"), InputKind::Boolean),
];
const CACHE_SAVE: [Input; 2] = [required("key"), required("path")];
const UPLOAD_ARTIFACT: [Input; 7] = [
    input("name", Some("artifact"), InputKind::Text),
    required("path"),
    input("if-no-files-found", Some("warn"), InputKind::Choice(&["warn", "error", "ignore"])),
    input("retention-days", None, InputKind::Number(1)),
    input("overwrite", Some("false"), InputKind::Boolean),
    // Actions: glob paths of the cache and artifacts.
    input("include-hidden-files", Some("false"), InputKind::Boolean),
    input("compression-level", Some("6"), InputKind::Choice(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"])),
];
const DOWNLOAD_ARTIFACT: [Input; 7] = [
    input("name", None, InputKind::Text),
    input("path", None, InputKind::Text),
    input("pattern", None, InputKind::Text),
    input("merge-multiple", Some("false"), InputKind::Boolean),
    // Actions: workflow_run. Another run of this repository; the token GitHub needs for it
    // is accepted and not used, as the runner's own credential reads the run.
    input("run-id", None, InputKind::Number(1)),
    input("github-token", None, InputKind::Text),
    input("repository", None, InputKind::Text),
];

/// The `with:` inputs of a built-in.
pub fn inputs(kind: BuiltinKind) -> &'static [Input] {
    match kind {
        BuiltinKind::Checkout => &CHECKOUT,
        BuiltinKind::Cache | BuiltinKind::CacheRestore => &CACHE,
        BuiltinKind::CacheSave => &CACHE_SAVE,
        BuiltinKind::UploadArtifact => &UPLOAD_ARTIFACT,
        BuiltinKind::DownloadArtifact => &DOWNLOAD_ARTIFACT,
    }
}

/// The built-in's own name, `rbit/checkout`.
pub const fn name(kind: BuiltinKind) -> &'static str {
    match kind {
        BuiltinKind::Checkout => "rbit/checkout",
        BuiltinKind::Cache => "rbit/cache",
        BuiltinKind::CacheRestore => "rbit/cache/restore",
        BuiltinKind::CacheSave => "rbit/cache/save",
        BuiltinKind::UploadArtifact => "rbit/upload-artifact",
        BuiltinKind::DownloadArtifact => "rbit/download-artifact",
    }
}

/// Every action name rbit resolves to a built-in, with the major versions it accepts (the
/// `actions/*` aliases: any from the first one rbit stands in for) and the built-in version
/// they resolve to.
const ACTIONS: [(&str, BuiltinKind, std::ops::RangeInclusive<u32>, u32); 12] = [
    ("rbit/checkout", BuiltinKind::Checkout, 1..=1, 1),
    ("actions/checkout", BuiltinKind::Checkout, 2..=u32::MAX, 1),
    ("rbit/cache", BuiltinKind::Cache, 1..=1, 1),
    ("rbit/cache/restore", BuiltinKind::CacheRestore, 1..=1, 1),
    ("rbit/cache/save", BuiltinKind::CacheSave, 1..=1, 1),
    ("actions/cache", BuiltinKind::Cache, 3..=u32::MAX, 1),
    ("actions/cache/restore", BuiltinKind::CacheRestore, 3..=u32::MAX, 1),
    ("actions/cache/save", BuiltinKind::CacheSave, 3..=u32::MAX, 1),
    ("rbit/upload-artifact", BuiltinKind::UploadArtifact, 1..=1, 1),
    ("actions/upload-artifact", BuiltinKind::UploadArtifact, 3..=u32::MAX, 1),
    ("rbit/download-artifact", BuiltinKind::DownloadArtifact, 1..=1, 1),
    ("actions/download-artifact", BuiltinKind::DownloadArtifact, 3..=u32::MAX, 1),
];

// Actions: third-party actions.
/// What a step's `uses:` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uses {
    /// A built-in and its version.
    Builtin(BuiltinKind, u32),
    /// An action the runner fetches and runs.
    Action(ActionSource),
}

/// Longest `uses:` rbit accepts.
pub const MAX_USES_BYTES: usize = 512;

/// Resolves a step's `uses:`: a built-in, else a repository's, the workspace's or a
/// container image's action. Reusable workflows and expressions are refused.
pub fn resolve_uses(uses: &str) -> Result<Uses, String> {
    if uses.contains("${{") {
        return Err("`uses` cannot contain expressions: name an action and its version".to_owned());
    }
    if uses.len() > MAX_USES_BYTES || uses.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("`{}` is not an action reference", uses.escape_debug()));
    }
    if let Some(image) = uses.strip_prefix("docker://") {
        let valid = !image.is_empty()
            && !image.starts_with(['-', '.', '/', ':', '@'])
            && image.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/' | b':' | b'@'));
        if !valid {
            return Err(format!("`{uses}` needs an image, as in `docker://alpine:3.20`"));
        }
        return Ok(Uses::Action(ActionSource::Docker { image: image.to_owned() }));
    }
    if let Some(path) = uses.strip_prefix("./") {
        let path = path.trim_end_matches('/');
        if !path.is_empty() && !path.split('/').all(|part| valid_part(part) || part == ".") {
            return Err(format!("`{uses}`: a local action is a directory inside the workspace, as in `./.github/actions/build`"));
        }
        return Ok(Uses::Action(ActionSource::Local { path: path.to_owned() }));
    }
    if uses.starts_with("../") || uses.starts_with('/') || uses.starts_with('.') {
        return Err(format!("`{uses}`: a local action is a directory inside the workspace, as in `./.github/actions/build`"));
    }
    let (action, reference) = uses.split_once('@').ok_or_else(|| format!("`{uses}` needs a version, as in `{uses}@v1`"))?;
    let workflow_file = std::path::Path::new(action)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml"));
    if action.contains("/.github/workflows/") || workflow_file {
        return Err(format!("`{uses}` is a workflow: call it from a job (`jobs.<id>.uses`), not from a step"));
    }
    if ACTIONS.iter().any(|(name, ..)| name.eq_ignore_ascii_case(action)) {
        return resolve(uses).map(|(kind, version)| Uses::Builtin(kind, version));
    }
    let mut parts = action.split('/');
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
        return Err(format!("`{uses}`: name an action as `owner/repo@ref`, `owner/repo/path@ref`, `./path` or `docker://image`"));
    };
    let path: Vec<&str> = parts.collect();
    if !valid_part(owner) || !valid_part(repo) || !path.iter().all(|part| valid_part(part)) {
        return Err(format!("`{uses}`: name an action as `owner/repo@ref`, `owner/repo/path@ref`, `./path` or `docker://image`"));
    }
    if reference.is_empty()
        || reference.starts_with('-')
        || reference.contains("..")
        || reference.contains(['\\', ':', '~', '^', '?', '*', '['])
    {
        return Err(format!("`{uses}`: `{reference}` is not a branch, tag or commit"));
    }
    Ok(Uses::Action(ActionSource::Repository {
        owner: owner.to_owned(),
        repo: repo.to_owned(),
        path: path.join("/"),
        reference: reference.to_owned(),
    }))
}

/// One part of an action's name or path: letters, digits, `-`, `_` and `.`, but not `.` or
/// `..` alone and not starting with `-`.
fn valid_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && !part.starts_with('-')
        && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// A full commit id, as actions are pinned by.
fn is_commit(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

const INSTEAD: &str = "use a `container` image that has the tool, or install it in a `run:` step";

/// The built-in `uses` names and its version: `(kind, built-in major version)`.
pub fn resolve(uses: &str) -> Result<(BuiltinKind, u32), String> {
    if uses.contains("${{") {
        return Err("`uses` cannot contain expressions: name a built-in action and its version".to_owned());
    }
    if uses.starts_with("docker://") {
        return Err(format!("`{uses}`: `docker://` steps are not supported by rbit; give the job a `container` instead"));
    }
    if uses.starts_with("./") || uses.starts_with("../") || uses.starts_with('/') {
        return Err(format!("`{uses}`: local actions are not supported by rbit; {INSTEAD}"));
    }
    let (action, reference) = uses.split_once('@').ok_or_else(|| format!("`{uses}` needs a version, as in `{uses}@v1`"))?;
    let workflow_file = std::path::Path::new(action)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml"));
    if action.contains("/.github/workflows/") || workflow_file {
        return Err(format!("`{uses}` is a workflow: call it from a job (`jobs.<id>.uses`), not from a step"));
    }
    let known = ACTIONS.iter().filter(|(name, ..)| name.eq_ignore_ascii_case(action)).collect::<Vec<_>>();
    // Actions: third-party actions. A pinned commit of an action rbit stands in for runs the
    // built-in, whichever version that commit was.
    if let Some((_, kind, _, version)) = known.first().filter(|_| is_commit(reference)) {
        return Ok((*kind, *version));
    }
    if known.is_empty() {
        let builtins = ["rbit/checkout", "rbit/cache", "rbit/upload-artifact", "rbit/download-artifact"].join(", ");
        return Err(format!("`{action}` is not one of rbit's built-in actions ({builtins}); {INSTEAD}"));
    }
    let major = major_version(reference).ok_or_else(|| {
        format!("`{uses}`: built-in actions are chosen by major version or commit, as in `{action}@v{}`", known[0].2.start().max(&1))
    })?;
    known.iter().find(|(_, _, versions, _)| versions.contains(&major)).map(|(_, kind, _, version)| (*kind, *version)).ok_or_else(|| {
        let versions = &known[0].2;
        let range = if versions.start() == versions.end() {
            format!("@v{}", versions.start())
        } else if *versions.end() == u32::MAX {
            format!("@v{} and later", versions.start())
        } else {
            format!("@v{} to @v{}", versions.start(), versions.end())
        };
        format!("`{uses}`: rbit supports `{action}` {range}")
    })
}

/// `v4`, `v4.1` or `v4.1.2` → 4.
fn major_version(reference: &str) -> Option<u32> {
    let version = reference.strip_prefix('v')?;
    let mut parts = version.split('.');
    let major = parts.next()?;
    if parts.count() > 2 || !version.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    major.parse().ok()
}

/// Checks one `with:` input of a built-in: the key exists, and a literal value is one the
/// input takes.
pub fn check_input(kind: BuiltinKind, uses: &str, key: &str, value: &str) -> Result<(), String> {
    let inputs = inputs(kind);
    let Some(input) = inputs.iter().find(|input| input.name == key) else {
        let names: Vec<&str> = inputs.iter().map(|input| input.name).collect();
        return Err(format!("`{uses}` has no input `{key}` in rbit; it takes {}", names.join(", ")));
    };
    if value.contains("${{") {
        return Ok(());
    }
    let valid = match input.kind {
        InputKind::Text => true,
        InputKind::Boolean => matches!(value, "true" | "false"),
        InputKind::Number(least) => value.parse::<u64>().is_ok_and(|number| number >= least),
        InputKind::Choice(words) => words.contains(&value),
    };
    if valid {
        return Ok(());
    }
    let expected = match input.kind {
        InputKind::Text => String::new(),
        InputKind::Boolean => "true or false".to_owned(),
        InputKind::Number(least) => format!("a whole number of at least {least}"),
        InputKind::Choice(words) => words.join(", "),
    };
    Err(format!("`{uses}`: `{key}` must be {expected}"))
}

/// The inputs a built-in requires that `with` lacks, as an error.
pub fn check_required(kind: BuiltinKind, uses: &str, with: &BTreeMap<String, String>) -> Result<(), String> {
    match inputs(kind).iter().find(|input| input.required && with.get(input.name).is_none_or(String::is_empty)) {
        Some(input) => Err(format!("`{uses}` needs `with.{}`", input.name)),
        None => Ok(()),
    }
}

/// `uses` and its `with:` as a [`Builtin`], checked.
pub fn builtin(uses: &str, with: BTreeMap<String, String>) -> Result<Builtin, String> {
    let (kind, version) = resolve(uses)?;
    for (key, value) in &with {
        check_input(kind, uses, key, value)?;
    }
    check_required(kind, uses, &with)?;
    Ok(Builtin { uses: uses.to_owned(), kind, version, with })
}

#[cfg(test)]
mod tests;
