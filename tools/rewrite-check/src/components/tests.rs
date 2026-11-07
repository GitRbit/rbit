//! Component gates reuse full AST resolution; only explicit type-size policy is scoped.
use super::check;
use std::{fmt::Write as _, fs, path::Path};
use tempfile::TempDir;
fn fixture(files: &[(&str, String)]) -> TempDir {
    let directory = TempDir::new().unwrap();
    write(
        directory.path(),
        "crates/legacy/Cargo.toml",
        "[package]\nname='legacy'\nversion='0.1.0'\nautotests=false\n[[test]]\nname='integration'\npath='tests/main.rs'\n",
    );
    for (path, text) in files {
        write(directory.path(), &format!("crates/legacy/{path}"), text);
    }
    policy(directory.path(), &["lib::roles::Reader"]);
    directory
}
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn policy(root: &Path, names: &[&str]) {
    write(
        root,
        "docs/design/rewrite/component-types.json",
        &serde_json::json!({"schema":1,"components":[{"crate":"crates/legacy","types":names}]}).to_string(),
    );
}
fn definitions() -> String {
    "pub struct Reader;\n".into()
}
fn legacy() -> String {
    let mut source = String::from("pub struct Legacy;\nimpl Legacy {\n");
    for id in 0..405 {
        writeln!(&mut source, "fn old_{id}(&self) {{}}").unwrap();
    }
    source.push_str("\n}\n");
    source
}
fn body(lines: usize) -> String {
    "let _ = 0;\n".repeat(lines)
}
#[test]
fn qualified_inherent_and_trait_impls_across_files_are_cumulative() {
    let root = fixture(&[
        ("src/lib.rs", format!("mod roles; mod first; mod second;\n{}", legacy())),
        ("src/roles.rs", definitions()),
        ("src/first.rs", format!("impl crate::roles::Reader {{\nfn read(&self) {{\n{}\n}}\n}}\n", body(198))),
        (
            "src/second.rs",
            format!("use crate::roles::Reader as R;\nimpl Default for R {{\nfn default()->Self {{\n{}\nSelf\n}}\n}}\n", body(198)),
        ),
        ("tests/main.rs", String::new()),
    ]);
    let report = check(root.path());
    assert_eq!(report.violations, Vec::<String>::new());
    let sizes = &report.crates[0];
    assert_eq!(sizes.types.len(), 1);
    assert!(sizes.types[0].lines > 400);
    assert_eq!(sizes.types[0].files.len(), 3);
    assert_eq!(sizes.violations.len(), 1);
    assert!(sizes.violations[0].contains("lib::roles::Reader"));
}
#[test]
fn unrelated_legacy_oversize_and_same_names_do_not_waive_or_pollute_selected_types() {
    let root = fixture(&[
        ("src/lib.rs", format!("mod roles;mod old {{ {} }}\n", legacy().replace("Legacy", "Reader"))),
        ("src/roles.rs", definitions()),
        ("tests/main.rs", String::new()),
    ]);
    let report = check(root.path());
    assert_eq!(report.violations, Vec::<String>::new());
    assert_eq!(report.crates[0].violations, Vec::<String>::new());
    assert_eq!(report.crates[0].types[0].name, "lib::roles::Reader");
    assert_eq!(report.crates[0].types[0].lines, 1);
    assert_ne!(crate::report::check(&root.path().join("crates/legacy"), 400).violations, Vec::<String>::new());
}
#[test]
fn missing_alias_and_unresolved_inherent_targets_fail_closed() {
    for text in ["pub struct Elsewhere;", "pub type Reader = String;", "pub struct Reader; impl Missing {}"] {
        let root = fixture(&[("src/lib.rs", "mod roles;".into()), ("src/roles.rs", text.into()), ("tests/main.rs", String::new())]);
        let report = check(root.path());
        assert_ne!(report.crates[0].violations, Vec::<String>::new(), "{text}");
    }
}
#[test]
fn missing_or_malformed_modules_remain_fatal_even_outside_selected_owner() {
    for entry in ["mod roles; mod absent;", "mod roles;mod broken;"] {
        let root = fixture(&[
            ("src/lib.rs", entry.into()),
            ("src/roles.rs", definitions()),
            ("src/broken.rs", "pub struct {".into()),
            ("tests/main.rs", String::new()),
        ]);
        assert_ne!(check(root.path()).crates[0].violations, Vec::<String>::new());
    }
}
#[test]
fn cargo_disabled_automatic_tests_keep_explicit_target_and_nested_module_errors_authoritative() {
    let root = fixture(&[
        ("src/lib.rs", "mod roles;".into()),
        ("src/roles.rs", definitions()),
        ("tests/main.rs", "mod suite;".into()),
        ("tests/suite.rs", "mod nested;".into()),
        ("tests/suite/nested.rs", "struct Fixture;".into()),
        // Cargo does not compile this conventional entry when autotests=false.
        ("tests/unbuilt.rs", "mod absent;".into()),
    ]);
    assert_eq!(check(root.path()).crates[0].violations, Vec::<String>::new());
    fs::remove_file(root.path().join("crates/legacy/tests/suite/nested.rs")).unwrap();
    assert_ne!(check(root.path()).crates[0].violations, Vec::<String>::new());
}
#[test]
fn policy_rejects_missing_empty_duplicate_wildcard_foreign_and_path_escape_mappings() {
    let root = fixture(&[("src/lib.rs", "mod roles;".into()), ("src/roles.rs", definitions()), ("tests/main.rs", String::new())]);
    for names in [vec![], vec!["lib::roles::Reader", "lib::roles::Reader"], vec!["lib::roles::*"], vec!["foreign::Reader"]] {
        policy(root.path(), &names);
        assert_ne!(check(root.path()).violations, Vec::<String>::new());
    }
    write(
        root.path(),
        "docs/design/rewrite/component-types.json",
        r#"{"schema":1,"components":[{"crate":"../escape","types":["lib::Reader"]}]}"#,
    );
    assert_ne!(check(root.path()).violations, Vec::<String>::new());
    fs::remove_file(root.path().join("docs/design/rewrite/component-types.json")).unwrap();
    assert_ne!(check(root.path()).violations, Vec::<String>::new());
}
