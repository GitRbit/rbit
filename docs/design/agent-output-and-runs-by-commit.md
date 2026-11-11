# Agent loop: output contract and runs by commit

Status: built. Work packages 1.1 (output contract) and 1.2 (runs by commit) of the agent
loop; step 0 is in `agent-step0-ci-and-candidate.md`.

## Problem

An agent that pushed a commit must learn what CI made of *that* commit. Before this change:

- `rbit run view`, `watch` and `download` without a run id took the repository's newest
  run, which after another push is someone else's;
- the runs API could not filter by commit, and its `branch` filter never matched pull
  request runs (their ref is `refs/pull/<n>/head`);
- `rbit pr checks` read "no status yet" as green (exit 0);
- `rbit pr merge` never sent the head it had tested;
- API errors had only prose, and the CLI exited 1 for everything.

## Interfaces as built

**API.** `GET /repos/{owner}/{repo}/actions/runs` and `…/workflows/{id}/runs` take
`head_sha` (full id, or a prefix of 7 to 39 hex digits; anything else is 422). `branch`
also matches runs whose ref is `refs/pull/<n>/head` for an open or closed pull request of
the repository from that branch. Every error body is `{"message", "code", …}`; `code` is
derived from the status (`not_found`, `unauthenticated`, `forbidden`, `blocked` for 405,
`conflict`, `invalid`, `rate_limited`, `unavailable`, `moved`, `internal`) unless the
handler names a precise one: the merge endpoint answers `head_changed` or `base_changed`.
The OpenAPI `Error` schema requires `code`.

**CLI.** A global `--json`, `--fields` and `--remote` (`RBIT_REMOTE`). One compact object on
stdout with `schema` (`rbit.runs/1`, `rbit.run/1`, `rbit.watch/1`, `rbit.prs/1`,
`rbit.pr/1`, `rbit.checks/1`, `rbit.merge/1`, `rbit.error/1`). Exit codes: 0, 1, 2 usage,
3 auth, 4 not found, 5 precondition, 8 pending or timeout, 9 untrustworthy. Without a run
id, run commands resolve the runs of `--sha` or `HEAD`; none yet is `pending` with a
`hint`. `pr checks` with no status is pending. `pr merge` sends `sha` always.

## Files

- `rbit-storage`: `sqlite/actions_run_filter.rs` (the filter, now with `sha`), the
  `runs_where!` clause (branch → pull request runs, `sha` exact or prefix).
- `rbit-services`: `RunQuery::sha`.
- `rbit-web`: `routes/api/actions/run_filters.rs` (query parsing and validation),
  `routes/api/error.rs` (`code`), the merge handler's codes, OpenAPI fragments.
- `rbit-server`: the client split into `agent.rs` (contract), `runs.rs`, `runs_view.rs`,
  `pulls.rs`, `util.rs`; `commands.rs` keeps the argument definitions.

## Tests

- Unit: error codes for every status family and override; `head_sha` validation; exit
  code mapping, error objects and `--fields`; job summaries and which jobs count as
  unsuccessful.
- End to end (`crates/rbit-e2e/tests/e2e/agent_runs.rs`): runs by full id and prefix, an
  unknown commit is an empty list, a branch's pull request run, `not_found` and
  `head_changed` codes, and the CLI's `run view --sha`, `pending` with exit 8 and a hint,
  `run list --fields`, `pr checks` pending, `pr merge --head` refused with exit 5, and a
  missing run with exit 4.

## Left out

Logs by step, the failure digest, `rbit status` and the long-poll watch are later work
packages (1.4 to 1.7). `watch` still polls; it now polls the right runs.
