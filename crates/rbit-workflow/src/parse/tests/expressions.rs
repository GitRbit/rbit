//! Original parser expressions fixtures; assertions and source samples retained.
use super::super::{MAX_JOB_ID_BYTES, valid_job_id};
use super::support::{parse_err, parse_ok};

/// Each field may only use the contexts it is evaluated with (§12.4).
#[test]
fn expressions_are_checked_where_they_are_written() {
    let refused: &[(&str, &str)] = &[
        ("run-name: '${{ matrix.os }}'\n", "`run-name`: the `matrix` context is not available here; it can use github, inputs, vars"),
        ("concurrency: '${{ matrix.os }}'\n", "`concurrency.group`: the `matrix` context is not available"),
        ("run-name: '${{ github.ref'\n", "`run-name`: a `${{` is not closed"),
        ("jobs: {a: {runs-on: '${{ secrets.LABEL }}', steps: [{run: x}]}}\n", "`jobs.a.runs-on`: the `secrets` context"),
        ("jobs: {a: {runs-on: x, name: '${{ steps.x.outputs.y }}', steps: [{run: x}]}}\n", "`jobs.a.name`: the `steps` context"),
        ("jobs: {a: {runs-on: x, name: '${{ success() }}', steps: [{run: x}]}}\n", "`success()` can only be used in `if`"),
        ("jobs: {a: {runs-on: x, name: \"${{ hashFiles('x') }}\", steps: [{run: x}]}}\n", "`hashFiles` only works in steps"),
        ("jobs: {a: {runs-on: x, if: env.X, steps: [{run: x}]}}\n", "`jobs.a.if`: the `env` context"),
        ("jobs: {a: {runs-on: x, if: '${{ github.ref }} == b', steps: [{run: x}]}}\n", "`jobs.a.if`: a condition must be one expression"),
        ("jobs: {a: {runs-on: x, if: 'github.ref ==', steps: [{run: x}]}}\n", "`jobs.a.if`: an expression is incomplete"),
        ("jobs: {a: {runs-on: x, timeout-minutes: '${{ env.T }}', steps: [{run: x}]}}\n", "`jobs.a.timeout-minutes`: the `env` context"),
        (
            "jobs: {a: {runs-on: x, strategy: {fail-fast: '${{ matrix.x }}'}, steps: [{run: x}]}}\n",
            "`jobs.a.strategy.fail-fast`: the `matrix`",
        ),
        ("jobs: {a: {runs-on: x, container: '${{ secrets.IMAGE }}', steps: [{run: x}]}}\n", "`jobs.a.container`: the `secrets` context"),
        ("jobs: {a: {runs-on: x, concurrency: '${{ steps.a }}', steps: [{run: x}]}}\n", "`jobs.a.concurrency.group`: the `steps` context"),
        (
            "jobs: {a: {runs-on: x, env: {A: '${{ failure() }}'}, steps: [{run: x}]}}\n",
            "`jobs.a.env.A`: `failure()` can only be used in `if`",
        ),
        ("jobs: {a: {runs-on: x, steps: [{run: '${{ always() }}'}]}}\n", "`jobs.a.steps[0].run`: `always()` can only be used in `if`"),
        ("jobs: {a: {runs-on: x, steps: [{run: '${{ github..ref }}'}]}}\n", "`jobs.a.steps[0].run`"),
        ("jobs: {a: {runs-on: x, steps: [{run: x, if: 'foo.bar'}]}}\n", "`foo` is not a context"),
    ];
    for (rest, message) in refused {
        let source = format!("on: push\n{rest}");
        let source = if source.contains("jobs:") { source } else { format!("{source}jobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n") };
        let error = parse_err(&source);
        assert!(error.message.contains(message), "{source}: {error}");
    }
    let accepted = "on: push
run-name: ${{ github.actor }} on ${{ inputs.target || vars.DEFAULT }}
jobs:
  a:
    runs-on: ${{ fromJSON(vars.RUNNERS)[matrix.os] }}
    if: ${{ needs.b.result == 'success' || failure() }}
    needs: b
    steps:
      - run: echo ${{ secrets.X }} ${{ steps.s.outputs.y }} ${{ runner.os }} ${{ job.status }} ${{ env.A }}
        if: hashFiles('**/Cargo.lock') != '' && !cancelled()
  b:
    runs-on: x
    steps: [{run: x}]
";
    parse_ok(accepted);
}

#[test]
fn job_ids() {
    for valid in ["a", "_", "build-and-test", "A_1", &"a".repeat(MAX_JOB_ID_BYTES)] {
        assert!(valid_job_id(valid), "{valid}");
    }
    for invalid in ["", "1a", "-a", "a.b", "a b", "é", &"a".repeat(MAX_JOB_ID_BYTES + 1)] {
        assert!(!valid_job_id(invalid), "{invalid}");
    }
}
