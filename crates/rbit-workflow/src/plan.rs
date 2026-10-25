//! A workflow → the jobs of one run: matrices expanded, `runs-on` and names interpolated,
//! one single-job payload per job (see [`crate::payload`]).
//!
//! Matrix expansion follows act's `GetMatrixes` (which Forgejo and Gitea run): the cartesian
//! product of the dimensions in file order, minus every combination an `exclude` entry
//! matches; then each `include` entry is merged into every combination whose *original*
//! values it does not contradict (values it adds may overwrite each other), and an entry
//! that fits none becomes a combination of its own. Combinations are then ordered by their
//! display name `(value, value)` — values in key order — as Forgejo orders them, and a
//! job without a `name` is called `<id> (<values>)`.
//!
//! The workflow-level `concurrency.group` is evaluated here too, against the `github.*`
//! values rbit knows ([`GroupContext`]).

use serde_json::{Map, Value, json};

use crate::{
    error::{Location, WorkflowError},
    expr::{ContextMap, Template, to_text},
    limits::{Deadline, Limits},
    model::{Job, Matrix, Permissions, Workflow},
    payload,
    yaml::{self, Mapping, Node, ScalarValue},
};

/// One matrix combination: each key and its value, keys in sorted order.
pub type Combination = Vec<(String, Node)>;

/// One job of a run.
#[derive(Debug, Clone)]
pub struct PlannedJob {
    /// The key under `jobs`.
    pub job_id: String,
    /// The display name, matrix values interpolated.
    pub name: String,
    /// The job ids this job waits for.
    pub needs: Vec<String>,
    /// The labels a runner must have, matrix values interpolated.
    pub runs_on: Vec<String>,
    /// Empty for a job without a matrix.
    pub matrix: Combination,
    /// The `permissions` that apply: the job's, else the workflow's. They can only lower
    /// the repository's default token permission.
    pub permissions: Option<Permissions>,
    /// The single-job workflow handed to the runner.
    pub payload: String,
}

impl PlannedJob {
    /// The combination as a JSON object, or `None` without a matrix.
    pub fn matrix_json(&self) -> Option<String> {
        if self.matrix.is_empty() {
            return None;
        }
        let at = Location::new(0, 0);
        let mapping = Mapping(self.matrix.iter().map(|(key, value)| (crate::yaml::Key { name: key.clone(), at }, value.clone())).collect());
        Some(crate::emit::json(&Node::new(yaml::Value::Mapping(mapping), at)))
    }
}

/// Plans every job of `workflow`, in file order, each job's combinations in Forgejo's order.
pub fn plan(workflow: &Workflow, limits: &Limits, deadline: &Deadline) -> Result<Vec<PlannedJob>, WorkflowError> {
    let mut planned = Vec::new();
    for job in &workflow.jobs {
        let combinations = match &job.matrix {
            Some(matrix) => combinations(matrix, limits, deadline)?,
            None => vec![Vec::new()],
        };
        if planned.len() + combinations.len() > limits.max_run_jobs {
            return Err(WorkflowError::limit(format!("a run may have at most {} jobs", limits.max_run_jobs), Some(job.at)));
        }
        for combination in combinations {
            deadline.check()?;
            let runs_on = runs_on(job, &combination)?;
            let name = job_name(job, &combination);
            let payload = payload::single_job(workflow, job, &name, &runs_on, &combination);
            planned.push(PlannedJob {
                job_id: job.id.clone(),
                name,
                needs: job.needs.clone(),
                runs_on,
                matrix: combination,
                permissions: job.permissions.or(workflow.permissions),
                payload,
            });
        }
    }
    Ok(planned)
}

/// Expands a matrix within `limits` (see the module documentation for the rules).
pub fn combinations(matrix: &Matrix, limits: &Limits, deadline: &Deadline) -> Result<Vec<Combination>, WorkflowError> {
    let too_many = || WorkflowError::limit(format!("a matrix may have at most {} combinations", limits.max_combinations), Some(matrix.at));
    let product = matrix.dimensions.iter().try_fold(1usize, |count, (_, values)| count.checked_mul(values.len()));
    if product.is_none_or(|count| count > limits.max_combinations) {
        return Err(too_many());
    }
    // The cartesian product, as (key, value) lists in dimension order.
    let mut combinations: Vec<Vec<(String, Node)>> = if matrix.dimensions.is_empty() { Vec::new() } else { vec![Vec::new()] };
    for (key, values) in &matrix.dimensions {
        deadline.check()?;
        combinations = combinations
            .into_iter()
            .flat_map(|combination| {
                values.iter().map(move |value| {
                    let mut next = combination.clone();
                    next.push((key.clone(), value.clone()));
                    next
                })
            })
            .collect();
    }
    combinations.retain(|combination| {
        !matrix
            .exclude
            .iter()
            .any(|exclude| combination.iter().all(|(key, value)| exclude.get(key).is_none_or(|excluded| excluded.same_value(value))))
    });
    let original = |key: &str| matrix.dimensions.iter().any(|(name, _)| name == key);
    let mut extra = Vec::new();
    for include in &matrix.include {
        deadline.check()?;
        let mut matched = false;
        for combination in &mut combinations {
            let fits =
                combination.iter().all(|(key, value)| !original(key) || include.get(key).is_none_or(|included| included.same_value(value)));
            if fits {
                matched = true;
                for (key, value) in include.iter() {
                    match combination.iter_mut().find(|(existing, _)| *existing == key.name) {
                        Some(entry) => entry.1 = value.clone(),
                        None => combination.push((key.name.clone(), value.clone())),
                    }
                }
            }
        }
        if !matched {
            extra.push(include.iter().map(|(key, value)| (key.name.clone(), value.clone())).collect());
        }
    }
    combinations.extend(extra);
    if combinations.len() > limits.max_combinations {
        return Err(too_many());
    }
    if combinations.is_empty() {
        combinations.push(Vec::new());
    }
    for combination in &mut combinations {
        combination.sort_by(|a, b| a.0.cmp(&b.0));
    }
    combinations.sort_by_cached_key(matrix_name);
    Ok(combinations)
}

/// `(value, value)`, values in key order, formatted as Go's `fmt.Sprint` formats them.
fn matrix_name(combination: &Combination) -> String {
    let values: Vec<String> = combination.iter().map(|(_, value)| go_format(value)).collect();
    format!("({})", values.join(", "))
}

/// A YAML value as Go's `fmt.Sprint` prints it once decoded into `interface{}`.
fn go_format(node: &Node) -> String {
    match &node.value {
        yaml::Value::Scalar(scalar) => match &scalar.value {
            ScalarValue::Null => "<nil>".to_owned(),
            ScalarValue::Bool(value) => value.to_string(),
            ScalarValue::Int(value) => value.to_string(),
            ScalarValue::Float(value) => go_float(*value),
            ScalarValue::String(text) => text.clone(),
        },
        yaml::Value::Sequence(items) => format!("[{}]", items.iter().map(go_format).collect::<Vec<_>>().join(" ")),
        yaml::Value::Mapping(mapping) => {
            let mut entries: Vec<(&str, String)> = mapping.iter().map(|(key, value)| (key.name.as_str(), go_format(value))).collect();
            entries.sort_unstable();
            format!("map[{}]", entries.iter().map(|(key, value)| format!("{key}:{value}")).collect::<Vec<_>>().join(" "))
        }
    }
}

/// Go's `%v` for a float64: the shortest representation, with an exponent (`1e+21`,
/// `1e-05`) below 1e-4 and from 1e21.
fn go_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf".to_owned() } else { "-Inf".to_owned() };
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-4..1e21).contains(&magnitude) {
        let formatted = format!("{value:e}");
        let (mantissa, exponent) = formatted.split_once('e').unwrap_or((&formatted, "0"));
        let (sign, digits) = exponent.strip_prefix('-').map_or(("+", exponent), |digits| ("-", digits));
        return format!("{mantissa}e{sign}{digits:0>2}");
    }
    format!("{value}")
}

/// The contexts a job's `runs-on` and `name` are evaluated with: its `matrix`.
fn matrix_contexts(combination: &Combination) -> ContextMap {
    let matrix: Map<String, Value> = combination.iter().map(|(key, value)| (key.clone(), value.to_json())).collect();
    ContextMap::new().with("matrix", matrix)
}

/// `runs-on` with matrix values interpolated. A label that is exactly one expression whose
/// value is a list (`${{ matrix.labels }}`) contributes every item. Anything but `matrix`
/// cannot be evaluated yet and is refused: a runner is chosen before the job runs.
fn runs_on(job: &Job, combination: &Combination) -> Result<Vec<String>, WorkflowError> {
    let refuse = |label: &str, problem: &dyn std::fmt::Display| {
        WorkflowError::plan(format!("job `{}`: runs-on `{label}`: {problem}", job.id), Some(job.at))
    };
    let contexts = matrix_contexts(combination);
    let mut labels = Vec::new();
    for label in &job.runs_on {
        let value = Template::parse(label).and_then(|template| template.evaluate(&contexts)).map_err(|problem| refuse(label, &problem))?;
        match value {
            Value::Array(items) => {
                for item in items {
                    let Value::String(item) = item else {
                        return Err(refuse(label, &"a list for runs-on must hold strings"));
                    };
                    labels.push(item);
                }
            }
            Value::Object(_) => return Err(refuse(label, &"runs-on must be a string or a list of strings")),
            other => labels.push(to_text(&other)),
        }
    }
    if labels.is_empty() || labels.iter().any(String::is_empty) {
        return Err(WorkflowError::plan(format!("job `{}`: runs-on evaluates to an empty label", job.id), Some(job.at)));
    }
    Ok(labels)
}

/// The job's `name` with matrix values interpolated (other expressions are left for the
/// runner), or `<id>` / `<id> (<values>)`.
fn job_name(job: &Job, combination: &Combination) -> String {
    match &job.name {
        Some(name) => {
            Template::parse(name).map_or_else(|_| name.clone(), |template| template.render_partially(&matrix_contexts(combination)))
        }
        None if combination.is_empty() => job.id.clone(),
        None => format!("{} {}", job.id, matrix_name(combination)),
    }
}

/// The `github.*` values a workflow-level `concurrency.group` may use (§11.6).
///
/// Other `github` properties are `null` until runs are created from job templates with the
/// whole `github` context.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupContext {
    /// The workflow's name, or its path when it has none.
    pub workflow: String,
    /// The full ref (`refs/heads/main`, `refs/pull/7/head`).
    pub reference: String,
    /// Pull requests only: the head branch.
    pub head_ref: String,
    /// Pull requests only: the base branch.
    pub base_ref: String,
    pub event_name: String,
    /// `owner/name`.
    pub repository: String,
    pub run_id: i64,
    pub pull_request_number: Option<u64>,
}

impl GroupContext {
    /// The values as a `github` context.
    pub fn github(&self) -> Value {
        json!({
            "workflow": self.workflow,
            "ref": self.reference,
            "head_ref": self.head_ref,
            "base_ref": self.base_ref,
            "event_name": self.event_name,
            "repository": self.repository,
            "run_id": self.run_id,
            "event": {"pull_request": {"number": self.pull_request_number}},
        })
    }
}

/// The run's concurrency group and whether it cancels runs in progress, or `None` when
/// the workflow sets no concurrency.
pub fn concurrency_group(workflow: &Workflow, context: &GroupContext) -> Result<Option<(String, bool)>, WorkflowError> {
    let Some(concurrency) = &workflow.concurrency else {
        return Ok(None);
    };
    let refuse = |problem: &dyn std::fmt::Display| WorkflowError::plan(format!("concurrency group: {problem}"), Some(concurrency.at));
    let contexts = ContextMap::new().with("github", context.github());
    let group = Template::parse(&concurrency.group).and_then(|template| template.render(&contexts)).map_err(|problem| refuse(&problem))?;
    if group.is_empty() {
        return Err(refuse(&"evaluates to an empty group"));
    }
    Ok(Some((group, concurrency.cancel_in_progress)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn workflow(source: &str) -> Workflow {
        parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
    }

    fn matrix_of(matrix: &str) -> Vec<String> {
        let source = format!("on: push\njobs:\n  j:\n    runs-on: x\n    strategy:\n      matrix:\n{matrix}\n");
        let workflow = workflow(&source);
        combinations(workflow.jobs[0].matrix.as_ref().unwrap(), &Limits::default(), &Deadline::NONE)
            .unwrap()
            .iter()
            .map(|combination| combination.iter().map(|(key, value)| format!("{key}={}", go_format(value))).collect::<Vec<_>>().join(","))
            .collect()
    }

    #[test]
    fn matrix_expansion_table() {
        let cases: &[(&str, &[&str])] = &[
            ("        os: [a, b]", &["os=a", "os=b"]),
            ("        os: [b, a]\n        v: [2, 1]", &["os=a,v=1", "os=a,v=2", "os=b,v=1", "os=b,v=2"]),
            ("        os: [a, b]\n        v: [1, 2]\n        exclude:\n          - {os: a, v: 2}", &["os=a,v=1", "os=b,v=1", "os=b,v=2"]),
            ("        os: [a, b]\n        v: [1, 2]\n        exclude:\n          - {os: a}", &["os=b,v=1", "os=b,v=2"]),
            // An include that fits merges into every combination it fits.
            ("        os: [a, b]\n        include:\n          - {os: a, extra: x}", &["os=b", "extra=x,os=a"]),
            ("        os: [a, b]\n        include:\n          - {extra: x}", &["extra=x,os=a", "extra=x,os=b"]),
            // An include that contradicts every original value stands alone.
            ("        os: [a, b]\n        include:\n          - {os: c, extra: x}", &["os=a", "os=b", "extra=x,os=c"]),
            // Added values may be overwritten by a later include; original ones never.
            ("        os: [a]\n        include:\n          - {extra: x}\n          - {extra: y}", &["extra=y,os=a"]),
            // GitHub's documented example.
            (
                "        fruit: [apple, pear]\n        animal: [cat, dog]\n        include:\n          - color: green\n          - color: pink\n            animal: cat\n          - fruit: apple\n            shape: circle\n          - fruit: banana\n          - fruit: banana\n            animal: cat",
                &[
                    "fruit=banana",
                    "animal=cat,fruit=banana",
                    "animal=cat,color=pink,fruit=apple,shape=circle",
                    "animal=cat,color=pink,fruit=pear",
                    "animal=dog,color=green,fruit=apple,shape=circle",
                    "animal=dog,color=green,fruit=pear",
                ],
            ),
            // Only includes: each is a combination.
            ("        include:\n          - {os: a}\n          - {os: b, v: 1}", &["os=a", "os=b,v=1"]),
            // An empty dimension leaves nothing but the includes, or one empty combination.
            ("        os: []", &[""]),
            ("        os: []\n        include:\n          - {os: z}", &["os=z"]),
            // Values compare as data: 1.10 equals 1.1, 1 does not equal '1'.
            ("        v: [1.10, 2]\n        exclude:\n          - {v: 1.1}", &["v=2"]),
            ("        v: [1, 2]\n        exclude:\n          - {v: '1'}", &["v=1", "v=2"]),
            ("        cfg: [{a: 1}, [x, y]]", &["cfg=[x y]", "cfg=map[a:1]"]),
        ];
        for (matrix, expected) in cases {
            assert_eq!(matrix_of(matrix), *expected, "{matrix}");
        }
    }

    #[test]
    fn matrix_limits() {
        let limits = Limits { max_combinations: 4, ..Limits::default() };
        let source = "on: push\njobs:\n  j:\n    runs-on: x\n    strategy:\n      matrix:\n        a: [1, 2, 3]\n        b: [1, 2]\n";
        let workflow = workflow(source);
        let error = combinations(workflow.jobs[0].matrix.as_ref().unwrap(), &limits, &Deadline::NONE).unwrap_err();
        assert_eq!(error.kind, crate::ErrorKind::Limit);
        let source =
            "on: push\njobs:\n  j:\n    runs-on: x\n    strategy:\n      matrix:\n        a: [1, 2, 3, 4]\n        include: [{a: 5}]\n";
        let workflow = self::workflow(source);
        assert!(combinations(workflow.jobs[0].matrix.as_ref().unwrap(), &limits, &Deadline::NONE).is_err(), "includes count too");
        let run_limits = Limits { max_run_jobs: 5, ..Limits::default() };
        let source = "on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {v: [1, 2, 3]}}\n  b:\n    runs-on: x\n    strategy: {matrix: {v: [1, 2, 3]}}\n";
        assert_eq!(plan(&self::workflow(source), &run_limits, &Deadline::NONE).unwrap_err().kind, crate::ErrorKind::Limit);
        let expired = Deadline::at(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1)).unwrap());
        assert_eq!(plan(&self::workflow(source), &Limits::default(), &expired).unwrap_err().kind, crate::ErrorKind::Timeout);
    }

    #[test]
    fn go_formatting() {
        let cases = [(1.17, "1.17"), (3.0, "3"), (1e21, "1e+21"), (1.5e-5, "1.5e-05"), (0.0001, "0.0001"), (-2.5, "-2.5"), (0.0, "0")];
        for (value, expected) in cases {
            assert_eq!(go_float(value), expected, "{value}");
        }
        assert_eq!(go_float(f64::NAN), "NaN");
    }

    #[test]
    fn names_and_labels() {
        let source = "on: push
jobs:
  plain:
    runs-on: linux
  named:
    name: Build ${{ matrix.os }} for ${{ inputs.target }}
    runs-on: [self-hosted, '${{ matrix.os }}-${{ matrix.arch || ''x64'' }}']
    strategy:
      matrix:
        os: [debian]
  listed:
    runs-on: ${{ matrix.labels }}
    strategy:
      matrix:
        labels: [[self-hosted, arm64]]
        n: [1.10]
";
        let jobs = plan(&workflow(source), &Limits::default(), &Deadline::NONE).unwrap();
        let summary: Vec<(&str, &str, Vec<&str>)> =
            jobs.iter().map(|job| (job.job_id.as_str(), job.name.as_str(), job.runs_on.iter().map(String::as_str).collect())).collect();
        assert_eq!(
            summary,
            [
                ("plain", "plain", vec!["linux"]),
                ("named", "Build debian for ${{ inputs.target }}", vec!["self-hosted", "debian-x64"]),
                ("listed", "listed ([self-hosted arm64], 1.1)", vec!["self-hosted", "arm64"]),
            ]
        );
        assert_eq!(jobs[0].matrix_json(), None);
        assert_eq!(jobs[2].matrix_json().as_deref(), Some(r#"{"labels":["self-hosted","arm64"],"n":1.1}"#));
    }

    #[test]
    fn runs_on_refusals() {
        for runs_on in ["${{ github.ref }}", "${{ matrix.missing }}", "${{ format( }}", "${{ matrix.empty }}", "${{ matrix.nested }}"] {
            let source = format!(
                "on: push\njobs:\n  j:\n    runs-on: \"{runs_on}\"\n    strategy: {{matrix: {{empty: [''], nested: [[{{a: 1}}]]}}}}\n"
            );
            let error = plan(&workflow(&source), &Limits::default(), &Deadline::NONE).unwrap_err();
            assert_eq!(error.kind, crate::ErrorKind::Plan, "{runs_on}");
            assert!(error.message.starts_with("job `j`"), "{error}");
        }
    }

    #[test]
    fn concurrency_groups() {
        let context = GroupContext {
            workflow: "CI".into(),
            reference: "refs/pull/7/head".into(),
            head_ref: "feature".into(),
            base_ref: "main".into(),
            event_name: "pull_request".into(),
            repository: "alice/rocket".into(),
            run_id: 31,
            pull_request_number: Some(7),
        };
        let group = |concurrency: &str| {
            let source = format!("on: push\nconcurrency: {concurrency}\njobs: {{a: {{runs-on: x}}}}\n");
            concurrency_group(&workflow(&source), &context)
        };
        assert_eq!(group("ci").unwrap(), Some(("ci".into(), false)));
        assert_eq!(group("'${{ github.workflow }}-${{ github.ref }}'").unwrap(), Some(("CI-refs/pull/7/head".into(), false)));
        assert_eq!(
            group("{group: '${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}', cancel-in-progress: true}")
                .unwrap(),
            Some(("CI-7".into(), true))
        );
        assert_eq!(group("'${{ github.head_ref || github.run_id }}'").unwrap(), Some(("feature".into(), false)));
        assert_eq!(
            group("'${{ github.repository }}/${{ github.event_name }}/${{ github.base_ref }}'").unwrap().unwrap().0,
            "alice/rocket/pull_request/main"
        );
        assert!(group("'${{ github.sha }}'").is_err());
        assert!(group("'${{ github.base_ref && null }}'").is_err(), "an empty group is refused");
        let push = GroupContext { pull_request_number: None, head_ref: String::new(), ..context };
        let source = "on: push\nconcurrency: '${{ github.event.pull_request.number || github.run_id }}'\njobs: {a: {runs-on: x}}\n";
        assert_eq!(concurrency_group(&workflow(source), &push).unwrap(), Some(("31".into(), false)));
        assert_eq!(concurrency_group(&workflow("on: push\njobs: {a: {runs-on: x}}\n"), &push).unwrap(), None);
    }
}
