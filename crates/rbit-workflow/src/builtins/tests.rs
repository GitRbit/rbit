//! Existing builtins acceptance fixtures, separated from production logic.
use super::{ACTIONS, ActionSource, BTreeMap, BuiltinKind, Uses, builtin, inputs, name, resolve, resolve_uses};

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
        ("octo/repo/.github/workflows/ci.yml@v1", "call it from a job"),
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
        builtin("actions/checkout@v4", with(&[("fetch-depth", "0"), ("submodules", "recursive"), ("ref", "${{ inputs.ref }}")])).unwrap();
    assert_eq!((checkout.kind, checkout.version, checkout.with.len()), (BuiltinKind::Checkout, 1, 3));
    assert_eq!(checkout.uses, "actions/checkout@v4");
    builtin("actions/cache@v4", with(&[("key", "cargo-${{ hashFiles('Cargo.lock') }}"), ("path", "~/.cargo")])).unwrap();
    builtin("rbit/upload-artifact@v1", with(&[("path", "dist/"), ("retention-days", "7"), ("if-no-files-found", "error")])).unwrap();
    builtin("actions/download-artifact@v4", BTreeMap::new()).unwrap();
    let refused = [
        ("actions/checkout@v4", with(&[("ssh-key", "x")]), "has no input `ssh-key` in rbit; it takes repository, token, ref"),
        ("actions/checkout@v4", with(&[("fetch-depth", "-1")]), "`fetch-depth` must be a whole number of at least 0"),
        ("actions/checkout@v4", with(&[("submodules", "maybe")]), "`submodules` must be false, true, recursive"),
        ("actions/checkout@v4", with(&[("lfs", "yes")]), "`lfs` must be true or false"),
        ("actions/cache@v4", with(&[("path", "x")]), "needs `with.key`"),
        ("actions/cache/save@v4", with(&[("key", "k"), ("path", "p"), ("restore-keys", "r")]), "has no input `restore-keys`"),
        ("actions/upload-artifact@v4", with(&[("name", "x")]), "needs `with.path`"),
        ("actions/upload-artifact@v4", with(&[("path", "x"), ("retention-days", "0")]), "at least 1"),
        ("actions/upload-artifact@v4", with(&[("path", "x"), ("compression-level", "10")]), "`compression-level` must be 0, 1"),
        ("actions/download-artifact@v4", with(&[("artifact-ids", "x")]), "no input `artifact-ids`"),
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
