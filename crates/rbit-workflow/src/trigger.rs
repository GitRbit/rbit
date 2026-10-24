//! Does an event start a workflow? Forgejo's `detectMatched` rules with GitHub's pattern
//! dialect (see [`crate::pattern`]):
//!
//! * `push` — every filter given must pass; when both a branch and a tag filter are
//!   given, the one that applies to the ref decides; a branch filter alone never matches a
//!   tag (and the reverse); `paths` filters are skipped for tags; deletions, and refs that
//!   are neither branches nor tags, never match.
//! * `pull_request` — the activity type must be listed (`opened`, `reopened` and
//!   `synchronize` by default); `branches` match the base branch, `paths` the pull request's
//!   changed files.
//! * `workflow_dispatch` and `schedule` — the workflow declares them.
//!
//! Changed paths are asked for only when a `paths` filter needs them (see
//! [`ChangedPaths`]); over the caller's cap, `paths` filters count as passed, as on GitHub.
//! Dispatch inputs are checked against the workflow's `inputs` by [`resolve_inputs`].

use std::collections::BTreeMap;

use crate::model::{ActivityType, Filter, InputKind, PushFilters, Workflow};

/// Markers in a head commit message (push) or a pull request title that skip its runs.
pub const SKIP_MARKERS: [&str; 5] = ["[skip ci]", "[ci skip]", "[no ci]", "[skip actions]", "[actions skip]"];
/// Most inputs one dispatch may set.
pub const MAX_DISPATCH_INPUTS_SET: usize = 10;
/// Longest dispatch input value, in bytes.
pub const MAX_DISPATCH_VALUE_BYTES: usize = 1024;

/// Whether `text` asks for its runs to be skipped.
pub fn has_skip_marker(text: &str) -> bool {
    SKIP_MARKERS.iter().any(|marker| text.contains(marker))
}

/// An event that might start a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    /// One updated ref of a push.
    Push {
        /// The full ref name: `refs/heads/main`, `refs/tags/v1.0`.
        reference: &'a str,
        /// The ref was deleted.
        deleted: bool,
    },
    PullRequest {
        action: &'a ActivityType,
        /// The base branch's short name.
        base_branch: &'a str,
    },
    Dispatch,
    /// A tick of one of the workflow's `schedule` entries, by its expression as written.
    Schedule {
        cron: &'a str,
    },
}

/// The paths an event changed, computed only when a filter needs them.
pub trait ChangedPaths {
    /// The changed paths, or `None` when there are too many to list (then `paths` filters
    /// pass).
    fn changed_paths(&mut self) -> Option<&[String]>;
}

/// [`ChangedPaths`] computed once, on first use, by a closure.
#[derive(Debug)]
pub struct LazyPaths<F> {
    /// Taken on first use.
    compute: Option<F>,
    /// `None` before the first use, and when there were too many paths.
    paths: Option<Vec<String>>,
}

impl<F: FnOnce() -> Option<Vec<String>>> LazyPaths<F> {
    pub fn new(compute: F) -> Self {
        Self { compute: Some(compute), paths: None }
    }

    /// Whether the paths were computed.
    pub fn was_computed(&self) -> bool {
        self.compute.is_none()
    }
}

impl<F: FnOnce() -> Option<Vec<String>>> ChangedPaths for LazyPaths<F> {
    fn changed_paths(&mut self) -> Option<&[String]> {
        if let Some(compute) = self.compute.take() {
            self.paths = compute();
        }
        self.paths.as_deref()
    }
}

/// A fixed list of changed paths (`None`: too many).
impl ChangedPaths for Option<Vec<String>> {
    fn changed_paths(&mut self) -> Option<&[String]> {
        self.as_deref()
    }
}

/// Whether `event` starts `workflow`.
pub fn matches(workflow: &Workflow, event: &Event<'_>, changed: &mut dyn ChangedPaths) -> bool {
    match *event {
        Event::Push { reference, deleted } => {
            !deleted && workflow.on.push.as_ref().is_some_and(|filters| push_matches(filters, reference, changed))
        }
        Event::PullRequest { action, base_branch } => workflow.on.pull_request.as_ref().is_some_and(|filters| {
            let listed = match &filters.types {
                Some(types) => types.contains(action),
                None => ActivityType::DEFAULT.contains(action),
            };
            listed && filters.branches.passes([base_branch]) && paths_pass(&filters.paths, changed)
        }),
        Event::Dispatch => workflow.on.workflow_dispatch.is_some(),
        Event::Schedule { cron } => workflow.on.schedule.iter().any(|entry| entry.cron.as_str() == cron),
    }
}

fn push_matches(filters: &PushFilters, reference: &str, changed: &mut dyn ChangedPaths) -> bool {
    let branch = reference.strip_prefix("refs/heads/");
    let tag = reference.strip_prefix("refs/tags/");
    let ref_filter = match (filters.branches.is_absent(), filters.tags.is_absent()) {
        (true, true) => branch.is_some() || tag.is_some(),
        // Only branch filters: a tag never matches (Forgejo's `hasBranchFilter`), and the
        // reverse.
        (false, true) => branch.is_some_and(|name| filters.branches.passes([name])),
        (true, false) => tag.is_some_and(|name| filters.tags.passes([name])),
        // Both: the filter for the ref's kind decides.
        (false, false) => match (branch, tag) {
            (Some(name), _) => filters.branches.passes([name]),
            (_, Some(name)) => filters.tags.passes([name]),
            _ => false,
        },
    };
    // Tags have no changed paths; their pushes skip `paths` filters.
    ref_filter && (tag.is_some() || paths_pass(&filters.paths, changed))
}

fn paths_pass(filter: &Filter, changed: &mut dyn ChangedPaths) -> bool {
    if filter.is_absent() {
        return true;
    }
    changed.changed_paths().is_none_or(|paths| filter.passes(paths))
}

/// A checked `workflow_dispatch` input value.
#[derive(Debug, Clone, PartialEq)]
pub enum InputValue {
    String(String),
    Boolean(bool),
    Number(f64),
}

impl InputValue {
    /// As GitHub puts it in `github.event.inputs`: always a string.
    pub fn to_text(&self) -> String {
        match self {
            Self::String(text) => text.clone(),
            Self::Boolean(value) => value.to_string(),
            Self::Number(value) => crate::yaml::format_float(*value),
        }
    }
}

/// Checks the inputs given to a dispatch against the workflow's `inputs` and fills in the
/// defaults. Every message is safe to show to the person dispatching.
pub fn resolve_inputs(workflow: &Workflow, given: &BTreeMap<String, String>) -> Result<BTreeMap<String, InputValue>, String> {
    let Some(dispatch) = &workflow.on.workflow_dispatch else {
        return Err("this workflow does not run on workflow_dispatch".into());
    };
    if given.len() > MAX_DISPATCH_INPUTS_SET {
        return Err(format!("at most {MAX_DISPATCH_INPUTS_SET} inputs can be set"));
    }
    if let Some(unknown) = given.keys().find(|name| !dispatch.inputs.iter().any(|input| input.name == **name)) {
        return Err(format!("the workflow has no input `{unknown}`"));
    }
    let mut resolved = BTreeMap::new();
    for input in &dispatch.inputs {
        let value = match given.get(&input.name).filter(|value| !value.is_empty()).or(input.default.as_ref()) {
            Some(value) => value,
            None if input.required => return Err(format!("the input `{}` is required", input.name)),
            None if input.kind == InputKind::Boolean => {
                resolved.insert(input.name.clone(), InputValue::Boolean(false));
                continue;
            }
            None => {
                resolved.insert(input.name.clone(), InputValue::String(String::new()));
                continue;
            }
        };
        if value.len() > MAX_DISPATCH_VALUE_BYTES {
            return Err(format!("the input `{}` must be at most {MAX_DISPATCH_VALUE_BYTES} bytes", input.name));
        }
        let checked = match &input.kind {
            InputKind::String | InputKind::Environment => InputValue::String(value.clone()),
            InputKind::Boolean => match value.as_str() {
                "true" => InputValue::Boolean(true),
                "false" => InputValue::Boolean(false),
                _ => return Err(format!("the input `{}` must be true or false", input.name)),
            },
            InputKind::Number => match value.trim().parse::<f64>() {
                Ok(number) if number.is_finite() => InputValue::Number(number),
                _ => return Err(format!("the input `{}` must be a number", input.name)),
            },
            InputKind::Choice(options) => {
                if !options.contains(value) {
                    return Err(format!("the input `{}` must be one of: {}", input.name, options.join(", ")));
                }
                InputValue::String(value.clone())
            }
        };
        resolved.insert(input.name.clone(), checked);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Deadline, Limits, parse};

    fn workflow(on: &str) -> Workflow {
        let source = format!("on:\n{on}\njobs:\n  a:\n    runs-on: x\n");
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
        assert!(resolve_inputs(&workflow, &many).unwrap_err().contains("at most 10"));
        assert!(resolve_inputs(&self::workflow("  push:"), &given(&[])).is_err());
    }
}
