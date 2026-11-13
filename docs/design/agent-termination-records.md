# Agent loop: termination records

Status: built. Work package 1.3. Before it, a timeout, a lost runner and a failing step
all read `failure`, a lost runner was noticed only after ten minutes, and nothing said
whether a job's log was complete, so a cut-off log could be read as the whole story.

## Interfaces as built

**Protocol.** `TaskStateRequest` gains an optional `termination`
(`{"reason", "step", "exit_code"}`), allowed on the final call only. The reason is a string
on the wire (validated as 1 to 32 lowercase letters and `_`), so a newer runner's word never
makes an older server refuse the whole call; the server reads an unknown word as `unknown`.
An older runner sends none; an older server ignores it. The protocol version stays 1.

**Runner.** `termination::Ending` (shared through the `Reporter`) learns the exit code of
each step's process, the first step whose conclusion (after `continue-on-error`) is a
failure, and whether `timeout-minutes` fired. At the end: `completed` for success;
`timeout` when the timer fired (whether the job then reads failed or cancelled);
`cancelled`; `step_failed` with the step and its exit code; `setup_failed` for a failure
with no failed step (a container that would not start, a job the runner could not read).

**Server.** Migration `0075_actions_terminations.sql`: one row per task, the first record
kept (`INSERT OR IGNORE`). The final state call records the runner's record, or the reason
its result implies (`completed`, `cancelled`, `unknown` for an unexplained failure) before
the task stops. The stale-task sweep records `runner_lost` for a silent task and `timeout`
for an endless one. `zombie_timeout_seconds` defaults to 120 (was 600); runners report
every 10 seconds.

**API.** Each job in `…/runs/{id}/jobs` and `…/jobs/{id}` gains `termination`:
`{reason, step, step_name, exit_code, log_complete}` once its task has ended, `step` being
the `number` in `steps`. A job without a record reads by its status. A job that ran out of
time reads `conclusion: timed_out`. `log_complete` is the task's log being final and not
truncated. `rbit run view --json` shows the record per job.

## Tests

- Protocol: wire shape, optional both ways, validation, final-call only.
- Runner: first failed step and its exit code, setup failure, timeout over the
  cancellation it causes, a signal with no exit code.
- Server: the reason an older runner's silence implies, an unknown word.
- API: annotation by record and by status, `timed_out`, step numbering and name.
- End to end (`agent_termination.rs`): a real runner, a step that exits 3 reads
  `step_failed` with its name, exit code 3 and a complete log; a passing job reads
  `completed`; the single-job endpoint agrees.

## Left out

`superseded` and `server_shutdown` are defined but not yet recorded: supersede is work
package 2.1, and a server that stops does not stop its tasks today. The job page in the web
interface does not show the record yet. A job killed mid-log end to end needs the
two-minute zombie wait and is left to the benchmark's slow scenarios.
