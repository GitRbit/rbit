use super::*;

const LIMITS: Limits = Limits { title_chars: 5, text_bytes: 3, release_name_chars: 4, release_notes_chars: 2 };

#[test]
fn labels_and_milestones_follow_rbits_limits() {
    let made = label("  bug  ", "F29513", Some("  Broken  ")).unwrap();
    assert_eq!(made, LabelItem { name: "bug".into(), color: "#f29513".into(), description: "Broken".into() });
    assert_eq!(label("x", "red", None).unwrap().color, DEFAULT_LABEL_COLOR);
    assert_eq!(label(&"n".repeat(60), "", None).unwrap().name.len(), MAX_LABEL_NAME);
    assert_eq!(label("  ", "", None).unwrap_err(), "label \"  \" was skipped: its name is not valid here");
    assert!(label("a\u{7}b", "", None).is_err());
    assert_eq!(milestone("  v1  ", Some("héllo"), LIMITS), Some(("v1".into(), "hé".into())));
    assert_eq!(milestone("   ", None, LIMITS), None);
    assert_eq!(due_date("2026-10-15T00:00:00Z"), "2026-10-15");
    assert_eq!(lookup_key("  Bug "), "bug");
}

#[test]
fn threads_keep_their_titles_closing_and_branch_fallbacks() {
    assert_eq!(thread_title("  a\tbcdefg ", LIMITS), "a bcd");
    assert_eq!(closing(true, true, true, true, true), Closing::Merged);
    assert_eq!(closing(false, true, false, false, false), Closing::Completed);
    assert_eq!(closing(false, true, false, true, false), Closing::NotPlanned);
    assert_eq!(closing(false, false, true, false, true), Closing::NotPlanned, "open pulls without commits");
    assert_eq!(closing(false, false, false, true, false), Closing::Open);
    assert!(closed_pull_note("GitHub").starts_with("\n\n*This pull request was open on GitHub;"));
    assert!(is_full_id(&"a".repeat(40)) && is_full_id(&"0".repeat(64)));
    assert!(!is_full_id("abc") && !is_full_id(&"g".repeat(40)));
    assert_eq!(non_empty(" ", "main"), "main");
    assert_eq!(history_head("abc"), "0".repeat(40));
    assert_eq!(comment_text(Some("  ")), EMPTY_COMMENT);
    assert_eq!(comment_text(Some("hi")), "hi");
}

#[test]
fn releases_and_files_keep_their_rules() {
    assert_eq!(release("  ", None, None, LIMITS), None);
    assert_eq!(release(" v1 ", Some(" a\u{7}bcdef"), Some("notes"), LIMITS), Some(("v1".into(), "a bc".into(), "no".into())));
    assert_eq!(release("v1", Some("   "), None, LIMITS).unwrap().1, "v1");
    assert_eq!(
        asset_refusal("v1", "a.zip", 11, 10, Ok(())).as_deref(),
        Some("release v1: a.zip was skipped: it is larger than this instance allows")
    );
    assert_eq!(asset_refusal("v1", "a/b", 1, 10, Err("bad name".into())).as_deref(), Some("release v1: a/b was skipped: bad name"));
    assert_eq!(asset_refusal("v1", "a.zip", 10, 10, Ok(())), None);
    assert_eq!(asset_download(false, Some("api"), Some("web")), Some(("api", "application/octet-stream")));
    assert_eq!(asset_download(true, Some("api"), Some("web")), Some(("web", "*/*")));
    assert_eq!(asset_download(true, Some("api"), None), Some(("api", "application/octet-stream")));
    assert_eq!(asset_download(true, None, None), None);
    assert_eq!(lfs_batch_endpoint("https://g.example/o/r.git/"), "https://g.example/o/r.git/info/lfs/objects/batch");
    assert_eq!(lfs_batch_endpoint("https://g.example/o/r"), "https://g.example/o/r.git/info/lfs/objects/batch");
}

#[test]
fn numbers_merges_heads_and_item_warnings_keep_their_rules() {
    assert_eq!(highest_number([3, 9, 4]), Some(9));
    assert_eq!(highest_number([]), None);
    let sha = "a".repeat(40);
    assert_eq!(merge_commit(Some(&sha)), Some(sha.clone()));
    assert_eq!(merge_commit(Some("abc")), None);
    assert_eq!(merged_event_commit(Some("abc"), "head"), "head");
    assert_eq!(merged_event_commit(Some(&sha), "head"), sha);
    assert!(same_repository(Some("Octo/Tools"), "octo/tools") && !same_repository(None, "octo/tools"));
    assert_eq!(incomplete_asset("v1", "a.zip", 3, 5).as_deref(), Some("release v1: a.zip arrived with 3 of 5 bytes; removed"));
    assert_eq!(incomplete_asset("v1", "a.zip", 3, 0), None);
    assert_eq!(incomplete_asset("v1", "a.zip", 5, 5), None);
    assert_eq!(unknown_branches(4), "#4: the pull request's branches are unknown; skipped");
    assert_eq!((thread_exists(4), thread_failed(4)), ("#4 already exists here; skipped".into(), "#4 could not be imported".into()));
    assert_eq!(release_exists("v1"), "release v1 already exists here; skipped");
    assert_eq!(release_failed("v1"), "release v1 could not be imported");
}
