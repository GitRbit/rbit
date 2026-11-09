//! The gate, review rights, bounds and full-ref pattern rewriting.
use std::{
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

use super::*;

fn patterns(raw: &[&str]) -> Vec<String> {
    raw.iter().map(|pattern| (*pattern).to_owned()).collect()
}

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}

/// `main` is protected; every question is logged.
#[derive(Default)]
struct Rules(Mutex<Vec<String>>);

impl ProtectedBranches for Rules {
    type Error = ();

    fn is_protected(&self, branch: &str) -> impl Future<Output = Result<bool, ()>> + Send {
        self.0.lock().unwrap().push(format!("protected {branch}"));
        ready(Ok(branch == "main"))
    }
}

#[test]
fn selected_patterns_tell_branches_from_tags() {
    let branches = patterns(&["main", "release/*"]);
    assert!(selected(&branches, "refs/heads/main") && selected(&branches, "refs/heads/release/1.0"));
    assert!(!selected(&branches, "refs/tags/main"), "a tag named like an allowed branch");
    assert!(!selected(&branches, "refs/heads/release/1.0/hotfix") && !selected(&branches, "refs/pull/1/merge"));
    let tags = patterns(&["refs/tags/v*"]);
    assert!(selected(&tags, "refs/tags/v1.2") && !selected(&tags, "refs/heads/v1.2"), "a branch named like a tag");
}

#[test]
fn the_last_matching_selected_pattern_decides() {
    let readmitted = patterns(&["**", "!release/**", "release/ok"]);
    assert!(selected(&readmitted, "refs/heads/release/ok") && !selected(&readmitted, "refs/heads/release/no"));
    assert!(selected(&readmitted, "refs/heads/main"));
    assert!(!selected(&patterns(&["["]), "refs/heads/["), "invalid patterns admit nothing");
}

#[test]
fn names_and_selected_patterns_are_checked_first() {
    assert_eq!(check_name_and_policy("production", RefPolicy::All), Ok(()));
    assert!(check_name_and_policy("a/b", RefPolicy::All).unwrap_err().starts_with("an environment name is 1 to 255"));
    assert_eq!(check_name_and_policy("p", RefPolicy::Selected(&[])), Err("choose 1 to 50 branch or tag patterns".to_owned()));
    let many: Vec<String> = (0..51).map(|n| format!("b{n}")).collect();
    assert_eq!(check_name_and_policy("p", RefPolicy::Selected(&many)), Err("choose 1 to 50 branch or tag patterns".to_owned()));
    let negative = patterns(&["!main"]);
    assert_eq!(
        check_name_and_policy("p", RefPolicy::Selected(&negative)),
        Err("at least one branch or tag pattern must not start with `!`".to_owned())
    );
    assert!(check_name_and_policy("p", RefPolicy::Selected(&patterns(&["["]))).unwrap_err().starts_with("branch pattern: "));
}

#[test]
fn bare_patterns_name_branches_and_full_ones_keep_their_kind() {
    assert_eq!(
        full_ref_patterns(&patterns(&["main", "!release/*", "refs/tags/v*", "!refs/heads/x", "refs/pull/1/merge"])),
        ["refs/heads/main", "!refs/heads/release/*", "refs/tags/v*", "!refs/heads/x", "refs/heads/refs/pull/1/merge"]
    );
}

#[test]
fn the_gate_reads_protection_only_for_protected_branches() {
    let rules = Rules::default();
    let gate = |policy: RefPolicy<'_>, reference: &str| run(DeploymentGate::allows(&rules, "production", policy, reference)).unwrap();
    let refused = |kind: &str, short: &str| {
        Err(format!("{kind} `{short}` is not allowed to deploy to production due to environment protection rules"))
    };
    assert_eq!(gate(RefPolicy::All, "refs/tags/v1"), Ok(()));
    assert_eq!(gate(RefPolicy::Protected, "refs/heads/main"), Ok(()));
    assert_eq!(gate(RefPolicy::Protected, "refs/heads/dev"), refused("branch", "dev"));
    assert_eq!(gate(RefPolicy::Protected, "refs/tags/main"), refused("tag", "main"));
    assert_eq!(gate(RefPolicy::Selected(&patterns(&["main"])), "refs/heads/main"), Ok(()));
    assert_eq!(gate(RefPolicy::Selected(&patterns(&["main"])), "refs/pull/3/head"), refused("tag", "refs/pull/3/head"));
    assert_eq!(*rules.0.lock().unwrap(), ["protected main", "protected dev"], "tags and other policies never read protection");
}

#[test]
fn reviewers_must_read_and_may_not_approve_their_own_run_when_prevented() {
    let reviewer = |reads, listed| Reviewer { reads, listed };
    assert_eq!(may_review("prod", reviewer(true, true), false, true), Ok(()));
    assert_eq!(may_review("prod", reviewer(false, true), false, false), Err(ReviewRefusal::NotReviewer));
    assert_eq!(may_review("prod", reviewer(true, false), true, true), Err(ReviewRefusal::NotReviewer), "not a reviewer comes first");
    assert_eq!(
        may_review("prod", reviewer(true, true), true, true),
        Err(ReviewRefusal::SelfReview("prod does not let whoever started a run approve it".to_owned()))
    );
    assert_eq!(may_review("prod", reviewer(true, true), true, false), Ok(()));
}

#[test]
fn bounds() {
    assert_eq!((check_reviewer_count(6), check_reviewer_count(7)), (Ok(()), Err("an environment has at most 6 reviewers".to_owned())));
    assert_eq!((check_wait_timer(43_200), check_wait_timer(43_201)), (Ok(()), Err("the wait timer is at most 43200 minutes".to_owned())));
}
