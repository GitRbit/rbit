//! `uses:` → one of the built-in actions `rbit-runner` carries (§12.2 of
//! `docs/design/actions.md`).
//!
//! rbit runs no action marketplace. A step's `uses:` names a built-in, by major version, or
//! one of the `actions/*` versions it stands in for, so common GitHub workflows run
//! unchanged:
//!
//! | `uses:` | Alias of | `with:` |
//! |---|---|---|
//! | `rbit/checkout@v1` | `actions/checkout@v2` to `@v5` | `ref`, `path`, `fetch-depth`, `fetch-tags`, `submodules`, `lfs`, `clean`, `persist-credentials` |
//! | `rbit/cache@v1` (also `/restore`, `/save`) | `actions/cache@v3` to `@v4` (also `/restore`, `/save`) | `key`, `path`, `restore-keys`, `fail-on-cache-miss` |
//! | `rbit/upload-artifact@v1` | `actions/upload-artifact@v3` to `@v4` | `name`, `path`, `if-no-files-found`, `retention-days`, `overwrite` |
//! | `rbit/download-artifact@v1` | `actions/download-artifact@v3` to `@v4` | `name`, `path`, `pattern`, `merge-multiple` |
//!
//! Anything else is a workflow error that says what to do instead, never a download:
//! `actions/setup-*` and other actions (a `container` image or a `run:` step), `docker://`
//! images, local and remote composite or JavaScript actions, reusable workflows, and a
//! `uses:` with `${{ }}` in it. A `with:` key a built-in does not have is an error too, and
//! so is a literal value it cannot take (`submodules: maybe`); values with expressions are
//! checked by the runner.

use std::collections::BTreeMap;

use crate::spec::{Builtin, BuiltinKind};

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

/// Every action name rbit resolves, with the major versions it accepts and the built-in and
/// built-in version they resolve to.
const ACTIONS: [(&str, BuiltinKind, std::ops::RangeInclusive<u32>, u32); 12] = [
    ("rbit/checkout", BuiltinKind::Checkout, 1..=1, 1),
    ("actions/checkout", BuiltinKind::Checkout, 2..=5, 1),
    ("rbit/cache", BuiltinKind::Cache, 1..=1, 1),
    ("rbit/cache/restore", BuiltinKind::CacheRestore, 1..=1, 1),
    ("rbit/cache/save", BuiltinKind::CacheSave, 1..=1, 1),
    ("actions/cache", BuiltinKind::Cache, 3..=4, 1),
    ("actions/cache/restore", BuiltinKind::CacheRestore, 3..=4, 1),
    ("actions/cache/save", BuiltinKind::CacheSave, 3..=4, 1),
    ("rbit/upload-artifact", BuiltinKind::UploadArtifact, 1..=1, 1),
    ("actions/upload-artifact", BuiltinKind::UploadArtifact, 3..=4, 1),
    ("rbit/download-artifact", BuiltinKind::DownloadArtifact, 1..=1, 1),
    ("actions/download-artifact", BuiltinKind::DownloadArtifact, 3..=4, 1),
];

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
    if known.is_empty() {
        let builtins = ["rbit/checkout", "rbit/cache", "rbit/upload-artifact", "rbit/download-artifact"].join(", ");
        return Err(format!("`{action}` is not one of rbit's built-in actions ({builtins}); {INSTEAD}"));
    }
    let major = major_version(reference)
        .ok_or_else(|| format!("`{uses}`: built-in actions are chosen by major version, as in `{action}@v{}`", known[0].2.end()))?;
    known.iter().find(|(_, _, versions, _)| versions.contains(&major)).map(|(_, kind, _, version)| (*kind, *version)).ok_or_else(|| {
        let versions = &known[0].2;
        let range = if versions.start() == versions.end() {
            format!("@v{}", versions.start())
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
            ("actions/setup-go@v5", "use a `container` image"),
            ("dtolnay/rust-toolchain@stable", "or install it in a `run:` step"),
            ("actions/checkout@v1", "rbit supports `actions/checkout` @v2 to @v5"),
            ("actions/checkout@v6", "@v2 to @v5"),
            ("actions/cache@v2", "@v3 to @v4"),
            ("rbit/checkout@v2", "rbit supports `rbit/checkout` @v1"),
            ("actions/checkout@main", "chosen by major version, as in `actions/checkout@v5`"),
            ("actions/checkout@0123456789abcdef0123456789abcdef01234567", "chosen by major version"),
            ("actions/checkout@v4.x", "chosen by major version"),
            ("actions/checkout@v4.1.2.3", "chosen by major version"),
            ("actions/checkout", "needs a version"),
            ("docker://alpine:3", "`docker://` steps are not supported by rbit"),
            ("./.github/actions/build", "local actions are not supported by rbit"),
            ("octo/repo/.github/workflows/ci.yml@v1", "reusable workflows are not supported by rbit"),
            ("actions/checkout@${{ matrix.version }}", "cannot contain expressions"),
            ("${{ matrix.action }}", "cannot contain expressions"),
        ];
        for (uses, message) in cases {
            let error = resolve(uses).unwrap_err();
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
