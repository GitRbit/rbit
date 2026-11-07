//! Authored multi-file fixtures exercise logical ownership rather than Rust formatting.

use std::{fs, path::Path};

use tempfile::TempDir;

use crate::report::{Report, check};

fn fixture(files: &[(&str, &str)]) -> TempDir {
    let directory = TempDir::new().unwrap();
    fs::write(directory.path().join("Cargo.toml"), "[package]\nname='fixture'\nversion='0.1.0'\n").unwrap();
    for (path, text) in files {
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    directory
}

fn named_lines(report: &Report, name: &str) -> usize {
    report.types.iter().find(|ty| ty.name == name).unwrap().lines
}

fn assert_clean(root: &Path, limit: usize) -> Report {
    let report = check(root, limit);
    assert!(report.violations.is_empty(), "{:?}", report.violations);
    report
}

#[test]
fn external_inherent_and_trait_implementations_are_cumulative() {
    let root = fixture(&[
        ("src/lib.rs", "mod model;\nmod first;\nmod second;\n"),
        ("src/model.rs", "pub struct Widget {\n pub value: u8,\n}\n"),
        ("src/first.rs", "use crate::model::Widget as W;\nimpl W {\n pub fn one(&self) -> u8 {\n self.value\n }\n}\n"),
        ("src/second.rs", "use super::model::Widget;\nimpl Default for Widget {\n fn default() -> Self {\n Self {value: 1}\n }\n}\n"),
    ]);
    let report = check(root.path(), 12);
    assert_eq!(named_lines(&report, "lib::model::Widget"), 13);
    assert_eq!(report.violations.len(), 1);
    assert!(report.violations[0].contains("13 cumulative"));
}

#[test]
fn module_aliases_grouped_uses_and_type_aliases_resolve_to_one_type() {
    let root = fixture(&[
        (
            "src/lib.rs",
            "mod model;\nuse crate::model::{self as models, Widget as Renamed};\ntype Handle = Renamed;\nimpl models::Widget {}\nimpl Handle {}\n",
        ),
        ("src/model.rs", "pub struct Widget;\n"),
    ]);
    let report = assert_clean(root.path(), 4);
    assert_eq!(report.types.len(), 1);
    assert_eq!(named_lines(&report, "lib::model::Widget"), 4);
}

#[test]
fn unrelated_same_names_and_compilation_targets_do_not_merge() {
    let root = fixture(&[
        ("src/lib.rs", "pub struct Widget;\nimpl Widget {}\nmod other {\n pub struct Widget;\n impl Widget {}\n}\n"),
        ("tests/example.rs", "struct Widget;\nimpl Widget {}\n"),
    ]);
    let report = assert_clean(root.path(), 2);
    assert_eq!(report.types.len(), 3);
    assert!(report.types.iter().all(|ty| ty.lines == 2));
}

#[test]
fn overlapping_source_lines_count_once_for_the_same_type() {
    let root = fixture(&[("src/lib.rs", "struct Widget; impl Widget {}\n")]);
    assert_eq!(named_lines(&assert_clean(root.path(), 1), "lib::Widget"), 1);
}

#[test]
fn ambiguous_glob_import_is_a_failure() {
    let root = fixture(&[("src/lib.rs", "mod a {pub struct Widget;}\nmod b {pub struct Widget;}\nuse a::*;\nuse b::*;\nimpl Widget {}\n")]);
    assert!(check(root.path(), 400).violations.iter().any(|error| error.contains("ambiguous")));
}

#[test]
fn unknown_local_self_type_cannot_silently_skip_the_check() {
    let root = fixture(&[("src/lib.rs", "impl crate::Missing {}\n")]);
    assert!(check(root.path(), 400).violations.iter().any(|error| error.contains("cannot resolve")));
}

#[test]
fn malformed_source_and_missing_module_are_actionable_failures() {
    let malformed = fixture(&[("src/lib.rs", "struct Widget {\n")]);
    assert_ne!(check(malformed.path(), 400).violations, Vec::<String>::new());
    let missing = fixture(&[("src/lib.rs", "mod missing;\n")]);
    assert_ne!(check(missing.path(), 400).violations, Vec::<String>::new());
}

#[test]
fn trait_definition_has_its_own_limit_and_implementation_counts_for_self_type() {
    let root =
        fixture(&[("src/lib.rs", "trait Work {\n fn run(&self) {}\n}\nstruct Widget;\nimpl Work for Widget {\n fn run(&self) {}\n}\n")]);
    let report = check(root.path(), 3);
    assert_eq!(named_lines(&report, "lib::Work"), 3);
    assert_eq!(named_lines(&report, "lib::Widget"), 4);
    assert_eq!(report.violations.len(), 1);
}

#[test]
fn local_items_in_functions_keep_separate_lexical_owners() {
    let root = fixture(&[("src/lib.rs", "fn one() {\n struct Local;\n impl Local {}\n}\nfn two() {\n struct Local;\n impl Local {}\n}\n")]);
    let report = assert_clean(root.path(), 2);
    assert_eq!(report.types.len(), 2);
}

#[test]
fn known_foreign_self_types_have_independent_implementation_limits() {
    let root = fixture(&[("src/lib.rs", "use std::sync::Arc;\ntrait Work {}\nimpl Work for Arc<u8> {}\n")]);
    let report = assert_clean(root.path(), 1);
    assert_eq!(report.types.len(), 2);
    assert!(report.types.iter().any(|ty| ty.name.contains("@foreign-impl")));
}

#[test]
fn explicit_external_module_path_is_checked() {
    let root = fixture(&[("src/lib.rs", "#[path=\"shared.rs\"]\nmod alternate;\n"), ("src/shared.rs", "struct Widget;\nimpl Widget {}\n")]);
    assert_eq!(named_lines(&assert_clean(root.path(), 2), "lib::alternate::Widget"), 2);
}

#[test]
fn local_import_cycles_fail_closed() {
    let root = fixture(&[("src/lib.rs", "use crate::B as A;\nuse crate::A as B;\nimpl A {}\n")]);
    assert!(check(root.path(), 400).violations.iter().any(|error| error.contains("cyclic")));
}

#[test]
fn explicit_local_import_cannot_be_mistaken_for_a_foreign_dependency() {
    let root = fixture(&[("src/lib.rs", "mod std {}\nuse crate::std::Missing as Arc;\ntrait Work {}\nimpl Work for Arc {}\n")]);
    assert!(check(root.path(), 400).violations.iter().any(|error| error.contains("cannot resolve")));
}

#[test]
fn external_path_attribute_is_relative_to_its_source_file() {
    let root = fixture(&[
        ("src/lib.rs", "mod outer;\n"),
        ("src/outer.rs", "#[path=\"shared.rs\"]\nmod inner;\n"),
        ("src/shared.rs", "struct Widget;\n"),
    ]);
    assert_eq!(named_lines(&assert_clean(root.path(), 1), "lib::outer::inner::Widget"), 1);
}

#[test]
fn missing_type_with_a_glob_terminates_with_an_actionable_error() {
    let root = fixture(&[("src/lib.rs", "mod models {pub struct Known;}\nuse models::*;\nimpl Missing {}\n")]);
    assert!(check(root.path(), 400).violations.iter().any(|error| error.contains("cannot resolve")));
}

#[test]
fn imported_module_glob_resolution_has_a_hard_recursion_budget() {
    let root = fixture(&[("src/lib.rs", "mod models {}\nuse crate::models as m;\nuse m::*;\nimpl Missing {}\n")]);
    assert_ne!(check(root.path(), 400).violations, Vec::<String>::new());
}

#[test]
fn module_value_export_does_not_redirect_bound_type_paths_or_parent_globs() {
    let root = fixture(&[
        ("src/lib.rs", "mod plan;\npub use plan::plan;\n"),
        (
            "src/plan.rs",
            "pub fn plan() {}\nuse std::collections::BTreeMap;\npub type Mapping = BTreeMap<String, String>;\nmod tests {\nuse super::*;\nstruct Local;\nimpl Default for Local { fn default() -> Self { Self } }\n}\n",
        ),
    ]);
    let report = assert_clean(root.path(), 400);
    assert_eq!(named_lines(&report, "lib::plan::tests::Local"), 2);
}
