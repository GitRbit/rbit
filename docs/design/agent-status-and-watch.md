# Agent loop: status and watch

Status: built. Work packages 1.6 and 1.7: a fresh session asks one question and knows
where the work stands, and an agent waits on CI without polling and without missing the
moment a job fails or the branch moves on.

## Interfaces as built

- `GET /repos/{owner}/{repo}/agent/status?branch=` (`rbit.status/1`): `head` (sha, base,
  `divergence` with merge base, ahead and behind, counted without diffing), `ci` (`state`,
  `reason`, `for_sha`, runs, job counts, failed jobs as in the failure digest), `base`
  (branch, sha, the state of its newest CI), `pr` (number, draft, base, head sha, whether
  the server's head is the PR's), `task` (`feature/<n>-…` → issue n) and `next`.
- CI states: `none_expected` (no workflow files at the commit, or Actions off),
  `not_started`, `queued`, `running`, `failure` (any job failed, even with others running),
  `indeterminate` (every failed job's account is incomplete), `cancelled`, `success`.
- `GET /repos/{owner}/{repo}/agent/watch?sha=&branch=&cursor=&timeout=` (`rbit.watch/1`):
  answers when the CI's fingerprint (state, counts, run statuses, failed jobs) differs from
  `cursor`, when `branch` no longer points at `sha` (`head_changed` with the newer commit
  and a `next` command), at once when the commit is `done` and all its runs finished, or
  after `timeout` (default and at most 25 s, below the API's 30 s request timeout, which
  otherwise answered an empty 408 first). It looks once a
  second and opens the repository with the caller's rights on every look.
- CLI: `rbit status [--branch]` (adds `local.pushed`: whether the checkout's `HEAD` is the
  server's head). `rbit run watch` on a commit uses the long-poll; exit 0, 1, 5
  (`head_changed`), 8 (`timeout`), 9 (`indeterminate`).

## Files

`rbit-git/src/divergence.rs`, `rbit-services/src/actions/status.rs` (`branch_status`,
`commit_ci`), `workflow_paths_at` in `dispatch.rs`, `rbit-web/src/routes/api/actions/
agent_status.rs`, `rbit-server/src/client/status.rs`, OpenAPI.

## Tests

Unit: issue numbers from branch names, the next command per state, the cursor moving only
when what a watcher cares about moves. End to end (`agent_runs.rs`): status of a branch
with queued runs (head, `queued`, ahead 1, its pull request, `next`, under 1.5 KB); the CLI's
`status`; a first watch answers, an unchanged one waits out its timeout, a newer push ends
it with `head_changed`, as does the CLI with exit 5; a commit without workflow files is
`none_expected` with its reason.

## Left out

A `paths` filter that skips a commit reads `not_started` until work package 2.2 records
which workflows apply to each commit. The watch looks once a second instead of waking on
the scheduler's notification; budgets per token are left to the server's rate limit.
`superseded` comes with 2.1, queue position with 2.6.
