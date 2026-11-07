//! Existing trigger acceptance fixtures, separated from production logic.
use super::{
    ActivityType, BTreeMap, Event, InputValue, LazyPaths, MAX_DISPATCH_INPUTS_SET, Workflow, has_skip_marker, matches, resolve_inputs,
};

use crate::model::WorkflowRunType;
use crate::{Deadline, Limits, parse};

fn workflow(on: &str) -> Workflow {
    let source = format!("on:\n{on}\njobs:\n  a:\n    runs-on: x\n    steps: [{{run: x}}]\n");
    parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
}

fn push(workflow: &Workflow, reference: &str, paths: &[&str]) -> bool {
    let mut changed = Some(paths.iter().map(|path| (*path).to_owned()).collect::<Vec<_>>());
    matches(workflow, &Event::Push { reference, deleted: false }, &mut changed)
}

const MAIN: &str = "refs/heads/main";
const FEATURE: &str = "refs/heads/feature/x";
const TAG: &str = "refs/tags/v1.2.0";

#[test]
fn push_filter_matrix() {
    // (on.push, ref, changed paths, expected)
    let cases: &[(&str, &str, &[&str], bool)] = &[
        ("  push:", MAIN, &[], true),
        ("  push:", TAG, &[], true),
        ("  push:", "refs/notes/commits", &[], false),
        ("  push:\n    branches: [main]", MAIN, &[], true),
        ("  push:\n    branches: [main]", FEATURE, &[], false),
        ("  push:\n    branches: [main]", TAG, &[], false),
        ("  push:\n    branches-ignore: [main]", MAIN, &[], false),
        ("  push:\n    branches-ignore: [main]", FEATURE, &[], true),
        ("  push:\n    branches-ignore: [main]", TAG, &[], false),
        ("  push:\n    tags: ['v*']", TAG, &[], true),
        ("  push:\n    tags: ['v*']", MAIN, &[], false),
        ("  push:\n    tags: ['v2*']", TAG, &[], false),
        ("  push:\n    tags-ignore: ['v1.*']", TAG, &[], false),
        ("  push:\n    tags-ignore: ['v2.*']", TAG, &[], true),
        ("  push:\n    tags-ignore: ['v2.*']", MAIN, &[], false),
        // Branch plus tag filters: the one for the ref's kind decides.
        ("  push:\n    branches: [main]\n    tags: ['v*']", MAIN, &[], true),
        ("  push:\n    branches: [main]\n    tags: ['v*']", TAG, &[], true),
        ("  push:\n    branches: [main]\n    tags: ['v9*']", TAG, &[], false),
        ("  push:\n    branches: [dev]\n    tags: ['v*']", MAIN, &[], false),
        ("  push:\n    branches-ignore: [main]\n    tags: ['v*']", FEATURE, &[], true),
        // Paths: skipped for tags, required for branches.
        ("  push:\n    paths: ['src/**']", MAIN, &["src/lib.rs"], true),
        ("  push:\n    paths: ['src/**']", MAIN, &["docs/a.md"], false),
        ("  push:\n    paths: ['src/**']", MAIN, &[], false),
        ("  push:\n    paths: ['src/**']", TAG, &[], true),
        ("  push:\n    paths-ignore: ['docs/**']", MAIN, &["docs/a.md"], false),
        ("  push:\n    paths-ignore: ['docs/**']", MAIN, &["docs/a.md", "src/b.rs"], true),
        ("  push:\n    paths-ignore: ['docs/**']", MAIN, &[], false),
        ("  push:\n    paths-ignore: ['docs/**']", TAG, &[], true),
        ("  push:\n    tags: ['v*']\n    paths: ['src/**']", TAG, &[], true),
        ("  push:\n    tags: ['v*']\n    paths: ['src/**']", MAIN, &["src/a"], false),
        ("  push:\n    branches: [main]\n    paths: ['src/**']", MAIN, &["src/a"], true),
        ("  push:\n    branches: [main]\n    paths: ['src/**']", MAIN, &["b"], false),
        ("  push:\n    branches: [main]\n    paths: ['src/**']", FEATURE, &["src/a"], false),
        ("  push:\n    branches: [main]\n    tags: ['v*']\n    paths: ['src/**']", MAIN, &["b"], false),
        ("  push:\n    branches: [main]\n    tags: ['v*']\n    paths: ['src/**']", TAG, &["b"], true),
        ("  push:\n    branches: ['**', '!main']", MAIN, &[], false),
        ("  push:\n    branches: ['**', '!main']", FEATURE, &[], true),
        ("  pull_request:", MAIN, &[], false),
    ];
    for (on, reference, paths, expected) in cases {
        assert_eq!(push(&workflow(on), reference, paths), *expected, "{on} / {reference} / {paths:?}");
    }
}

#[test]
fn deletions_never_match() {
    let workflow = workflow("  push:");
    let mut changed = None;
    assert!(!matches(&workflow, &Event::Push { reference: MAIN, deleted: true }, &mut changed));
}

#[test]
fn changed_paths_are_computed_only_when_needed() {
    let unfiltered = workflow("  push:\n    branches: [main]");
    let mut lazy = LazyPaths::new(|| Some(vec!["a".to_owned()]));
    assert!(matches(&unfiltered, &Event::Push { reference: MAIN, deleted: false }, &mut lazy));
    assert!(!lazy.was_computed());
    let filtered = workflow("  push:\n    paths: [a]");
    assert!(matches(&filtered, &Event::Push { reference: TAG, deleted: false }, &mut lazy));
    assert!(!lazy.was_computed(), "tags skip paths");
    assert!(matches(&filtered, &Event::Push { reference: MAIN, deleted: false }, &mut lazy));
    assert!(lazy.was_computed());
    // Computed once, reused by the next workflow.
    assert!(!matches(&workflow("  push:\n    paths-ignore: [a]"), &Event::Push { reference: MAIN, deleted: false }, &mut lazy));
    // Too many to list: paths filters pass.
    let mut too_many = LazyPaths::new(|| None);
    assert!(matches(&filtered, &Event::Push { reference: MAIN, deleted: false }, &mut too_many));
    assert!(matches(&workflow("  push:\n    paths-ignore: ['**']"), &Event::Push { reference: MAIN, deleted: false }, &mut too_many));
}

#[test]
fn pull_request_matrix() {
    use ActivityType::{Closed, Edited, Opened, ReadyForReview, Reopened, Synchronize};
    let cases: &[(&str, ActivityType, &str, &[&str], bool)] = &[
        ("  pull_request:", Opened, "main", &[], true),
        ("  pull_request:", Reopened, "main", &[], true),
        ("  pull_request:", Synchronize, "main", &[], true),
        ("  pull_request:", Closed, "main", &[], false),
        ("  pull_request:", Edited, "main", &[], false),
        ("  pull_request:", ReadyForReview, "main", &[], false),
        ("  pull_request:\n    types: [closed]", Closed, "main", &[], true),
        ("  pull_request:\n    types: [closed]", Opened, "main", &[], false),
        ("  pull_request:\n    types: [edited, ready_for_review]", Edited, "main", &[], true),
        ("  pull_request:\n    types: [edited, ready_for_review]", ReadyForReview, "main", &[], true),
        ("  pull_request:\n    types: [labeled]", Opened, "main", &[], false),
        ("  pull_request:\n    branches: [main]", Opened, "main", &[], true),
        ("  pull_request:\n    branches: [main]", Opened, "dev", &[], false),
        ("  pull_request:\n    branches: ['releases/**']", Opened, "releases/1/x", &[], true),
        ("  pull_request:\n    branches-ignore: [main]", Opened, "main", &[], false),
        ("  pull_request:\n    branches-ignore: [main]", Opened, "dev", &[], true),
        ("  pull_request:\n    paths: ['src/**']", Opened, "main", &["src/a"], true),
        ("  pull_request:\n    paths: ['src/**']", Opened, "main", &["b"], false),
        ("  pull_request:\n    paths-ignore: ['src/**']", Opened, "main", &["src/a"], false),
        ("  pull_request:\n    types: [closed]\n    branches: [main]\n    paths: [a]", Closed, "main", &["a"], true),
        ("  pull_request:\n    types: [closed]\n    branches: [main]\n    paths: [a]", Closed, "dev", &["a"], false),
        ("  push:", Opened, "main", &[], false),
    ];
    for (on, action, base, paths, expected) in cases {
        let mut changed = Some(paths.iter().map(|path| (*path).to_owned()).collect::<Vec<_>>());
        let event = Event::PullRequest { action, base_branch: base };
        assert_eq!(matches(&workflow(on), &event, &mut changed), *expected, "{on} / {action:?} / {base} / {paths:?}");
    }
}

#[test]
fn dispatch_and_schedule() {
    let mut none = None;
    let both = workflow("  workflow_dispatch:\n  schedule:\n    - cron: '0 3 * * *'");
    assert!(matches(&both, &Event::Dispatch, &mut none));
    assert!(matches(&both, &Event::Schedule { cron: "0 3 * * *" }, &mut none));
    assert!(!matches(&both, &Event::Schedule { cron: "0 4 * * *" }, &mut none));
    let push_only = workflow("  push:");
    assert!(!matches(&push_only, &Event::Dispatch, &mut none));
    assert!(!matches(&push_only, &Event::Schedule { cron: "0 3 * * *" }, &mut none));
}

#[test]
fn skip_markers() {
    for text in ["fix typo [skip ci]", "[ci skip] docs", "x [no ci]", "[skip actions]", "wip [actions skip] more"] {
        assert!(has_skip_marker(text), "{text}");
    }
    for text in ["skip ci", "[skip-ci]", "[Skip CI]", "", "[ci]"] {
        assert!(!has_skip_marker(text), "{text}");
    }
}

#[test]
fn dispatch_inputs() {
    let workflow = workflow(
        "  workflow_dispatch:
    inputs:
      level: {type: choice, options: [low, high], default: low}
      dry-run: {type: boolean}
      count: {type: number, required: true}
      note: {}
      env: {type: environment}",
    );
    let given = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<BTreeMap<_, _>>();
    let resolved = resolve_inputs(&workflow, &given(&[("count", "3"), ("dry-run", "true")])).unwrap();
    assert_eq!(resolved["level"], InputValue::String("low".into()));
    assert_eq!(resolved["dry-run"], InputValue::Boolean(true));
    assert_eq!(resolved["count"], InputValue::Number(3.0));
    assert_eq!(resolved["note"], InputValue::String(String::new()));
    assert_eq!(resolved["count"].to_text(), "3");
    let defaults = resolve_inputs(&workflow, &given(&[("count", "1.5")])).unwrap();
    assert_eq!(defaults["dry-run"], InputValue::Boolean(false));
    assert_eq!(defaults["count"].to_text(), "1.5");
    let errors = [
        (given(&[]), "`count` is required"),
        (given(&[("count", "")]), "`count` is required"),
        (given(&[("count", "x")]), "must be a number"),
        (given(&[("count", "inf")]), "must be a number"),
        (given(&[("count", "1"), ("dry-run", "yes")]), "true or false"),
        (given(&[("count", "1"), ("level", "mid")]), "one of: low, high"),
        (given(&[("count", "1"), ("other", "x")]), "no input `other`"),
        (given(&[("count", "1"), ("note", &"x".repeat(1025))]), "at most 1024 bytes"),
    ];
    for (inputs, message) in errors {
        let error = resolve_inputs(&workflow, &inputs).unwrap_err();
        assert!(error.contains(message), "{inputs:?}: {error}");
    }
    let many: BTreeMap<_, _> = (0..=MAX_DISPATCH_INPUTS_SET).map(|index| (format!("i{index}"), String::new())).collect();
    assert!(resolve_inputs(&workflow, &many).unwrap_err().contains("at most 25"));
    // The dispatch form sends every input, set or not.
    let twelve = (0..12).map(|index| format!("      i{index}: {{}}\n")).collect::<Vec<_>>().concat();
    let declared = self::workflow(&format!("  workflow_dispatch:\n    inputs:\n{twelve}"));
    let sent: BTreeMap<_, _> = (0..12).map(|index| (format!("i{index}"), String::new())).collect();
    assert_eq!(resolve_inputs(&declared, &sent).unwrap().len(), 12);
    assert!(resolve_inputs(&self::workflow("  push:"), &given(&[])).is_err());
}

// Actions: workflow_run and repository_dispatch.
#[test]
fn workflow_run_matches_name_type_and_branch() {
    use WorkflowRunType::{Completed, InProgress, Requested};
    let run = |workflow: &Workflow, name: &str, action, branch: Option<&str>| {
        matches(workflow, &Event::WorkflowRun { workflow: name, action, branch }, &mut None::<Vec<String>>)
    };
    let any = workflow(
        "  workflow_run:
    workflows: [CI, Build]",
    );
    for action in [Requested, InProgress, Completed] {
        assert!(run(&any, "CI", action, Some("main")), "all types when none are listed");
    }
    assert!(run(&any, "Build", Completed, None) && !run(&any, "ci", Completed, Some("main")), "names match exactly");
    let completed = workflow(
        "  workflow_run:
    workflows: [CI]
    types: [completed]
    branches: ['release/**']",
    );
    assert!(run(&completed, "CI", Completed, Some("release/1.0")));
    assert!(!run(&completed, "CI", Requested, Some("release/1.0")));
    assert!(!run(&completed, "CI", Completed, Some("main")) && !run(&completed, "CI", Completed, None));
    let ignore = workflow(
        "  workflow_run:
    workflows: [CI]
    branches-ignore: [main]",
    );
    assert!(!run(&ignore, "CI", Completed, Some("main")) && run(&ignore, "CI", Completed, Some("dev")));
    assert!(!run(&workflow("  push:"), "CI", Completed, Some("main")), "only workflows that ask for it");
}

#[test]
fn workflow_run_and_repository_dispatch_are_checked() {
    for (on, message) in [
        ("  workflow_run:", "needs `workflows`"),
        (
            "  workflow_run:
    types: [completed]",
            "needs `workflows`",
        ),
        (
            "  workflow_run:
    workflows: []",
            "names one or more workflows",
        ),
        (
            "  workflow_run:
    workflows: [CI]
    types: [finished]",
            "is not requested, in_progress or completed",
        ),
        (
            "  workflow_run:
    workflows: [CI]
    branches: [a]
    branches-ignore: [b]",
            "cannot have both",
        ),
        (
            "  workflow_run:
    workflows: [CI]
    paths: [a]",
            "paths",
        ),
        (
            "  repository_dispatch:
    events: [a]",
            "events",
        ),
    ] {
        let source = format!(
            "on:
{on}
jobs:
  a:
    runs-on: x
    steps: [{{run: x}}]
"
        );
        let error = parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap_err().to_string();
        assert!(error.contains(message), "{on}: {error}");
    }
}

#[test]
fn repository_dispatch_matches_its_types() {
    let dispatch = |workflow: &Workflow, event_type| matches(workflow, &Event::RepositoryDispatch { event_type }, &mut None::<Vec<String>>);
    let any = workflow("  repository_dispatch:");
    assert!(dispatch(&any, "deploy") && dispatch(&any, "anything"));
    let typed = workflow(
        "  repository_dispatch:
    types: [deploy, rollback]",
    );
    assert!(dispatch(&typed, "deploy") && dispatch(&typed, "rollback") && !dispatch(&typed, "Deploy"));
    assert!(!dispatch(&workflow("  push:"), "deploy"));
}
