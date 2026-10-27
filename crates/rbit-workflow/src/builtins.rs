//! `uses:` → one of the built-in actions `rbit-runner` carries (§12.2 of
//! `docs/design/actions.md`), or an action the runner fetches ([`resolve_uses`]).
//!
//! A step's `uses:` names a built-in, by major version, or one of the `actions/*` versions it
//! stands in for, so common GitHub workflows run unchanged:
//!
//! | `uses:` | Alias of | `with:` |
//! |---|---|---|
//! | `rbit/checkout@v1` | `actions/checkout@v2` to `@v5` | `ref`, `path`, `fetch-depth`, `fetch-tags`, `submodules`, `lfs`, `clean`, `persist-credentials` |
//! | `rbit/cache@v1` (also `/restore`, `/save`) | `actions/cache@v3` to `@v4` (also `/restore`, `/save`) | `key`, `path`, `restore-keys`, `fail-on-cache-miss` |
//! | `rbit/upload-artifact@v1` | `actions/upload-artifact@v3` to `@v4` | `name`, `path`, `if-no-files-found`, `retention-days`, `overwrite` |
//! | `rbit/download-artifact@v1` | `actions/download-artifact@v3` to `@v4` | `name`, `path`, `pattern`, `merge-multiple` |
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

const CHECKOUT: [Input; 8] = [
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
const UPLOAD_ARTIFACT: [Input; 5] = [
    input("name", Some("artifact"), InputKind::Text),
    required("path"),
    input("if-no-files-found", Some("warn"), InputKind::Choice(&["warn", "error", "ignore"])),
    input("retention-days", None, InputKind::Number(1)),
    input("overwrite", Some("false"), InputKind::Boolean),
];
const DOWNLOAD_ARTIFACT: [Input; 4] = [
    input("name", None, InputKind::Text),
    input("path", None, InputKind::Text),
    input("pattern", None, InputKind::Text),
    input("merge-multiple", Some("false"), InputKind::Boolean),
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
        return Err(format!("`{uses}`: reusable workflows are not supported by rbit"));
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
        return Err(format!("`{uses}`: reusable workflows are not supported by rbit"));
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
mod tests {
    use super::*;

    #[test]
    fn names_and_aliases() {
        let cases = [
            ("rbit/checkout@v1", BuiltinKind::Checkout),
            ("actions/checkout@v2", BuiltinKind::Checkout),
            ("actions/checkout@v4", BuiltinKind::Checkout),
            ("actions/checkout@v4.2.1", BuiltinKind::Checkout),
            ("Actions/Checkout@v5", BuiltinKind::Checkout),
            ("rbit/cache@v1", BuiltinKind::Cache),
            ("rbit/cache/restore@v1", BuiltinKind::CacheRestore),
            ("rbit/cache/save@v1.0", BuiltinKind::CacheSave),
            ("actions/cache@v3", BuiltinKind::Cache),
            ("actions/cache@v4", BuiltinKind::Cache),
            ("actions/cache/restore@v4", BuiltinKind::CacheRestore),
            ("actions/cache/save@v3", BuiltinKind::CacheSave),
            ("rbit/upload-artifact@v1", BuiltinKind::UploadArtifact),
            ("actions/upload-artifact@v4", BuiltinKind::UploadArtifact),
            ("rbit/download-artifact@v1", BuiltinKind::DownloadArtifact),
            ("actions/download-artifact@v3", BuiltinKind::DownloadArtifact),
            // Later majors and pinned commits of the aliases run the built-ins too.
            ("actions/checkout@v6", BuiltinKind::Checkout),
            ("actions/checkout@0123456789abcdef0123456789abcdef01234567", BuiltinKind::Checkout),
            ("actions/cache@v5", BuiltinKind::Cache),
            ("actions/upload-artifact@v6", BuiltinKind::UploadArtifact),
        ];
        for (uses, kind) in cases {
            assert_eq!(resolve(uses), Ok((kind, 1)), "{uses}");
        }
        for kind in [BuiltinKind::Checkout, BuiltinKind::CacheRestore, BuiltinKind::UploadArtifact] {
            assert_eq!(resolve(&format!("{}@v1", name(kind))), Ok((kind, 1)));
        }
    }

    #[test]
    fn everything_else_is_refused_with_advice() {
        let cases = [
            ("actions/setup-node@v4", "not one of rbit's built-in actions"),
            ("dtolnay/rust-toolchain@stable", "or install it in a `run:` step"),
            ("actions/checkout@v1", "rbit supports `actions/checkout` @v2 and later"),
            ("actions/cache@v2", "@v3 and later"),
            ("rbit/checkout@v2", "rbit supports `rbit/checkout` @v1"),
            ("actions/checkout@main", "chosen by major version or commit, as in `actions/checkout@v2`"),
            ("actions/checkout@0123456789abcdef", "chosen by major version or commit"),
            ("actions/checkout@v4.x", "chosen by major version"),
            ("actions/checkout@v4.1.2.3", "chosen by major version"),
            ("actions/checkout", "needs a version"),
            ("actions/checkout@${{ matrix.version }}", "cannot contain expressions"),
            ("${{ matrix.action }}", "cannot contain expressions"),
        ];
        for (uses, message) in cases {
            let error = resolve(uses).unwrap_err();
            assert!(error.contains(message), "{uses}: {error}");
        }
    }

    // Actions: third-party actions.
    #[test]
    fn other_actions_resolve_to_what_the_runner_fetches() {
        let repository = |owner: &str, repo: &str, path: &str, reference: &str| {
            Ok(Uses::Action(ActionSource::Repository {
                owner: owner.into(),
                repo: repo.into(),
                path: path.into(),
                reference: reference.into(),
            }))
        };
        assert_eq!(resolve_uses("actions/setup-node@v4"), repository("actions", "setup-node", "", "v4"));
        assert_eq!(resolve_uses("dtolnay/rust-toolchain@stable"), repository("dtolnay", "rust-toolchain", "", "stable"));
        assert_eq!(resolve_uses("github/codeql-action/init@v3"), repository("github", "codeql-action", "init", "v3"));
        let sha = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(resolve_uses(&format!("actions/setup-python@{sha}")), repository("actions", "setup-python", "", sha));
        assert_eq!(resolve_uses("octo/tools@release/1.x"), repository("octo", "tools", "", "release/1.x"));
        assert_eq!(resolve_uses("./.github/actions/build"), Ok(Uses::Action(ActionSource::Local { path: ".github/actions/build".into() })));
        assert_eq!(resolve_uses("./"), Ok(Uses::Action(ActionSource::Local { path: String::new() })));
        assert_eq!(resolve_uses("docker://alpine:3.20"), Ok(Uses::Action(ActionSource::Docker { image: "alpine:3.20".into() })));
        assert_eq!(resolve_uses("actions/checkout@v4"), Ok(Uses::Builtin(BuiltinKind::Checkout, 1)));
        assert_eq!(resolve_uses("Actions/Upload-Artifact@v4"), Ok(Uses::Builtin(BuiltinKind::UploadArtifact, 1)));

        let refused = [
            ("octo/repo/.github/workflows/ci.yml@v1", "reusable workflows are not supported"),
            ("octo/repo@${{ inputs.v }}", "cannot contain expressions"),
            ("octo@v1", "name an action as `owner/repo@ref`"),
            ("octo/../etc@v1", "name an action as"),
            ("octo/repo/../../x@v1", "name an action as"),
            ("-x/repo@v1", "name an action as"),
            ("octo/re po@v1", "is not an action reference"),
            ("octo/repo@--upload-pack=evil", "is not a branch, tag or commit"),
            ("octo/repo@a..b", "is not a branch, tag or commit"),
            ("octo/repo@", "is not a branch, tag or commit"),
            ("octo/repo", "needs a version"),
            ("../outside", "inside the workspace"),
            ("/etc/action", "inside the workspace"),
            ("./a/../../b", "inside the workspace"),
            ("docker://", "needs an image"),
            ("docker://-v", "needs an image"),
            ("actions/checkout@v1", "rbit supports `actions/checkout`"),
        ];
        for (uses, message) in refused {
            let error = resolve_uses(uses).unwrap_err();
            assert!(error.contains(message), "{uses}: {error}");
        }
    }

    #[test]
    fn with_inputs_are_checked() {
        let with = |pairs: &[(&str, &str)]| pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect();
        let checkout =
            builtin("actions/checkout@v4", with(&[("fetch-depth", "0"), ("submodules", "recursive"), ("ref", "${{ inputs.ref }}")]))
                .unwrap();
        assert_eq!((checkout.kind, checkout.version, checkout.with.len()), (BuiltinKind::Checkout, 1, 3));
        assert_eq!(checkout.uses, "actions/checkout@v4");
        builtin("actions/cache@v4", with(&[("key", "cargo-${{ hashFiles('Cargo.lock') }}"), ("path", "~/.cargo")])).unwrap();
        builtin("rbit/upload-artifact@v1", with(&[("path", "dist/"), ("retention-days", "7"), ("if-no-files-found", "error")])).unwrap();
        builtin("actions/download-artifact@v4", BTreeMap::new()).unwrap();
        let refused = [
            ("actions/checkout@v4", with(&[("token", "x")]), "has no input `token` in rbit; it takes ref, path, fetch-depth"),
            ("actions/checkout@v4", with(&[("fetch-depth", "-1")]), "`fetch-depth` must be a whole number of at least 0"),
            ("actions/checkout@v4", with(&[("submodules", "maybe")]), "`submodules` must be false, true, recursive"),
            ("actions/checkout@v4", with(&[("lfs", "yes")]), "`lfs` must be true or false"),
            ("actions/cache@v4", with(&[("path", "x")]), "needs `with.key`"),
            ("actions/cache/save@v4", with(&[("key", "k"), ("path", "p"), ("restore-keys", "r")]), "has no input `restore-keys`"),
            ("actions/upload-artifact@v4", with(&[("name", "x")]), "needs `with.path`"),
            ("actions/upload-artifact@v4", with(&[("path", "x"), ("retention-days", "0")]), "at least 1"),
            ("actions/upload-artifact@v4", with(&[("path", "x"), ("compression-level", "9")]), "no input `compression-level`"),
            ("actions/download-artifact@v4", with(&[("github-token", "x")]), "no input `github-token`"),
        ];
        for (uses, with, message) in refused {
            let error = builtin(uses, with).unwrap_err();
            assert!(error.contains(message), "{uses}: {error}");
        }
    }

    #[test]
    fn every_built_in_has_its_inputs() {
        for (action, kind, ..) in ACTIONS {
            assert!(!inputs(kind).is_empty(), "{action}");
            assert!(name(kind).starts_with("rbit/"));
        }
        let checkout: Vec<_> = inputs(BuiltinKind::Checkout).iter().map(|input| (input.name, input.default)).collect();
        assert!(checkout.contains(&("fetch-depth", Some("1"))) && checkout.contains(&("persist-credentials", Some("true"))));
    }
}
