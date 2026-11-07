//! Conformance eval for `rbit-workflow`.
//!
//! Every case in `tests/conformance/*.json` states an input and the outcome GitHub Actions
//! gives for it, and this file checks the crate against each case. Grading is programmatic:
//! no case is judged by a model. The case schema and the `kind`s are documented in
//! `tests/conformance/README.md`.
//!
//! Cases listed in `tests/conformance/known_failures.txt` are reported as `KNOWN` and do not
//! fail the run, so that list is the hill-climbing target: delete an id once its case
//! passes. A listed id whose case now passes is reported as `NOW PASSES`. Any other failure,
//! a malformed case file, or a known-failure id that matches no case fails the test.
//!
//! Run with `cargo test -p rbit-workflow --test conformance -- --nocapture` to see the
//! per-case report.

#[path = "conformance/cases.rs"]
mod cases;
#[path = "conformance/observation.rs"]
mod observation;
#[path = "conformance/report.rs"]
mod report;
#[path = "conformance/verdict.rs"]
mod verdict;

use self::cases::{load_cases, load_known};
use self::observation::observe;
use self::report::{describe, panic_message, percent};
use self::verdict::verdict;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

#[test]
fn conformance() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
    let mut problems = Vec::new();
    let cases = load_cases(&dir, &mut problems);
    let known = load_known(&dir, &mut problems);

    let ids: BTreeSet<&str> = cases.iter().map(|case| case.id.as_str()).collect();
    for id in &known {
        if !ids.contains(id.as_str()) {
            problems.push(format!("known_failures.txt lists `{id}`, which no case has"));
        }
    }

    // Each case runs under catch_unwind, so one panic is one failure and the rest still run.
    // The default hook would print every panic; the report below carries the message instead.
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut report = String::new();
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut unexpected = Vec::new();
    let mut known_failing = Vec::new();
    let mut now_passing = Vec::new();
    for case in &cases {
        let observed = panic::catch_unwind(AssertUnwindSafe(|| observe(case)));
        let actual = match observed {
            Ok(Ok(value)) => value,
            Ok(Err(message)) => json!({"error": message}),
            Err(payload) => json!({"error": format!("panicked: {}", panic_message(payload.as_ref()))}),
        };
        let passed = verdict(&case.expect, &actual);
        let entry = tally.entry(case.split.clone()).or_default();
        entry.1 += 1;
        if passed {
            entry.0 += 1;
        }
        match (passed, known.contains(&case.id)) {
            (true, false) => {}
            (true, true) => now_passing.push(case.id.clone()),
            (false, true) => {
                known_failing.push(case.id.clone());
                let _ = writeln!(report, "KNOWN {}", describe(case, &actual));
            }
            (false, false) => {
                unexpected.push(case.id.clone());
                let _ = writeln!(report, "FAIL {}", describe(case, &actual));
            }
        }
    }
    panic::set_hook(previous_hook);

    for id in &now_passing {
        let _ = writeln!(report, "NOW PASSES {id}: remove it from known_failures.txt");
    }
    for problem in &problems {
        let _ = writeln!(report, "PROBLEM {problem}");
    }

    let mut summary = String::new();
    for (split, (passed, total)) in &tally {
        let _ = writeln!(summary, "{split}: {passed}/{total} pass ({:.1}%)", percent(*passed, *total));
    }
    let _ = writeln!(
        summary,
        "total: {} cases, {} known failures, {} unexpected failures, {} problems",
        cases.len(),
        known_failing.len(),
        unexpected.len(),
        problems.len()
    );
    println!("{report}{summary}");

    assert!(unexpected.is_empty() && problems.is_empty(), "conformance failed:\n{report}{summary}");
}
