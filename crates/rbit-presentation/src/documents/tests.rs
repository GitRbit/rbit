use super::*;

fn file(name: &str) -> DocumentEntry<'_> {
    DocumentEntry { name, shape: EntryShape::File }
}

#[test]
fn conventional_names_multiple_licenses_and_readme_priority() {
    let entries = [
        file("README.txt"),
        file("README.md"),
        file("LICENSE-MIT"),
        file("LICENSE-APACHE"),
        file("COPYING.LESSER"),
        file("CODE_OF_CONDUCT.md"),
        file("CONTRIBUTING.md"),
        file("SECURITY.md"),
        file("NOTICE"),
        file("readme.rs"),
        file("main.rs"),
    ];
    let mut found = candidates(entries, "", false);
    sort_and_bound(&mut found);
    assert_eq!(found.len(), 9);
    assert_eq!(found[0].path, "README.md");
    assert_eq!(found[1].path, "README.txt");
    assert_eq!(found.iter().filter(|candidate| candidate.kind == DocumentKind::License).count(), 3);
    let mit = found.iter().find(|candidate| candidate.path == "LICENSE-MIT").unwrap();
    assert_eq!(label(mit, &found), ("MIT license".into(), "file-text"));
}

#[test]
fn only_regular_files_are_candidates_and_license_directory_is_bounded() {
    let symlink = DocumentEntry { name: "README.md", shape: EntryShape::Other };
    let submodule = DocumentEntry { name: "LICENSE", shape: EntryShape::Other };
    assert_eq!(candidates([symlink, submodule], "", false), Vec::<DocumentCandidate>::new());
    let names: Vec<_> = (0..100).map(|index| format!("MIT-{index}.txt")).collect();
    let mut found = candidates(names.iter().map(|name| file(name)), "LICENSES", true);
    sort_and_bound(&mut found);
    assert_eq!(found.len(), MAX_DOCUMENTS);
    assert!(found.iter().all(|candidate| candidate.path.starts_with("LICENSES/")));
}

#[test]
fn fallbacks_fill_only_missing_kinds_and_never_licenses() {
    let root = [file("README.md"), DocumentEntry { name: "Docs", shape: EntryShape::Directory }];
    assert_eq!(directory(root, "docs"), Some("Docs"));
    assert_eq!(directory(root, "README.md"), None, "files are not directories");
    let mut found = candidates(root, "", false);
    add_fallback(&mut found, candidates([file("README.md"), file("SECURITY.md"), file("LICENSE")], ".github", false));
    add_fallback(&mut found, candidates([file("SECURITY.md"), file("CONTRIBUTING.md")], "Docs", false));
    sort_and_bound(&mut found);
    let paths: Vec<_> = found.iter().map(|candidate| candidate.path.as_str()).collect();
    assert_eq!(paths, ["README.md", "Docs/CONTRIBUTING.md", ".github/SECURITY.md"]);
}

#[test]
fn generic_licenses_get_distinct_labels() {
    let found = candidates([file("LICENSE"), file("COPYING"), file("README"), file("readme.md")], "", false);
    let labels: Vec<_> = found.iter().map(|candidate| label(candidate, &found).0).collect();
    assert_eq!(labels, ["License (LICENSE)", "License (COPYING)", "README", "readme.md"]);
}
