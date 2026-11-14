# Driving rbit from a coding agent

## Pull request skills

The repository ships [agent skills](../.agents/skills) for use with the rbit client.
Open the checkout in an agent host that loads repository skills from `.agents/skills`.
For a host that uses another directory, copy the skill directories together into its
configured skills directory. Keep the sibling directories together so cross-references
resolve. Install a matching rbit client and use the [client guide](cli.md) to connect it
to the instance hosting your repository.

| Skill | Use it to |
| --- | --- |
| `rbit-modernize` | Assess an older PR against its actual base and recommend an update, targeted rework, or closure. Request an update explicitly to publish a changed head. |
| `rbit-takeover` | Continue an existing PR or branch while preserving the objective, contributor credit, and local work. |
| `rbit-file-pr` | Verify the diff and file or recover a PR for the branch. |
| `rbit-babysit-pr` | Follow checks, reviews, and the PR head until the requested handoff or landing condition. |
| `rbit-full-stop` | Verify and file a PR, then hand it back for human merge. |
| `rbit-full-send` | Verify and land a PR through the merge queue when explicitly authorized. |
| `rbit-review` | Request independent review of a fixed revision from one reviewer or an explicitly requested council. Requires a host with the requested delegation capability. |
| `rbit-pr-comments` | Draft evidence-based PR comments and verify posting before resolving a thread when authorized. |

For example, ask “Modernize rbit PR #42 and tell me whether it still belongs on
its current base” for an assessment. Ask “Babysit rbit PR #42 until it is ready
for me to merge” to follow it through review and CI.

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
