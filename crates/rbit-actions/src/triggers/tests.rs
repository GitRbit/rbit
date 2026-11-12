use super::*;

fn pushed<'a>(name: &'a str, before: Option<&'a str>, after: Option<&'a str>) -> PushedRef<'a> {
    PushedRef { name, before, after }
}

#[test]
fn pushes_consider_live_branches_and_tags_in_order_up_to_the_cap() {
    let mut updates = vec![
        ("refs/heads/gone", Some("a"), None),
        ("refs/notes/commits", None, Some("b")),
        ("refs/tags/v1", None, Some("c")),
        ("refs/heads/main", Some("d"), Some("e")),
    ];
    let names: Vec<_> = considered_refs(&updates, |(n, b, a)| pushed(n, *b, *a)).into_iter().map(|u| u.0).collect();
    assert_eq!(names, ["refs/tags/v1", "refs/heads/main"]);
    updates = (0..MAX_REF_UPDATES + 4).map(|_| ("refs/heads/x", None, Some("f"))).collect();
    assert_eq!(considered_refs(&updates, |(n, b, a)| pushed(n, *b, *a)).len(), MAX_REF_UPDATES);
    assert_eq!(dropped_refs(&updates, |(n, b, a)| pushed(n, *b, *a)).len(), 4, "the rest are said to be left out");
}

#[test]
fn the_default_branch_head_and_comparison_bases() {
    let refs = [pushed("refs/heads/topic", None, Some("1")), pushed("refs/heads/main", Some("0"), Some("2"))];
    assert_eq!(default_branch_head(refs, "main"), Some("2"));
    assert_eq!(default_branch_head([pushed("refs/heads/main", Some("0"), None)], "main"), None);
    assert_eq!(default_branch_head(refs, "trunk"), None);
    assert_eq!(comparison_base(pushed("refs/heads/topic", Some("0"), Some("1")), "main"), Some("0"));
    assert_eq!(comparison_base(pushed("refs/heads/topic", None, Some("1")), "main"), Some("main"));
    assert_eq!(comparison_base(pushed("refs/heads/main", None, Some("1")), "main"), None);
    assert_eq!(comparison_base(pushed("refs/tags/v1", None, Some("1")), "main"), None);
}

#[test]
fn changed_paths_are_known_only_within_the_cap_and_with_a_merge_base() {
    assert!(changed_paths_known(MAX_CHANGED_PATHS, true));
    assert!(!changed_paths_known(LISTED_CHANGED_PATHS, true));
    assert!(!changed_paths_known(0, false));
    assert_eq!(head_message("Fix", Some("[skip ci]")), "Fix\n\n[skip ci]");
    assert_eq!(head_message("Fix", None), "Fix");
}

#[test]
fn pull_request_events_map_to_activities_and_causes() {
    assert_eq!(pull_trigger::<u8>(PullEvent::Opened), Some((PullActivity::Opened, None)));
    assert_eq!(pull_trigger::<u8>(PullEvent::ReadyForReview), Some((PullActivity::ReadyForReview, None)));
    assert_eq!(pull_trigger(PullEvent::Synchronized { starts_runs: true, pusher: 7 }), Some((PullActivity::Synchronize, Some(7))));
    assert_eq!(pull_trigger(PullEvent::Synchronized { starts_runs: false, pusher: 7 }), None);
    assert_eq!(pull_trigger::<u8>(PullEvent::Other), None);
    assert_eq!(pull_cause(Some(7), 1), 7);
    assert_eq!(pull_cause(None, 1), 1);
}

#[test]
fn closing_cancels_only_runs_that_have_not_started() {
    for stage in [RunStage::Waiting, RunStage::Held, RunStage::AwaitingApproval] {
        assert!(cancelled_when_pull_closes(stage));
    }
    assert!(!cancelled_when_pull_closes(RunStage::Started));
}

#[test]
fn a_scratch_ref_is_pushed_like_a_branch_of_its_name() {
    let updates = [("refs/scratch/ada/4c1e9a", None, Some("a")), ("refs/scratch/ada/gone", Some("b"), None)];
    let names: Vec<_> = considered_refs(&updates, |(n, b, a)| pushed(n, *b, *a)).into_iter().map(|u| u.0).collect();
    assert_eq!(names, ["refs/scratch/ada/4c1e9a"], "a deleted scratch ref starts nothing");
    let scratch = pushed("refs/scratch/ada/4c1e9a", None, Some("a"));
    assert_eq!(comparison_base(scratch, "main"), Some("main"), "its paths are compared with the default branch");
}
