# Agent loop: logs by step and the failure digest

Status: built. Work packages 1.4 and 1.5. Before them, the only log API was a job's whole
log: 167 KB to learn one failing test line (see the step 0 baseline). Annotations and
earlier attempts could not be read through the API at all.

## Interfaces as built

All rbit's own, beside the GitHub-shaped routes, read with the caller's rights:

- `GET …/actions/runs/{run_id}/failure` (`rbit.failure/1`): per job that failed or was
  cancelled, its termination record (the step it names, or else the last failed step,
  or else the last that ran), up to 10 error annotations as `{path, line, message}`, and an
  excerpt: the last 30 lines of that step, read tail first (at most 400 lines and 128 KiB
  from the store), with runner timestamps and ANSI codes removed, runs of the same line
  folded (`… (repeated 50 times)`), each line cut at 400 characters. A cut excerpt says
  `truncated`, `shown` and `total`, and gives `more` as a runnable command.
- `GET …/actions/jobs/{job_id}/steps/{step}/log` (`rbit.log/1`): `tail` (default 100, at
  most 1,000) or `from` and `lines`; `attempt`; `raw`. At most 256 KiB.
- `GET …/actions/jobs/{job_id}/annotations` (`rbit.annotations/1`), `?attempt=`.
- `GET …/actions/jobs/{job_id}/attempts` (`rbit.attempts/1`): every attempt, oldest first,
  with `current`, the termination reason and `log_complete`.
- CLI: `rbit run failure [RUN | --sha]` (one `rbit.failure/1`, or `rbit.failures/1` for a
  commit's failed runs) and `rbit run log JOB --step N …`. Exit 9 when a failure's log is
  incomplete or its cause is `runner_lost` or `unknown`.

## Files

`rbit-actions/src/logs/digest.rs` (cleaning and folding, pure), `rbit-services/src/actions/
logs/digest.rs` (the read model; `job_attempt`, `job_annotations`, `job_attempts`),
`rbit-storage/src/sqlite/actions_runs/terminations.rs` (`job_tasks`), `rbit-web/src/routes/
api/actions/agent_logs.rs` (routes), `rbit-server/src/client/failure.rs` (CLI), OpenAPI.

## Tests

Unit: line cleaning, folding and the cut; which step a digest shows; when the CLI calls a
failure untrusted. End to end (`agent_termination.rs`): a real runner's failed job gives a
digest with only that job, its record and exit code, an excerpt containing the exit line
and no timestamps, under 6 KB in total; a one-line tail with `more_before`; one attempt,
current, `step_failed`; an error annotation.

## Left out

Failed tests from test reports (3.4), fingerprints and `seen` (3.5), `also_failing_on_base`
(3.5). The digest is what the benchmark's next recipe reads; its numbers go into the step's
benchmark record when the recipe lands.
