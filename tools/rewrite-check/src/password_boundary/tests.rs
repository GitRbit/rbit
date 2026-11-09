//! Parsed/token fixtures keep comments and data separate from authored scheduling calls.

use std::fs;

use tempfile::TempDir;

use super::{Report, check};

fn fixture(files: &[(&str, &str)]) -> TempDir {
    let root = TempDir::new().unwrap();
    for (path, text) in [("crates/rbit-services/src/lib.rs", "// fixture\n"), ("crates/rbit-server/src/main.rs", "fn main() {}\n")]
        .into_iter()
        .chain(files.iter().copied())
    {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    root
}

fn messages(report: &Report) -> Vec<&str> {
    report.violations.iter().map(|diagnostic| diagnostic.message.as_str()).collect()
}

#[test]
fn aliases_nested_paths_raw_identifiers_and_opaque_macro_tokens_cannot_hide_the_facade() {
    for source in [
        "use rbit_auth::Passwords as Legacy;\nfn run() { Legacy::hash(String::new()); }\n",
        "use rbit_auth::{password::{Passwords as Legacy}};\n",
        "fn run() { rbit_identity::Passwords::hash(String::new()); }\n",
        "use rbit_identity::r#Passwords as Legacy;\n",
        "macro_rules! legacy { () => { rbit_auth::Passwords::hash(String::new()) }; }\n",
    ] {
        let root = fixture(&[("crates/rbit-services/src/accounts/nested/login.rs", source)]);
        let report = check(root.path());
        assert!(report.violations.iter().any(|diagnostic| diagnostic.message.contains("legacy Passwords facade")), "{source}");
        assert!(report.violations.iter().all(|diagnostic| diagnostic.line >= 1));
    }
}

#[test]
fn diagnostics_identify_the_source_path_and_line() {
    let path = "crates/rbit-server/src/commands/credentials.rs";
    let root = fixture(&[(path, "// retained comment\nuse rbit_auth::Passwords as Legacy;\n")]);
    let report = check(root.path());
    assert_eq!(report.violations.len(), 1);
    assert_eq!(report.violations[0].file, root.path().join(path));
    assert_eq!(report.violations[0].line, 2);
}

#[test]
fn auth_identity_and_transitive_alias_wildcards_are_rejected_even_inside_macro_tokens() {
    for source in [
        "use rbit_auth::*;\n",
        "use ::rbit_identity::*;\n",
        "use rbit_auth::{password::*};\n",
        "use rbit_identity as id; use id as second; use second::*;\n",
        "use rbit_auth::{self as auth}; use auth::password::*;\n",
        "extern crate rbit_auth as auth; use auth::*;\n",
        "use r#rbit_identity as r#identity; use r#identity::*;\n",
        "macro_rules! legacy { () => { use rbit_auth::*; }; }\n",
    ] {
        let root = fixture(&[("crates/rbit-services/src/lib.rs", source)]);
        let report = check(root.path());
        assert!(messages(&report).iter().any(|message| message.contains("wildcard can hide")), "{source}");
    }
}

#[test]
fn namespace_aliases_are_tracked_across_authored_files_without_following_module_paths() {
    let root = fixture(&[
        ("crates/rbit-services/src/lib.rs", "use rbit_auth as credential_namespace;\n"),
        ("crates/rbit-services/src/nested.rs", "use crate::credential_namespace as another;\nuse another::*;\n"),
    ]);
    assert!(messages(&check(root.path())).iter().any(|message| message.contains("wildcard can hide")));
}

#[test]
fn every_crate_target_is_checked_once_the_facade_is_retired() {
    for path in [
        "crates/rbit-services/tests/compatibility.rs",
        "crates/rbit-identity/src/lib.rs",
        "crates/rbit-auth/src/password.rs",
        "crates/rbit-web/benches/login.rs",
        "crates/rbit-auth/build.rs",
    ] {
        let root = fixture(&[(path, "pub use rbit_identity::Passwords;\n")]);
        let report = check(root.path());
        assert!(report.violations.iter().any(|diagnostic| diagnostic.file == root.path().join(path)), "{path}");
    }
    let redefined = fixture(&[("crates/rbit-identity/src/credential.rs", "pub struct Passwords;\n")]);
    assert!(messages(&check(redefined.path())).iter().any(|message| message.contains("legacy Passwords facade")));
}

#[test]
fn comments_strings_and_unrelated_blocking_work_are_allowed() {
    let root = fixture(&[
        (
            "crates/rbit-services/src/lib.rs",
            r##"
//! Passwords::hash is the old API; this is documentation.
/* use rbit_auth::*; Passwords::verify; */
const NOTE: &str = r#"Passwords::hash and use rbit_identity::*;"#;
use std::sync::*;
fn git_work() { tokio::task::spawn_blocking(|| 42); }
"##,
        ),
        ("crates/rbit-services/tests/compatibility.rs", "//! Passwords was retired.\nconst OLD: &str = \"Passwords\";\n"),
        ("crates/rbit-identity/src/lib.rs", "pub struct PasswordWork;\n"),
    ]);
    let report = check(root.path());
    assert_eq!(report.files, 4);
    assert!(report.violations.is_empty(), "{:?}", report.violations);
}

#[test]
fn cfg_gated_source_is_conservative_and_malformed_or_missing_sources_fail_closed() {
    let conditional = fixture(&[("crates/rbit-server/src/main.rs", "#[cfg(test)] mod tests { use rbit_auth::Passwords; }\n")]);
    assert!(!check(conditional.path()).violations.is_empty());
    let malformed = fixture(&[("crates/rbit-services/src/lib.rs", "fn broken(\n")]);
    assert!(messages(&check(malformed.path())).iter().any(|message| message.contains("cannot parse Rust")));
    let missing = TempDir::new().unwrap();
    assert!(!check(missing.path()).violations.is_empty());
}

#[cfg(unix)]
#[test]
fn linked_rust_files_and_nested_directories_are_refused_without_reading_their_targets() {
    use std::os::unix::fs::symlink;

    let root = fixture(&[]);
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("forbidden.rs");
    fs::write(&target, "use rbit_auth::Passwords;\n").unwrap();
    symlink(&target, root.path().join("crates/rbit-server/src/linked.rs")).unwrap();
    symlink(outside.path(), root.path().join("crates/rbit-services/src/linked-directory")).unwrap();
    let report = check(root.path());
    assert_eq!(report.files, 2);
    assert_eq!(report.violations.len(), 2);
    assert!(messages(&report).iter().all(|message| message.contains("linked source entries")));
    assert_eq!(fs::read_to_string(target).unwrap(), "use rbit_auth::Passwords;\n");
}

#[cfg(unix)]
#[test]
fn every_source_ancestor_is_checked_below_the_explicit_repository_anchor() {
    use std::os::unix::fs::symlink;

    for ancestor in ["crates", "crates/rbit-services", "crates/rbit-server", "crates/rbit-services/src"] {
        let root = fixture(&[]);
        let outside = TempDir::new().unwrap();
        let original = root.path().join(ancestor);
        let target = outside.path().join("target");
        fs::rename(&original, &target).unwrap();
        let nested = match ancestor {
            "crates" => "rbit-services/src/hidden.rs",
            "crates/rbit-services/src" => "hidden.rs",
            _ => "src/hidden.rs",
        };
        fs::write(target.join(nested), "use rbit_auth::Passwords;\n").unwrap();
        symlink(&target, &original).unwrap();
        let report = check(root.path());
        assert!(messages(&report).iter().any(|message| message.contains("linked source ancestors")), "{ancestor}");
        assert!(!messages(&report).iter().any(|message| message.contains("legacy Passwords facade")), "must not read linked targets");
        assert_eq!(report.files, usize::from(ancestor != "crates"));
    }
}

#[cfg(unix)]
#[test]
fn an_explicit_root_alias_is_allowed_but_does_not_authorize_descendant_links() {
    use std::os::unix::fs::symlink;

    let root = fixture(&[]);
    let aliases = TempDir::new().unwrap();
    let alias = aliases.path().join("repository");
    symlink(root.path(), &alias).unwrap();
    assert!(check(&alias).violations.is_empty());
    let original = root.path().join("crates/rbit-services");
    let linked = aliases.path().join("linked-member");
    fs::rename(&original, &linked).unwrap();
    symlink(&linked, &original).unwrap();
    assert!(messages(&check(&alias)).iter().any(|message| message.contains("linked source ancestors")));
}
