//! Original planner support fixtures; behavior assertions retained.
use super::super::contexts::RunContext;
use super::super::loader::{Loaded, Loader};
use super::super::matrix::{Axes, combinations, go_format};
use super::super::planning::{plan, plan_with};
use crate::error::WorkflowError;
use crate::limits::{Deadline, Limits};
use crate::model::{CallTarget, Workflow};
use crate::parse::parse;
use crate::spec::JobTemplate;
use serde_json::json;
use std::collections::BTreeMap;

pub(super) fn workflow(source: &str) -> Workflow {
    parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
}

/// A push to `main` of `alice/app`, dispatched with `target=arm64`, with `RUNNERS` set.
pub(super) fn run_context() -> RunContext {
    RunContext {
        github: json!({"ref": "refs/heads/main", "repository": "alice/app", "event_name": "push", "workflow": "CI", "run_id": 7})
            .as_object()
            .cloned()
            .unwrap(),
        inputs: json!({"target": "arm64", "minutes": 45, "matrix": "{\"os\": [\"x\", \"y\"]}"}).as_object().cloned().unwrap(),
        vars: BTreeMap::from([("RUNNER".into(), "big".into()), ("OSES".into(), "[\"debian\", \"alpine\"]".into())]),
    }
}

pub(super) fn planned(source: &str) -> Vec<JobTemplate> {
    plan(&workflow(source), &run_context(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
}

pub(super) fn matrix_of(matrix: &str) -> Vec<String> {
    let source = format!("on: push\njobs:\n  j:\n    steps: [{{run: x}}]\n    runs-on: x\n    strategy:\n      matrix:\n{matrix}\n");
    let workflow = workflow(&source);
    let axes = Axes::resolve(workflow.jobs[0].strategy.matrix.as_ref().unwrap(), &run_context().contexts(), "j").unwrap();
    combinations(&axes, &Limits::default(), &Deadline::NONE)
        .unwrap()
        .iter()
        .map(|combination| combination.iter().map(|(key, value)| format!("{key}={}", go_format(value))).collect::<Vec<_>>().join(","))
        .collect()
}

// Actions: reusable workflows.
/// Workflows by path, all in one repository; a call outside it is not there.
pub(super) struct Files(BTreeMap<&'static str, &'static str>);

impl Loader for Files {
    fn load(&mut self, origin: Option<&str>, target: &CallTarget) -> Result<Loaded, String> {
        let path = match target {
            CallTarget::Local { path } => path.clone(),
            CallTarget::Remote { owner, repo, path, reference } => format!("{owner}/{repo}/{path}@{reference}"),
        };
        let source = self.0.get(path.as_str()).ok_or_else(|| format!("`{path}` is not there"))?;
        Ok(Loaded { workflow: workflow(source), origin: origin.unwrap_or("here").to_owned(), id: path })
    }
}

pub(super) fn planned_with(source: &str, files: &[(&'static str, &'static str)]) -> Result<Vec<JobTemplate>, WorkflowError> {
    plan_with(&workflow(source), &run_context(), &Limits::default(), &Deadline::NONE, &mut Files(files.iter().copied().collect()))
}

pub(super) const BUILD: &str = "on:
  workflow_call:
    inputs:
      greeting: {type: string, required: true}
      count: {type: number, default: 2}
      debug: {type: boolean}
      late: {type: string}
    secrets:
      key: {required: true}
    outputs:
      result: {value: '${{ jobs.build.outputs.out }}'}
permissions:
  contents: read
jobs:
  build:
    runs-on: ${{ inputs.greeting }}-runner
    outputs: {out: '${{ steps.s.outputs.out }}'}
    steps: [{id: s, run: x}]
  test:
    needs: build
    runs-on: x
    steps: [{run: x}]
";

pub(super) const EVERY_FIELD: &str = "\
name: CI
on: push
env: {CARGO_TERM_COLOR: always}
defaults:
  run: {shell: bash, working-directory: crates}
permissions: read-all
jobs:
  build:
    runs-on: linux
    steps: [{run: cargo build}]
  test:
    name: Test ${{ matrix.os }}
    needs: build
    if: github.ref == 'refs/heads/main'
    runs-on: ${{ matrix.os }}
    permissions: {contents: write}
    env: {RUST: '${{ matrix.rust }}'}
    defaults:
      run: {shell: sh}
    outputs:
      version: ${{ steps.meta.outputs.version }}
    timeout-minutes: ${{ inputs.minutes }}
    continue-on-error: ${{ matrix.os == 'windows' }}
    concurrency:
      group: test-${{ github.ref }}-${{ matrix.os }}
      cancel-in-progress: true
    strategy:
      fail-fast: false
      max-parallel: ${{ fromJSON('1') }}
      matrix:
        os: [linux, windows]
    container:
      image: rust:${{ matrix.os }}
      credentials: {username: ci, password: '${{ secrets.REGISTRY }}'}
    services:
      db:
        image: postgres:${{ strategy.job-total }}
        env: {POSTGRES_PASSWORD: '${{ secrets.DB }}'}
    steps:
      - uses: actions/checkout@v4
        with: {fetch-depth: 0}
      - id: meta
        name: Meta
        if: always()
        run: echo version=1 >> \"$GITHUB_OUTPUT\"
        shell: bash
        working-directory: x
        env: {A: b}
        continue-on-error: true
        timeout-minutes: 5
";
