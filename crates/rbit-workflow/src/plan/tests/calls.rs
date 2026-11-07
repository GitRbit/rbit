//! Original planner calls fixtures; behavior assertions retained.
use super::super::planning::plan;
use super::support::{BUILD, planned_with, run_context, workflow};
use crate::limits::{Deadline, Limits};
use crate::spec::{DeferredInput, NeedSource, PassedSecrets};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn called_workflows_become_jobs_of_the_run() {
    let caller = "on: push
jobs:
  first:
    runs-on: x
    steps: [{run: x}]
  call:
    needs: first
    if: github.ref == 'refs/heads/main'
    uses: ./.rbit/workflows/build.yml
    with: {greeting: hello, late: '${{ needs.first.outputs.x }}'}
    secrets: {key: '${{ secrets.DEPLOY }}'}
  after:
    needs: call
    runs-on: x
    steps: [{run: x}]
";
    let jobs = planned_with(caller, &[(".rbit/workflows/build.yml", BUILD)]).unwrap();
    let keys: Vec<_> = jobs.iter().map(|job| (job.job_id.as_str(), job.name.as_str())).collect();
    assert_eq!(keys, [("first", "first"), ("call/build", "call / build"), ("call/test", "call / test"), ("after", "after")]);
    let (build, test, after) = (&jobs[1], &jobs[2], &jobs[3]);
    assert_eq!(build.runs_on, ["hello-runner"], "runs-on reads the inputs known when planning");
    assert_eq!(build.needs, ["first"], "the called workflow waits for what its caller waits for");
    assert_eq!(test.needs, ["call/build"]);
    assert_eq!(test.need_names, BTreeMap::from([("build".to_owned(), NeedSource::Jobs(vec!["call/build".to_owned()]))]));
    assert!(build.need_names.is_empty(), "a called workflow's first job sees no `needs`");
    let level = &build.calls[0];
    assert_eq!(level.inputs, json!({"greeting": "hello", "count": 2.0, "debug": false}).as_object().cloned().unwrap());
    assert_eq!(level.deferred["late"], DeferredInput { value: "${{ needs.first.outputs.x }}".into(), kind: "string".into() });
    assert_eq!(level.condition.as_deref(), Some("github.ref == 'refs/heads/main'"));
    assert_eq!(level.secrets, PassedSecrets::Map(BTreeMap::from([("key".into(), "${{ secrets.DEPLOY }}".into())])));
    assert_eq!(build.permissions.map(|permissions| permissions.contents), Some(crate::spec::Access::Read));
    assert_eq!(after.needs, ["call/build", "call/test"]);
    let NeedSource::Call { keys, outputs, jobs } = &after.need_names["call"] else { panic!("{:?}", after.need_names) };
    assert_eq!((keys.len(), outputs["result"].as_str()), (2, "${{ jobs.build.outputs.out }}"));
    assert_eq!(jobs["build"], NeedSource::Jobs(vec!["call/build".into()]));
}

#[test]
fn matrices_and_nesting_call_once_per_combination() {
    let caller = "on: push
jobs:
  call:
    strategy: {matrix: {flavor: [a, b]}}
    uses: ./outer.yml
    with: {flavor: '${{ matrix.flavor }}'}
    secrets: inherit
";
    let outer = "on: {workflow_call: {inputs: {flavor: {type: string}}}}
jobs:
  inner:
    uses: ./leaf.yml
    with: {flavor: '${{ inputs.flavor }}'}
";
    let leaf = "on: {workflow_call: {inputs: {flavor: {type: string}}}}
jobs:
  leaf:
    runs-on: ${{ inputs.flavor }}
    steps: [{run: x}]
";
    let jobs = planned_with(caller, &[("outer.yml", outer), ("leaf.yml", leaf)]).unwrap();
    let keys: Vec<_> = jobs.iter().map(|job| (job.job_id.as_str(), job.name.as_str(), job.runs_on[0].as_str())).collect();
    assert_eq!(keys, [("call#0/inner/leaf", "call (a) / inner / leaf", "a"), ("call#1/inner/leaf", "call (b) / inner / leaf", "b")]);
    assert_eq!(jobs[0].calls.len(), 2);
    assert_eq!(jobs[0].calls[0].secrets, PassedSecrets::Inherit);
}

#[test]
fn calls_are_checked_when_the_run_is_planned() {
    let call = |with: &str, secrets: &str| {
        format!("on: push\npermissions: {{contents: read}}\njobs:\n  call:\n    uses: ./b.yml\n    with: {with}\n    secrets: {secrets}\n")
    };
    let files = [("b.yml", BUILD), ("plain.yml", "on: push\njobs:\n  a: {runs-on: x, steps: [{run: x}]}\n")];
    for (source, message) in [
        (call("{}", "{key: x}"), "needs the input `greeting`"),
        (call("{greeting: hi, nope: 1}", "{key: x}"), "`with.nope`: the called workflow has no such input"),
        (call("{greeting: hi, count: many}", "{key: x}"), "`many` is not a number"),
        (call("{greeting: hi}", "{}"), "needs the secret `key`"),
        (call("{greeting: hi}", "{key: x, other: y}"), "`secrets.other`: the called workflow has no such secret"),
        ("on: push\njobs:\n  call: {uses: ./plain.yml}\n".to_owned(), "does not run on `workflow_call`"),
        ("on: push\njobs:\n  call: {uses: ./missing.yml}\n".to_owned(), "`missing.yml` is not there"),
    ] {
        let error = planned_with(&source, &files).unwrap_err().to_string();
        assert!(error.contains(message), "{source}\n{error}");
    }
    let writer = "on: workflow_call\njobs:\n  w:\n    runs-on: x\n    permissions: {contents: write}\n    steps: [{run: x}]\n";
    let caller = "on: push\npermissions: {contents: read}\njobs:\n  call:\n    uses: ./w.yml\n";
    let error = planned_with(caller, &[("w.yml", writer)]).unwrap_err().to_string();
    assert!(error.contains("asks for `contents: write`, but the workflow calling it allows only `contents: read`"), "{error}");
    // A caller that declares nothing gives its own token, the repository's default: the called
    // job is marked to stay within it, whatever it declares.
    let undeclared = "on: push
jobs:
  call:
    uses: ./w.yml
";
    let planned = planned_with(undeclared, &[("w.yml", writer)]).unwrap();
    assert!(planned.iter().all(|template| template.default_ceiling), "{planned:?}");
    let declared = "on: push
permissions: {contents: write}
jobs:
  call:
    uses: ./w.yml
";
    assert!(planned_with(declared, &[("w.yml", writer)]).unwrap().iter().all(|template| !template.default_ceiling));
    assert!(plan(&workflow("on: push\njobs:\n  call: {uses: ./b.yml}\n"), &run_context(), &Limits::default(), &Deadline::NONE).is_err());
}

#[test]
fn calls_nest_at_most_ten_levels_deep() {
    let forever = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
    let leaf = "on: workflow_call\njobs:\n  leaf: {runs-on: x, steps: [{run: x}]}\n";
    let deep = |called: usize| {
        let mut files: Vec<(&'static str, &'static str)> = (1..called)
            .map(|n| (forever(format!("w{n}.yml")), forever(format!("on: workflow_call\njobs:\n  d:\n    uses: ./w{}.yml\n", n + 1))))
            .collect();
        files.push((forever(format!("w{called}.yml")), leaf));
        planned_with("on: push\njobs:\n  d:\n    uses: ./w1.yml\n", &files)
    };
    assert!(deep(9).is_ok(), "the run's workflow and nine called ones: ten levels");
    let error = deep(10).unwrap_err().to_string();
    assert!(error.contains("more than 10 levels deep"), "{error}");
}
