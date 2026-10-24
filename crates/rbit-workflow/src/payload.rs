//! The single-job workflow a runner receives as a task's `workflow_payload`, in the shape
//! Forgejo's and Gitea's job parsers produce:
//!
//! * top-level `name`, `run-name`, `on`, `env`, `defaults` and `permissions` are copied in
//!   file order, then `jobs` with exactly one key;
//! * the job gets `name` first (the planned display name), `runs-on` with matrix values
//!   interpolated, and `strategy.matrix` rewritten to its one combination, each value a
//!   one-element list, so the runner's own expansion yields exactly that job;
//! * `needs` is removed — the runner rebuilds the `needs` context from the task's `needs`
//!   map, and a job naming jobs absent from its document would not plan there — and empty
//!   `steps` entries are dropped;
//! * everything else is copied as written, plain scalars with their original spelling.

use crate::{
    emit,
    model::{Job, Workflow},
    plan::Combination,
    yaml::{Key, Mapping, Node, Scalar, Value},
};

/// The top-level keys a payload carries over.
const COPIED: [&str; 6] = ["name", "run-name", "on", "env", "defaults", "permissions"];

/// The payload of one planned job.
pub fn single_job(workflow: &Workflow, job: &Job, name: &str, runs_on: &[String], combination: &Combination) -> String {
    let at = job.at;
    let key = |name: &str| Key { name: name.to_owned(), at };
    let text = |value: &str| Node::scalar(Scalar::string(value), at);

    let mut body = Mapping(vec![(key("name"), text(name))]);
    for (entry, value) in job.raw().iter() {
        let value = match entry.name.as_str() {
            "name" | "needs" => continue,
            "runs-on" => match runs_on {
                [label] if value.as_scalar().is_some() => text(label),
                labels => Node::new(Value::Sequence(labels.iter().map(|label| text(label)).collect()), value.at),
            },
            "strategy" if !combination.is_empty() => with_combination(value, combination),
            "steps" => match &value.value {
                Value::Sequence(steps) => {
                    Node::new(Value::Sequence(steps.iter().filter(|step| !step.is_null()).cloned().collect()), value.at)
                }
                _ => value.clone(),
            },
            _ => value.clone(),
        };
        body.0.push((entry.clone(), value));
    }

    let mut document = Mapping::default();
    for (entry, value) in workflow.document().iter() {
        if COPIED.contains(&entry.name.as_str()) {
            document.0.push((entry.clone(), value.clone()));
        }
    }
    let jobs = Mapping(vec![(key(&job.id), Node::new(Value::Mapping(body), at))]);
    document.0.push((key("jobs"), Node::new(Value::Mapping(jobs), at)));
    emit::document(&document)
}

/// `strategy` with `matrix` replaced by the combination, each value a one-element list.
fn with_combination(strategy: &Node, combination: &Combination) -> Node {
    let Value::Mapping(entries) = &strategy.value else {
        return strategy.clone();
    };
    let matrix = Mapping(
        combination
            .iter()
            .map(|(name, value)| (Key { name: name.clone(), at: value.at }, Node::new(Value::Sequence(vec![value.clone()]), value.at)))
            .collect(),
    );
    let entries = entries
        .iter()
        .map(|(key, value)| {
            let value = if key.name == "matrix" { Node::new(Value::Mapping(matrix.clone()), value.at) } else { value.clone() };
            (key.clone(), value)
        })
        .collect();
    Node::new(Value::Mapping(Mapping(entries)), strategy.at)
}

#[cfg(test)]
mod tests {
    use crate::{Deadline, Limits, parse, plan::plan};

    /// Each case runs a workflow through planning and compares the payloads, joined by
    /// `---`, with the expected file. The inputs are Gitea's job parser test data (MIT,
    /// see `THIRD-PARTY.md`), the outputs rbit's.
    const GOLDEN: [(&str, &str, &str); 8] = [
        ("multiple_jobs", include_str!("../tests/golden/multiple_jobs.in.yaml"), include_str!("../tests/golden/multiple_jobs.out.yaml")),
        (
            "multiple_matrix",
            include_str!("../tests/golden/multiple_matrix.in.yaml"),
            include_str!("../tests/golden/multiple_matrix.out.yaml"),
        ),
        (
            "job_name_with_matrix",
            include_str!("../tests/golden/job_name_with_matrix.in.yaml"),
            include_str!("../tests/golden/job_name_with_matrix.out.yaml"),
        ),
        ("has_needs", include_str!("../tests/golden/has_needs.in.yaml"), include_str!("../tests/golden/has_needs.out.yaml")),
        ("empty_step", include_str!("../tests/golden/empty_step.in.yaml"), include_str!("../tests/golden/empty_step.out.yaml")),
        (
            "prefixed_newline",
            include_str!("../tests/golden/prefixed_newline.in.yaml"),
            include_str!("../tests/golden/prefixed_newline.out.yaml"),
        ),
        (
            "continue_on_error_expr",
            include_str!("../tests/golden/continue_on_error_expr.in.yaml"),
            include_str!("../tests/golden/continue_on_error_expr.out.yaml"),
        ),
        (
            "include_exclude",
            include_str!("../tests/golden/include_exclude.in.yaml"),
            include_str!("../tests/golden/include_exclude.out.yaml"),
        ),
    ];

    fn payloads(source: &str) -> String {
        let workflow = parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap();
        let jobs = plan(&workflow, &Limits::default(), &Deadline::NONE).unwrap();
        jobs.iter().map(|job| job.payload.as_str()).collect::<Vec<_>>().join("---\n")
    }

    #[test]
    fn golden_payloads() {
        // RBIT_BLESS=1 rewrites the expected files after a deliberate change.
        let bless = std::env::var_os("RBIT_BLESS").is_some();
        for (name, input, expected) in GOLDEN {
            let actual = payloads(input);
            if bless {
                let path = format!("{}/tests/golden/{name}.out.yaml", env!("CARGO_MANIFEST_DIR"));
                std::fs::write(path, &actual).unwrap();
                continue;
            }
            assert_eq!(actual, expected.replace("\r\n", "\n"), "{name}");
        }
    }

    #[test]
    fn payloads_parse_back_into_single_job_workflows() {
        for (name, input, _) in GOLDEN {
            let workflow = parse(input.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap();
            for job in plan(&workflow, &Limits::default(), &Deadline::NONE).unwrap() {
                let single = parse(job.payload.as_bytes(), &Limits::default(), &Deadline::NONE)
                    .unwrap_or_else(|error| panic!("{name}: {error}\n{}", job.payload));
                assert_eq!(single.jobs.len(), 1, "{name}");
                assert!(single.jobs[0].needs.is_empty(), "{name}");
                let replanned = plan(&single, &Limits::default(), &Deadline::NONE).unwrap();
                assert_eq!(replanned.len(), 1, "{name}: one combination left");
                assert_eq!((&replanned[0].name, &replanned[0].runs_on), (&job.name, &job.runs_on), "{name}");
            }
        }
    }
}
