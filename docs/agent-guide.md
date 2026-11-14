# Driving rbit from a coding agent

## Pull request workflows

Connect a matching rbit client to the instance hosting the repository using the
[client guide](cli.md). Use `--json` for structured output and specify `--hostname`
and `--repo` when the checkout has more than one possible target.

| Command | Use it to |
| --- | --- |
| `rbit pr ensure --head BRANCH --base BASE --title TITLE` | Recover an open PR for the branch pair or create one with the intended title and body. Use a stable request ID when retrying the same request. |
| `rbit pr modernize 42` | Gather the PR's actual base and head, Git divergence, changed-file manifest, and current PR state. |
| `rbit pr takeover 42` | Gather current PR context and evidence before continuing someone else's work. |
| `rbit pr babysit 42` or `rbit pr full-stop 42` | Wait for a handoff or an actionable result from CI, review, or the PR's lifecycle. |
| `rbit pr full-send 42 --head SHA` | Queue an explicitly authorized head and follow it to the landing result. |

### Assess or take over a change

Use `modernize` to see what changed on the PR and how far its actual base moved.
Read the linked issue, affected code, and tests before recommending an update,
rework, or closure. A squash or a different implementation may have solved the
same problem, even if the PR commit is absent from the base's ancestry.

For an authorized same-repository merge update, use `rbit pr update-branch 42
--head SHA` and verify and test the resulting head. For a rebase, work in an
isolated checkout, inspect the rewritten series with `git range-diff`, and guard
the push against the observed remote head with an explicit force-with-lease.

For a takeover, use the observed revisions with the original goal and review
evidence. Inspect any active local checkout before making changes. If it is
tracked by rbit, `rbit task resume BRANCH --json` shows whether it has uncommitted
work. Carry existing commits and contributor credit forward and confirm any
recorded proof still applies to the revision you are continuing.

### Review, hand off, or land

Read the PR's review and comments endpoints for context and use the inbox for
unresolved threads. Verify review findings against the change before updating
the branch. For an authorized reply, use `rbit pr comment 42 --reply-to COMMENT_ID
--body-file FILE --json` and confirm it posted before marking the thread resolved.
An independent reviewer can use the same PR evidence if the agent host supplies
that capability.

Use `babysit` or `full-stop` to observe the PR through a handoff. If a return
shows a new head or base, reassess the evidence for the new revisions. Use
`rbit pr ready 42 --head SHA --mark` for an explicitly requested draft
transition. An existing merge queue entry can land independently. Inspect it
before making changes intended for a human handoff.

Use `full-send` only after the user authorizes landing and the evidence applies
to the observed head. It guards the requested head and checks for unresolved
threads before queueing. The merge queue tests the candidate on the current
base under repository protection. If a wait ends while an entry is active, read
the current queue and PR state before issuing another write.

## Watching a pull request

```bash
rbit pr watch 42 --json
rbit pr watch 42 --cursor <returned-cursor> --timeout 25 --json
```

The first call returns a snapshot. A later call with its cursor returns when the
observed state differs or the wait ends. Check `changed`, PR state, and both
observed revisions. Reassess the diff and relevant evidence if the head or base
moves. Use the review inbox for unresolved feedback and readiness for the server's
merge rules as seen by the current credential. If `readiness_checks_protection`
is false, the evaluation is advisory. Confirm readiness for the intended merger
before reporting a PR ready to hand off. Passing required checks can coexist with
open review threads.

The watcher follows state while the host is running. To resume an agent after it
exits, its host needs a scheduler or another wake-up mechanism.

## Reading CI results

Use the commit you pushed as the identity of the work being checked. A repository's newest
run may belong to another branch or commit. The `rbit` client and the agent API both
distinguish a run that has not started yet from a commit for which no workflow applies.

### From the command line

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

### From the REST API

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
