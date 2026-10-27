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

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use rbit_workflow::cron::Cron;
use rbit_workflow::expr::{self, Condition, ContextMap, Contexts, Status};
use rbit_workflow::model::ActivityType;
use rbit_workflow::pattern::Patterns;
use rbit_workflow::plan::{self, RunContext};
use rbit_workflow::spec::JobResult;
use rbit_workflow::trigger::{self, ChangedPaths, Event};
use rbit_workflow::{Deadline, Limits, Workflow};
use serde_json::{Map, Value, json};

/// Every `kind` a case may use, and what it drives. See the README for each input and output.
const KINDS: &[&str] = &[
    "expr",
    "condition",
    "render",
    "needs_status",
    "parse",
    "plan",
    "run_name",
    "concurrency",
    "trigger",
    "dispatch",
    "pattern",
    "skip_marker",
    "cron",
];

/// One conformance case, validated.
#[derive(Debug)]
struct Case {
    id: String,
    file: String,
    area: String,
    tags: Vec<String>,
    why_hard: Option<String>,
    source: String,
    split: String,
    kind: String,
    input: Value,
    expect: Value,
}

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

/// Whether `actual` satisfies `expect`. `{"error": true}` matches any error; a `jobs` list
/// matches its planned jobs in any order, each by subset; everything else is equal, with
/// numbers compared as numbers.
fn verdict(expect: &Value, actual: &Value) -> bool {
    if expect.as_object().is_some_and(|object| object.len() == 1) && expect.get("error") == Some(&Value::Bool(true)) {
        return actual.get("error").is_some();
    }
    if let (Some(expected), Some(got)) = (expect.get("jobs").and_then(Value::as_array), actual.get("jobs").and_then(Value::as_array)) {
        return expected.len() == got.len() && jobs_match(expected, got);
    }
    json_eq(expect, actual)
}

/// A planned job matches a listed one when every listed field is present and equal, and its
/// `matrix` is exactly the listed combination: an extra matrix key is a different job.
fn jobs_match(expected: &[Value], actual: &[Value]) -> bool {
    let mut used = vec![false; actual.len()];
    let matches = |wanted: &Value, job: &Value| {
        subset(wanted, job) && wanted.get("matrix").is_none_or(|matrix| json_eq(matrix, job.get("matrix").unwrap_or(&Value::Null)))
    };
    expected.iter().all(|wanted| match actual.iter().enumerate().find(|(index, job)| !used[*index] && matches(wanted, job)) {
        Some((index, _)) => {
            used[index] = true;
            true
        }
        None => false,
    })
}

/// Every key of `expected` appears in `actual` with a matching value; a missing key reads as
/// `null`.
fn subset(expected: &Value, actual: &Value) -> bool {
    match expected {
        Value::Object(object) => object.iter().all(|(key, value)| subset(value, actual.get(key).unwrap_or(&Value::Null))),
        _ => json_eq(expected, actual),
    }
}

fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_eq(p, q)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(key, value)| y.get(key).is_some_and(|other| json_eq(value, other)))
        }
        _ => a == b,
    }
}

fn describe(case: &Case, actual: &Value) -> String {
    let mut text = format!("{} [{}, {}] {} area={} tags={}", case.id, case.split, case.kind, case.file, case.area, case.tags.join(","));
    if let Some(why) = &case.why_hard {
        let _ = write!(text, "\n    why hard: {why}");
    }
    let _ = write!(
        text,
        "\n    input:    {}\n    expected: {}\n    actual:   {}\n    rule:     {}\n",
        case.input, case.expect, actual, case.source
    );
    text
}

/// The share of `passed` in `total`, as a percentage.
#[allow(clippy::cast_precision_loss, reason = "case counts are far below 2^52")]
fn percent(passed: usize, total: usize) -> f64 {
    100.0 * passed as f64 / total as f64
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".into())
}

/// Runs one case's input through the crate and returns what it produced. An `Err` is an
/// evaluation or load error, reported as `{"error": message}`.
fn observe(case: &Case) -> Result<Value, String> {
    let input = &case.input;
    match case.kind.as_str() {
        "expr" => {
            let context = contexts(input)?;
            Ok(json!({"value": expr::evaluate(str_of(input, "template")?, &context).map_err(|e| e.to_string())?}))
        }
        "condition" => {
            let context = contexts(input)?;
            let condition = Condition::parse(str_of(input, "condition")?).map_err(|e| e.to_string())?;
            Ok(json!({"value": condition.evaluate(&context).map_err(|e| e.to_string())?}))
        }
        "render" => {
            let context = contexts(input)?;
            Ok(json!({"value": expr::render(str_of(input, "template")?, &context).map_err(|e| e.to_string())?}))
        }
        "needs_status" => {
            let results = string_list(input.get("results")).iter().map(|name| job_result(name)).collect::<Result<Vec<_>, _>>()?;
            let status = Status::of_needs(results);
            Ok(json!({"value": format!("{status:?}").to_lowercase()}))
        }
        "parse" => {
            workflow(input)?;
            Ok(json!({"ok": true}))
        }
        "plan" => {
            let workflow = workflow(input)?;
            let jobs = plan::plan(&workflow, &run_context(input), &Limits::default(), &Deadline::NONE).map_err(|e| e.to_string())?;
            let jobs: Vec<Value> = jobs.iter().map(|job| serde_json::to_value(job).expect("a job template serializes")).collect();
            Ok(json!({"jobs": jobs}))
        }
        "run_name" => {
            let workflow = workflow(input)?;
            let name = plan::run_name(&workflow, &run_context(input)).map_err(|e| e.to_string())?;
            Ok(json!({"value": name}))
        }
        "concurrency" => {
            let workflow = workflow(input)?;
            let group = plan::concurrency_group(&workflow, &run_context(input)).map_err(|e| e.to_string())?;
            let value = match group {
                Some((group, cancel)) => json!({"group": group, "cancel_in_progress": cancel}),
                None => Value::Null,
            };
            Ok(json!({"value": value}))
        }
        "trigger" => {
            let workflow = workflow(input)?;
            let event = input.get("event").ok_or("a trigger case needs an `event`")?;
            // Changed paths are only consulted by `paths` filters; a case that needs them
            // lists them, and one that does not is given none.
            let mut paths: Option<Vec<String>> = Some(string_list(input.get("changed_paths")));
            let starts = match str_of(event, "type")? {
                "push" => {
                    let deleted = event.get("deleted").and_then(Value::as_bool).unwrap_or(false);
                    let event = Event::Push { reference: str_of(event, "ref")?, deleted };
                    trigger::matches(&workflow, &event, &mut paths as &mut dyn ChangedPaths)
                }
                "pull_request" => {
                    let action = activity(str_of(event, "action")?);
                    let event = Event::PullRequest { action: &action, base_branch: str_of(event, "base")? };
                    trigger::matches(&workflow, &event, &mut paths as &mut dyn ChangedPaths)
                }
                "dispatch" => trigger::matches(&workflow, &Event::Dispatch, &mut paths as &mut dyn ChangedPaths),
                "schedule" => {
                    let event = Event::Schedule { cron: str_of(event, "cron")? };
                    trigger::matches(&workflow, &event, &mut paths as &mut dyn ChangedPaths)
                }
                other => return Err(format!("unknown event type `{other}`")),
            };
            Ok(json!({"value": starts}))
        }
        "dispatch" => {
            let workflow = workflow(input)?;
            let given: BTreeMap<String, String> = input
                .get("given")
                .and_then(Value::as_object)
                .map(|object| object.iter().map(|(name, value)| (name.clone(), text_of(value))).collect())
                .unwrap_or_default();
            let values = trigger::resolve_inputs(&workflow, &given)?;
            let values: Map<String, Value> = values.iter().map(|(name, value)| (name.clone(), Value::String(value.to_text()))).collect();
            Ok(json!({"value": values}))
        }
        "pattern" => {
            let patterns = Patterns::compile(string_list(input.get("patterns"))).map_err(|e| format!("{e:?}"))?;
            let inputs = string_list(input.get("inputs"));
            // `passes`: some input is included (`include`) or not every input is ignored
            // (`ignore`).
            let passes = match str_of(input, "mode")? {
                "include" => !patterns.skip(inputs),
                "ignore" => !patterns.filter(inputs),
                other => return Err(format!("unknown mode `{other}`")),
            };
            Ok(json!({"value": passes}))
        }
        "skip_marker" => Ok(json!({"value": trigger::has_skip_marker(str_of(input, "text")?)})),
        "cron" => {
            let cron = Cron::parse(str_of(input, "expression")?).map_err(|e| format!("{e:?}"))?;
            let next = input.get("after").and_then(Value::as_i64).and_then(|after| cron.next_after(after));
            Ok(json!({"value": {"too_frequent": cron.is_too_frequent(), "next": next}}))
        }
        other => Err(format!("unknown kind `{other}`")),
    }
}

/// The workflow a case's `yaml` (an array of lines, or one string) parses to.
fn workflow(input: &Value) -> Result<Workflow, String> {
    let text = match input.get("yaml") {
        Some(Value::Array(lines)) => lines.iter().map(text_of).collect::<Vec<_>>().join("\n") + "\n",
        Some(Value::String(text)) => text.clone(),
        _ => return Err("the case has no `yaml`".into()),
    };
    rbit_workflow::parse(text.as_bytes(), &Limits::default(), &Deadline::NONE).map_err(|e| e.to_string())
}

fn run_context(input: &Value) -> RunContext {
    let run = input.get("run");
    let object = |key: &str| run.and_then(|run| run.get(key)).and_then(Value::as_object).cloned().unwrap_or_default();
    let vars: BTreeMap<String, String> = object("vars").into_iter().map(|(name, value)| (name, text_of(&value))).collect();
    RunContext { github: object("github"), inputs: object("inputs"), vars }
}

/// The contexts of an expression case: `contexts` by name, plus an optional `status` for the
/// status functions and an optional `hash_files` result for `hashFiles`.
fn contexts(input: &Value) -> Result<Ctx, String> {
    let mut values = ContextMap::new();
    if let Some(object) = input.get("contexts").and_then(Value::as_object) {
        for (name, value) in object {
            values.insert(name, value.clone());
        }
    }
    let status = match input.get("status").and_then(Value::as_str) {
        None => None,
        Some("success") => Some(Status::Success),
        Some("failure") => Some(Status::Failure),
        Some("cancelled") => Some(Status::Cancelled),
        Some("skipped") => Some(Status::Skipped),
        Some(other) => return Err(format!("unknown status `{other}`")),
    };
    let hash = input.get("hash_files").and_then(Value::as_str).map(str::to_string);
    Ok(Ctx { values, status, hash })
}

/// [`Contexts`] for one expression case.
struct Ctx {
    values: ContextMap,
    status: Option<Status>,
    hash: Option<String>,
}

impl Contexts for Ctx {
    fn context(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    fn status(&self) -> Option<Status> {
        self.status
    }

    fn hash_files(&self, _patterns: &[String]) -> Option<Result<String, String>> {
        self.hash.clone().map(Ok)
    }
}

fn activity(name: &str) -> ActivityType {
    match name {
        "opened" => ActivityType::Opened,
        "reopened" => ActivityType::Reopened,
        "synchronize" => ActivityType::Synchronize,
        "closed" => ActivityType::Closed,
        "edited" => ActivityType::Edited,
        "ready_for_review" => ActivityType::ReadyForReview,
        other => ActivityType::Other(other.to_string()),
    }
}

fn job_result(name: &str) -> Result<JobResult, String> {
    match name {
        "success" => Ok(JobResult::Success),
        "failure" => Ok(JobResult::Failure),
        "cancelled" => Ok(JobResult::Cancelled),
        "skipped" => Ok(JobResult::Skipped),
        other => Err(format!("unknown job result `{other}`")),
    }
}

fn str_of<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| format!("the input needs a string `{key}`"))
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).map(|items| items.iter().map(text_of).collect()).unwrap_or_default()
}

/// A JSON value as the text a workflow would see it: strings as they are, anything else as
/// its JSON.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn load_cases(dir: &Path, problems: &mut Vec<String>) -> Vec<Case> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    files.sort();

    let mut cases = Vec::new();
    let mut seen = BTreeSet::new();
    for path in files {
        let file = path.file_name().expect("a file name").to_string_lossy().into_owned();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                problems.push(format!("{file}: unreadable: {error}"));
                continue;
            }
        };
        let document: Value = match serde_json::from_str(&text) {
            Ok(document) => document,
            Err(error) => {
                problems.push(format!("{file}: not valid JSON: {error}"));
                continue;
            }
        };
        let Some(raw_cases) = document.get("cases").and_then(Value::as_array) else {
            problems.push(format!("{file}: no `cases` array"));
            continue;
        };
        for raw in raw_cases {
            if let Some(case) = parse_case(raw, &file, problems) {
                if !seen.insert(case.id.clone()) {
                    problems.push(format!("{file}: duplicate case id `{}`", case.id));
                }
                cases.push(case);
            }
        }
    }
    cases
}

/// Reads one case and checks the schema: every field present, the tags known, a `why_hard`
/// on every hard case, and a split of `train` or `test`.
fn parse_case(raw: &Value, file: &str, problems: &mut Vec<String>) -> Option<Case> {
    let id = raw.get("id").and_then(Value::as_str).unwrap_or("<no id>").to_string();
    let mut fail = |why: &str| problems.push(format!("{file} {id}: {why}"));
    let text = |key: &str| raw.get(key).and_then(Value::as_str).map(str::to_string);

    let mut ok = true;
    if raw.get("id").and_then(Value::as_str).is_none() {
        fail("missing `id`");
        ok = false;
    }
    let tags = string_list(raw.get("tags"));
    if tags.is_empty() || tags.iter().any(|tag| !["easy", "hard", "edge"].contains(&tag.as_str())) {
        fail("`tags` must hold easy, hard or edge");
        ok = false;
    }
    let why_hard = text("why_hard").filter(|why| !why.trim().is_empty());
    if tags.iter().any(|tag| tag == "hard") && why_hard.is_none() {
        fail("a hard case needs `why_hard`");
        ok = false;
    }
    let source = text("source").filter(|source| !source.trim().is_empty());
    if source.is_none() {
        fail("missing `source` (the GitHub docs behaviour it relies on)");
        ok = false;
    }
    let split = text("split").unwrap_or_default();
    if split != "train" && split != "test" {
        fail("`split` must be train or test");
        ok = false;
    }
    let kind = text("kind").unwrap_or_default();
    if !KINDS.contains(&kind.as_str()) {
        fail(&format!("unknown kind `{kind}`"));
        ok = false;
    }
    let (Some(input), Some(expect)) = (raw.get("input").cloned(), raw.get("expect").cloned()) else {
        fail("missing `input` or `expect`");
        return None;
    };
    if !ok {
        return None;
    }
    Some(Case {
        id,
        file: file.to_string(),
        area: text("area").unwrap_or_default(),
        tags,
        why_hard,
        source: source.unwrap_or_default(),
        split,
        kind,
        input,
        expect,
    })
}

/// The ids in `known_failures.txt`: the first word of each line that is not blank or a
/// comment.
fn load_known(dir: &Path, problems: &mut Vec<String>) -> BTreeSet<String> {
    let path = dir.join("known_failures.txt");
    let Ok(text) = std::fs::read_to_string(&path) else {
        problems.push("known_failures.txt is missing".into());
        return BTreeSet::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}
