//! Original parser support fixtures; assertions and source samples retained.
use super::super::parse;
use crate::error::{Location, WorkflowError};
use crate::limits::{Deadline, Limits};
use crate::model::Workflow;

pub(super) fn parse_ok(source: &str) -> Workflow {
    parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
}

pub(super) fn parse_err(source: &str) -> WorkflowError {
    parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).expect_err(source)
}

/// `job` is spliced into a workflow as the body of job `a`, at line 4.
pub(super) fn job_err(job: &str) -> WorkflowError {
    parse_err(&format!("on: push\njobs:\n  a:\n{job}"))
}

/// Every error has its message and location; all mismatches are reported at once.
pub(super) fn assert_errors<'a>(cases: impl Iterator<Item = (&'a str, WorkflowError, &'a str, (u32, u32))>) {
    let mismatches: Vec<String> = cases
        .filter(|(_, error, message, (line, column))| {
            !error.message.contains(message) || error.at != Some(Location { line: *line, column: *column })
        })
        .map(|(source, error, message, at)| format!("{source:?}: {error} (expected {message:?} at {at:?})"))
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

pub(super) const CI: &str = "\
name: CI
run-name: Build ${{ github.ref }}
on:
  push:
    branches: [main, 'release/**']
    paths-ignore: ['docs/**']
  pull_request:
    types: [opened, synchronize, ready_for_review]
    branches: main
  workflow_dispatch:
    inputs:
      level:
        type: choice
        options: [low, high]
        default: low
        required: true
      dry-run:
        type: boolean
  schedule:
    - cron: '0 3 * * 1'
    - cron: '*/30 * * * *'
permissions:
  contents: write
  pull-requests: read
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs:
  build:
    name: Build ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, debian-12]
        rust: [stable, 1.94]
        include:
          - os: debian-12
            experimental: true
        exclude:
          - os: debian-12
            rust: 1.94
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
  deploy:
    needs: build
    runs-on: [self-hosted, linux]
    permissions: read-all
    steps:
      - run: ./deploy
";

pub(super) const EVERY_KEY: &str = "\
name: Every key
on: push
env:
  CARGO_TERM_COLOR: always
  LEVEL: 3
  EMPTY:
defaults:
  run:
    shell: bash
    working-directory: crates
jobs:
  test:
    name: Test ${{ matrix.os }}
    needs: [lint]
    if: github.ref == 'refs/heads/main'
    runs-on: [self-hosted, '${{ matrix.os }}']
    permissions: {contents: read}
    env:
      RUST: ${{ matrix.rust }}
    defaults:
      run: {shell: pwsh}
    outputs:
      version: ${{ steps.meta.outputs.version }}
    timeout-minutes: 30
    continue-on-error: ${{ matrix.experimental }}
    concurrency:
      group: test-${{ github.ref }}-${{ matrix.os }}
      cancel-in-progress: true
    strategy:
      fail-fast: false
      max-parallel: 2
      matrix:
        os: [linux, windows]
    container:
      image: docker.io/library/rust:${{ matrix.rust || '1.94' }}
      env: {CARGO_HOME: /cargo}
      credentials:
        username: ci
        password: ${{ secrets.REGISTRY_PASSWORD }}
    services:
      postgres:
        image: postgres:17
        env: {POSTGRES_PASSWORD: pw}
      redis:
        image: redis:7
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          lfs: true
      - id: meta
        name: Meta on ${{ runner.os }}
        if: ${{ always() }}
        run: echo \"version=1\" >> \"$GITHUB_OUTPUT\"
        shell: sh
        working-directory: ${{ github.workspace }}/x
        env: {TOKEN: '${{ secrets.GITHUB_TOKEN }}'}
        continue-on-error: true
        timeout-minutes: ${{ matrix.minutes }}
  lint:
    runs-on: linux
    container: alpine:3
    steps:
      - run: cargo clippy
";
