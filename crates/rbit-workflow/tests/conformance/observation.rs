//! Conformance observation with unchanged verdict and fixture semantics.
use super::cases::Case;
use rbit_workflow::cron::Cron;
use rbit_workflow::expr::{self, Condition, ContextMap, Contexts, Status};
use rbit_workflow::model::ActivityType;
use rbit_workflow::pattern::Patterns;
use rbit_workflow::plan::{self, RunContext};
use rbit_workflow::spec::JobResult;
use rbit_workflow::trigger::{self, ChangedPaths, Event};
use rbit_workflow::{Deadline, Limits, Workflow};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// Runs one case's input through the crate and returns what it produced. An `Err` is an
/// evaluation or load error, reported as `{"error": message}`.
pub(super) fn observe(case: &Case) -> Result<Value, String> {
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

pub(super) fn string_list(value: Option<&Value>) -> Vec<String> {
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
