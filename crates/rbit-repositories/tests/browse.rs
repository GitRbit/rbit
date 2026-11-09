//! Browsing decisions that need no Git.
use rbit_repositories::browse::{
    COMMITS_PER_PAGE, MAX_RAW_BYTES, commit_offset, file_name, image_content_type, join_path, landing_revision, matching_entries,
    parent_path, split_known_ref,
};

#[test]
fn known_refs_split_at_the_longest_whole_segment() {
    let refs = ["main", "feature", "feature/login", "v1.0"];
    let split = |s: &str| split_known_ref(refs, s);
    assert_eq!(split("main"), Some(("main".into(), String::new())));
    assert_eq!(split("/feature/login/src/"), Some(("feature/login".into(), "src".into())));
    assert_eq!(split("feature/logout"), Some(("feature".into(), "logout".into())));
    assert_eq!(split("mainline/src"), None);
    assert_eq!(split("3f9a2c1/src"), None);
}

#[test]
fn landing_prefers_the_default_branch_then_the_first() {
    assert_eq!(landing_revision(["a", "main"], "main"), Some("main"));
    assert_eq!(landing_revision(["a", "b"], "main"), Some("a"));
    assert_eq!(landing_revision([] as [&str; 0], "main"), None);
}

#[test]
fn history_pages_and_raw_downloads_are_bounded() {
    assert_eq!((COMMITS_PER_PAGE, MAX_RAW_BYTES), (30, 100 * 1024 * 1024));
    assert_eq!((commit_offset(0), commit_offset(1), commit_offset(2)), (Some(0), Some(0), Some(30)));
    assert_eq!(commit_offset(usize::MAX), None);
}

#[test]
fn tree_filters_and_paths() {
    let entries = ["README.md", "src", "Readme.txt"];
    let found: Vec<usize> = matching_entries(&entries, |entry: &&str| *entry, "readme").into_iter().map(|(index, _)| index).collect();
    assert_eq!(found, [0, 2]);
    assert_eq!(matching_entries(&entries, |entry: &&str| *entry, "").len(), 3);
    assert_eq!((join_path("", "a"), join_path("d", "a")), ("a".to_owned(), "d/a".to_owned()));
    assert_eq!((parent_path("a/b/c"), parent_path("c")), ("a/b", ""));
    assert_eq!((file_name("a/b/c"), file_name("c")), ("c", "c"));
    assert_eq!(
        (image_content_type("x/Logo.PNG"), image_content_type("a.svg"), image_content_type("a.txt")),
        (Some("image/png"), Some("image/svg+xml"), None)
    );
}
