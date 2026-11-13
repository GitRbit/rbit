# Reading CI results from a coding agent

Use the commit you pushed as the identity of the work being checked. A repository's newest
run may belong to another branch or commit. The `rbit` client and the agent API both
distinguish a run that has not started yet from a commit for which no workflow applies.

## From the command line

After `rbit auth login` and `rbit auth setup-git` (see the [client guide](cli.md)):

```bash
rbit status --json
rbit run watch --sha <commit> --exit-status --timeout 600
rbit run failure --sha <commit>
```

`rbit status` reports the branch head, whether the local `HEAD` has reached the server,
the commit's CI state, its pull request and the next useful action. `run watch` follows the
specified commit through a long poll. It reports `none_expected` when the server knows no
workflow applies; an empty run list immediately after a push is still pending. Watching
the current `HEAD` with its branch also stops with `head_changed` if the branch moves.

Read `run failure` first when CI fails. It returns the failed jobs, termination reasons,
annotations and a short excerpt from the failing step. Use
`rbit run log <job-id> --step <number> --tail 100` for more context. The full command set,
JSON schemas and exit codes are in [the client guide](cli.md#workflow-runs).

For a local change that has not been pushed, `rbit ci run` builds a temporary commit of
the working tree and tests it on a scratch ref. A `--jobs` subset is useful for feedback;
it does not replace the full required checks for a pull request.

## From the REST API

All paths below are under `/api/v1/repos/{owner}/{repo}`. Authenticate with a token that
can read the repository and, for failure details, its logs.

1. `GET /agent/status?branch=<branch>` returns the branch head and its CI state in one
   response. Confirm that `head.sha` is the commit you pushed.
2. `GET /agent/watch?sha=<commit>&branch=<branch>&timeout=25` waits for a change in that
   commit's CI. Send its returned `cursor` on the next request. A completed state, early
   failure, changed branch head or timeout ends the wait; use the response's `done`,
   `head_changed` and `waits_for` fields to decide what happened.
3. `GET /actions/runs?head_sha=<commit>` lists only that commit's runs. If a run failed,
   `GET /actions/runs/{run_id}/failure` returns a concise failure digest. Fetch a single
   step with `GET /actions/jobs/{job_id}/steps/{step}/log` when the digest needs context.

The [Actions guide](actions.md#the-rest-api-and-webhooks) describes trigger
decisions, required checks and failure records. A missing run or status alone is never
evidence that CI passed.
