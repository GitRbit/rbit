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

/// `main` is protected; selected patterns admit exactly the refs they list. Every
/// question is logged.
#[derive(Default)]
struct Rules(Mutex<Vec<String>>);

impl RefRules for Rules {
    type Error = ();

    fn is_protected(&self, branch: &str) -> impl Future<Output = Result<bool, ()>> + Send {
        self.0.lock().unwrap().push(format!("protected {branch}"));
        ready(Ok(branch == "main"))
    }

    fn selects(&self, patterns: &[String], reference: &str) -> bool {
        self.0.lock().unwrap().push(format!("selects {}", patterns.join(",")));
        patterns.iter().any(|pattern| pattern == reference)
    }
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
    let gate = |policy: &BranchPolicy, reference: &str| run(DeploymentGate::allows(&rules, "production", policy, reference)).unwrap();
    let refused = |kind: &str, short: &str| {
        Err(format!("{kind} `{short}` is not allowed to deploy to production due to environment protection rules"))
    };
    assert_eq!(gate(&BranchPolicy::All, "refs/tags/v1"), Ok(()));
    assert_eq!(gate(&BranchPolicy::Protected, "refs/heads/main"), Ok(()));
    assert_eq!(gate(&BranchPolicy::Protected, "refs/heads/dev"), refused("branch", "dev"));
    assert_eq!(gate(&BranchPolicy::Protected, "refs/tags/main"), refused("tag", "main"));
    assert_eq!(gate(&BranchPolicy::Selected(patterns(&["main"])), "refs/heads/main"), Ok(()));
    assert_eq!(gate(&BranchPolicy::Selected(patterns(&["main"])), "refs/pull/3/head"), refused("tag", "refs/pull/3/head"));
    assert_eq!(
        *rules.0.lock().unwrap(),
        ["protected main", "protected dev", "selects refs/heads/main", "selects refs/heads/main"],
        "tags and other policies never read protection"
    );
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
fn bounds_and_stored_policies() {
    assert_eq!((check_reviewer_count(6), check_reviewer_count(7)), (Ok(()), Err("an environment has at most 6 reviewers".to_owned())));
    assert_eq!((check_wait_timer(43_200), check_wait_timer(43_201)), (Ok(()), Err("the wait timer is at most 43200 minutes".to_owned())));
    let policies = [BranchPolicy::All, BranchPolicy::Protected, BranchPolicy::Selected(Vec::new())];
    assert_eq!(policies.map(|policy| policy.as_str()), ["all", "protected", "selected"]);
    assert_eq!(BranchPolicy::default(), BranchPolicy::All);
}
