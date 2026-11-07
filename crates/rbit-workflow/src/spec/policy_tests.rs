//! Exact spec policy tests acceptance fixtures.

use super::{ActionSource, ActionsPolicy, ActionsRule};

fn repository(owner: &str, repo: &str, path: &str, reference: &str) -> ActionSource {
    ActionSource::Repository { owner: owner.into(), repo: repo.into(), path: path.into(), reference: reference.into() }
}

#[test]
fn every_rule_must_allow_an_action() {
    let open = ActionsPolicy::default();
    assert!(open.is_open() && open.refuses(&repository("evil", "x", "", "v1")).is_none());
    let local = ActionsPolicy { rules: vec![ActionsRule::LocalOnly] };
    assert!(local.refuses(&ActionSource::Local { path: "x".into() }).is_none(), "the workspace's own always run");
    assert!(local.refuses(&repository("actions", "setup-node", "", "v4")).is_some());
    assert!(local.refuses(&ActionSource::Docker { image: "alpine".into() }).is_some());

    let selected = ActionsRule::Selected {
        github_owned: true,
        patterns: vec!["octo-org/*".into(), "Deploy/Tool@V2".into(), "one/repo/sub@*".into(), "docker://alpine:*".into()],
    };
    let policy = ActionsPolicy { rules: vec![selected.clone()] };
    for allowed in [
        repository("actions", "setup-node", "", "v4"),
        repository("github", "codeql-action", "init", "v3"),
        repository("octo-org", "anything", "deep/path", "main"),
        repository("deploy", "tool", "", "v2"),
        repository("one", "repo", "sub", "abc123"),
    ] {
        assert!(policy.refuses(&allowed).is_none(), "{allowed:?}");
    }
    for refused in [repository("deploy", "tool", "", "v3"), repository("one", "repo", "", "v1"), repository("evil", "x", "", "v1")] {
        assert!(policy.refuses(&refused).is_some(), "{refused:?}");
    }
    assert!(policy.refuses(&ActionSource::Docker { image: "alpine:3.20".into() }).is_none());
    assert!(policy.refuses(&ActionSource::Docker { image: "ubuntu".into() }).is_some());

    let both = ActionsPolicy { rules: vec![selected, ActionsRule::Selected { github_owned: false, patterns: vec!["octo-org/*".into()] }] };
    assert!(both.refuses(&repository("actions", "setup-node", "", "v4")).is_some(), "the owner's rule refuses it");
    assert!(both.refuses(&repository("octo-org", "x", "", "v1")).is_none());
}

#[test]
fn a_restricted_spec_names_its_policy_and_an_open_one_does_not() {
    let rule = serde_json::to_string(&ActionsPolicy { rules: vec![ActionsRule::LocalOnly] }).unwrap();
    assert_eq!(rule, r#"{"rules":["local_only"]}"#);
}
