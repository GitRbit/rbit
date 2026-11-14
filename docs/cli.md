# The rbit command-line client

The `rbit` binary is also a client for an rbit instance's REST API, in the manner of GitHub's
`gh`: it lists and follows workflow runs, reads their logs, runs workflows, downloads
artifacts and handles pull requests. It is included in the server binary for every release
platform. Use a client release that matches the version running on your server for the
documented API behavior.

## Logging in

Create a personal access token (**Settings → Access tokens**; `read` for looking, `write`
to run, re-run, cancel and merge, `admin` for settings such as `actions/permissions`) and store it:

```bash
rbit auth login --hostname https://git.example.com --with-token < token.txt
rbit auth status
```

Logins are kept in `hosts.toml` in your configuration directory (`$XDG_CONFIG_HOME/rbit`,
`~/.config/rbit`, or `%APPDATA%\rbit` on Windows; `RBIT_CONFIG_DIR` overrides it), readable
only by you on Unix. `RBIT_TOKEN` (or `GH_TOKEN` holding an rbit token; a GitHub token there
is `gh`'s and is never sent) in the environment wins over a stored
login, which suits CI; it is sent only to the instance `--hostname` (or `RBIT_HOST`) names or
one with a stored login, never to a host only the clone's remote names, so CI sets
`RBIT_HOST` too. `rbit auth token` prints a stored token for scripts, and
`rbit auth logout --hostname …` forgets it.

`rbit auth setup-git --hostname https://git.example.com` makes Git use that login for the
instance: it installs `rbit auth git-credential` as the credential helper for that URL in
your global Git configuration (resetting other helpers for it). A push then never stops at
a password prompt: without a login the helper tells Git to stop at once
(`rbit: no login for …`), and a revoked token fails with `Authentication failed`.

## Which repository

Commands work on `--repo owner/name` (`-R`), or on the repository a Git remote of the
current directory points to, on the instance of that remote: `origin` when it is on an
instance you are logged in to (or the one `--hostname` names), otherwise the one remote that
is, as in a mirror whose `origin` is on GitHub. When no remote is on such an instance, or
several are, the command says so instead of asking for another host's path. `--remote NAME`
(or `RBIT_REMOTE`) names the remote; `rbit ci run` and `rbit task` push and fetch through
the remote chosen. `--hostname` (or `RBIT_HOST`) picks the instance otherwise; with one
login, it is that one.

## Output for programs

Every `run`, `pr`, `workflow`, `artifact` and `api` command takes `--json`. It then prints
exactly one compact JSON object on stdout, whose `schema` names its shape and version
(`rbit.runs/1`, `rbit.run/1`, `rbit.watch/1`, `rbit.prs/1`, `rbit.pr/1`, `rbit.checks/1`,
`rbit.merge/1`); progress goes to stderr. A field may be added to a schema without a new
version; a field removed or changed means a new version. `--fields id,sha` keeps only those
fields of each listed item.

A failure with `--json` is an object too:
`{"schema":"rbit.error/1","code":"pending","message":"…","hint":"rbit run watch --sha …"}`.
`code` is the server's (every API error body has one: `not_found`, `unauthenticated`,
`forbidden`, `conflict`, `head_changed`, `base_changed`, `blocked`, `invalid`,
`rate_limited`, `unavailable`, …) or the client's own (`pending`, `timeout`, `usage`). A
`hint` is a command that can be run as it is. A read the server throttles (`429` with a
`Retry-After` of at most 30 seconds) is tried again after that wait, up to three times, so a
long `run watch` survives a busy moment; a write is never repeated.

The exit code says what kind of answer it was, with or without `--json`:

| Exit | Meaning |
|---|---|
| 0 | success, or green |
| 1 | failed, or red |
| 2 | usage: a bad argument or value |
| 3 | not signed in, or not allowed |
| 4 | not found |
| 5 | a precondition failed: the head or base moved, a conflict |
| 8 | pending, or a wait that timed out |
| 9 | the answer cannot be trusted: a log cut short, the server unavailable |

## Where am I

```bash
rbit status [--branch BRANCH]
```

One call for the state of a branch (the current one by default): its head on the server and
whether your checkout's `HEAD` is that commit, how far it is ahead of and behind its base,
the CI of exactly that commit, the base's CI, the open pull request from the branch, the
issue a `feature/<n>-…` branch works on, and `next`, the command worth running now. The CI
`state` is one of `none_expected` (with the reason: no workflow files, Actions off),
`not_started`, `queued`, `running`, `failure` (as soon as one job failed, even while others
run), `indeterminate` (nobody can say why the jobs failed: a lost runner, a log cut short),
`cancelled` and `success`, so "nothing yet" is never confused with "nothing expected".

## Workflow runs

```bash
rbit run list [--workflow ci.yml] [--branch main] [--sha COMMIT] [--event push] [--status failure] [-L 50]
rbit run view [RUN | --sha COMMIT] [--log | --log-failed] [--job JOB] [--exit-status]
rbit run watch [RUN | --sha COMMIT] [--exit-status] [--timeout SECONDS]
rbit run rerun [RUN | --sha COMMIT] [--failed | --job JOB]
rbit run cancel RUN
rbit run download [RUN | --sha COMMIT] [-n NAME]... [-p PATTERN] [-D DIR]
rbit run failure [RUN | --sha COMMIT]
rbit run log JOB --step N [--tail K | --from LINE [--lines K]] [--attempt A] [--raw]
```

`run failure` is the one to read first when a run is red: for each failed job, how it
ended (the reason, the step and its exit code, whether its log is complete), its error
annotations (`path:line: message`) and the last 30 lines of the step that ended it, cleaned
of timestamps and colours, with repeated lines folded. Without an id it covers every failed
run of `--sha` or `HEAD`, including a run still going once one of its jobs failed (as
`status` and `run watch` report it). It exits with 1 when something failed, 0 when nothing
did, 8 while a run has not finished and nothing failed yet, and 9 when a failure's log is incomplete or nobody recorded
its cause (a lost runner): then the answer is not the whole story. `run log` reads one
step's lines, the last 100 by default, of any attempt.

Runs are found by the commit they tested. Without a run id, `view`, `watch` and `download`
work on the runs of `--sha` (a commit id, or a prefix of at least 7 hex digits), or of the
current checkout's `HEAD`: never on "the newest run" of the repository, which may be another
branch's. A commit with no run yet is `pending` (exit 8) with a hint to watch it. `watch` on a
commit long-polls the server: it returns as soon as a job fails (exit 1 with
`--exit-status`), when everything passed (0), when no run will come (`none_expected`, 0),
when nobody can say why a job failed (`indeterminate`, 9), when a job waits for `runs-on`
labels no runner of the repository has (`no_runner`, 8: it would never start; `rbit status`
gives the reason `no_runner` and the labels in `waits_for`), when the branch moves on to a
newer commit (`head_changed`, exit 5, naming it; watching `HEAD` follows the current branch,
`--branch` names another), or after `--timeout` seconds (exit 8). `--branch` also matches the runs of pull requests from that
branch. `--log-failed` prints the logs of the jobs that did not succeed (failed, timed out
or cancelled), each line prefixed with its job's name. `rerun --job` wins over `--failed`
when both are given; `rerun` without a run id reruns every failed or cancelled run of `--sha`
or `HEAD`; while its run has not finished it says so (`pending`, exit 8) rather than that
nothing failed, and so does `rerun RUN` for a run that has not finished.

A job that uploads its JUnit report as an artifact named `rbit-test-report`, or that name
with a suffix such as `rbit-test-report-${{ matrix.os }}` since a run's artifact names are
unique (with `actions/upload-artifact`, `if: always()`), gets its failed tests in `run failure`: the test,
where it failed and its message, and `flaky` when the same test passed at the same commit in
another attempt or run of a job of the same name (the server keeps the ids of the first 2,000
passed tests of each report for this). Each failed job also says whether its failures are `new` or
`recurring` against the previous run of the workflow on that ref, and whether the default
branch fails the same way (`also_failing_on_base`): then the failure is not yours. `--exit-status` exits with 1 when a run did not succeed and 8 while one
has not finished; a skipped run (every job's `if` was false) counts as passed, here and in
`run failure`.

## CI without a branch

```bash
rbit ci run [--jobs JOB,JOB] [--commit COMMIT] [--no-watch] [--timeout SECONDS]
```

Tests what is in your working tree, committed or not, without making a branch or moving
`HEAD`: it builds a commit of the tree (tracked and untracked files; what `.gitignore`
leaves out stays out) through a temporary index, pushes it to `refs/scratch/<login>/<id>`
and watches its run like `rbit run watch --sha`, with the same exit codes. `--commit` tests
an existing commit instead. `--jobs` runs only the named jobs and the jobs they `needs`, by
adding an `Rbit-CI-Jobs: lint, tests` trailer to the commit it builds; a workflow with none
of them starts no run, and when no workflow has any of them the commit reads `none_expected`
at once (the watch ends with exit 0) instead of waiting for runs that never come. The push uses Git's credentials for the instance (`rbit auth
setup-git`).

Scratch refs are not branches: no branch list shows them and no pull request comes from
them. You push only under your own login, hold at most 20 at a time in a repository, and
the server deletes each one a day after it was pushed. Workflows see a scratch ref as the
branch `scratch/<login>/<id>`, so `on: push` with `branches: ["**"]` or no filter runs on it
and `branches: [main]` does not. A subset run reports only its jobs, so it never stands in
for the full set of required checks.

## Bisecting from recorded results

```bash
rbit bisect GOOD BAD [--workflow .rbit/workflows/ci.yml]
```

Reads what CI already recorded for the commits on `BAD`'s first-parent line down to `GOOD`
(at most 1,000) before anything runs: a commit passed (a successful run, no failed one),
failed, or is untested. It answers the newest commit that passed before the first that
failed, that failure, how many untested commits lie between them and the one to test next
(the middle); with none left, the commit that broke it. `--workflow` counts one workflow's
runs only. `GOOD` must be on `BAD`'s first-parent line: reversed arguments, a commit merged
in from a side branch or an unrelated one are refused (exit 2) with the reason. When CI's
records contradict the bounds (`GOOD` has a failed run, or `BAD` only passing ones), the
answer still comes, with `warnings` (printed to stderr) saying so.

## Landing

```bash
rbit pr ensure [--head BRANCH] [--base main] [--title TITLE] [--body BODY] [--ready] [--request-id ID]
rbit pr ready N [--head SHA] [--mark] [--watch]
rbit pr queue N [--head SHA] [--squash | --rebase] [--watch]
rbit pr dequeue N
rbit pr update-branch N --head SHA
rbit pr revert COMMIT [--base main]
```

`ensure` returns the pull request for the branch or opens a draft; with `--request-id`, a
retry gets the first answer. `ready` evaluates the head with the merge's own rules (exit 0
ready, 1 a required check failed, 8 still expected or running) and `--mark` makes a draft
ready. `queue` lands the pull request only after its merge on the current base passed the
required checks, exactly that commit; `--watch` waits (exit 0 landed, 1 failed, 5 taken
out). `update-branch` merges the base into the head branch if it is still `--head`; `revert`
opens a pull request that undoes a commit.

## Tasks

```bash
rbit task start ISSUE|BRANCH [--base main] [--dir PATH]
rbit task resume BRANCH
rbit task finish BRANCH
```

`start 31` takes issue 31 (it fails with `conflict`, naming the holder, when someone else
has it), fetches the base and adds a Git worktree beside the current checkout on a new
branch `feature/31-<title>` from exactly that commit. `start NAME` does the same for a
branch without an issue. `resume` reports the worktree's head, its uncommitted changes,
whether the server has that head and its CI. `finish` removes the worktree and refuses
while it has uncommitted changes; the branch stays.

## Issues

```bash
rbit issue list [--state open|closed|all] [--label NAME]... [--mine] [-L 20]
rbit issue view NUMBER
rbit issue create --title "…" [--body "…" | --body-file FILE] [--label NAME]...
rbit issue comment NUMBER [--body "…" | --body-file FILE]
rbit issue close NUMBER [--reason completed|not_planned]
rbit issue label NUMBER LABEL... [--remove]
rbit issue take NUMBER
rbit issue search WORD... [--state open|closed|all]
```

`list` leaves pull requests out and gives one short row per issue; `view` gives the body
and the three newest comments, each cut at 4,000 characters with `--json`. `--body-file -`
reads standard input. `take` assigns the issue to you only if nobody holds it, and fails
with `conflict` (exit 5) naming the holder otherwise. `search` finds issues and pull
requests whose title or body has every word, before you file another.

## Workflows and artifacts

```bash
rbit workflow list
rbit workflow run deploy.yml [--ref v1.2.0] [-f environment=staging]...
rbit artifact list [--run RUN]
rbit artifact download NAME_OR_ID [--run RUN] [-D DIR]
```

A downloaded artifact is unpacked into the directory, never outside it.

## Pull requests

```bash
rbit pr create --title "…" [--body "…"] [--base main] [--head my-branch] [--draft]
rbit pr list [--state open|closed|all] [--head BRANCH]
rbit pr view NUMBER
rbit pr comment NUMBER (--body TEXT | --body-file FILE) [--reply-to COMMENT_ID] [--json]
rbit pr merge NUMBER [--merge | --squash | --rebase] [--head COMMIT]
rbit pr checks NUMBER [--watch]
rbit pr watch NUMBER [--cursor CURSOR] [--timeout SECONDS] [--json]
```

`pr create` takes the current branch as the head unless `--head` names one. `pr merge`
always names the head it may merge: `--head`, or the head it reads just before merging. If
the branch moved since, the merge is refused with `head_changed` (exit 5) instead of landing
a commit nobody tested. `pr checks` lists the commit statuses of the pull request's head,
Actions jobs included, and exits with 0 when all passed, 1 when one failed and 8 while some
are pending, as `gh` does. No status at all is pending too (exit 8): straight after a push
the checks may not have started, and that is not green.

`pr comment` posts a top-level discussion comment or replies to an inline review
comment with `--reply-to`. Use `--body-file -` to read the text from standard
input. JSON results use `rbit.pr-comment/1` and include the posted comment's ID
and URL. Thread resolution is a separate operation.

`pr watch` reads the PR's current state, head and base, merge readiness, reviews,
unresolved threads, and queue state. Pass the returned cursor to wait for a changed
snapshot. The wait defaults to 25 seconds, with 0 for an immediate read. In JSON
mode, read `changed` to distinguish a changed snapshot from an unchanged timeout.
A read-only caller can watch activity. When `readiness_checks_protection` is false,
its readiness result is advisory. Discussion activity is represented by comment count.
A successful read exits 0 regardless of the PR's readiness. The matching REST
route is `GET /api/v1/repos/{owner}/{repo}/pulls/{number}/watch`.

### Agent workflows

```bash
rbit pr modernize NUMBER [--json]
rbit pr takeover NUMBER [--json]
rbit pr babysit NUMBER [--timeout SECONDS] [--json]
rbit pr full-stop NUMBER [--timeout SECONDS] [--json]
rbit pr full-send NUMBER --head SHA [--squash | --rebase] [--timeout SECONDS] [--json]
```

`modernize` reads Git divergence and a changed-file manifest against the PR's
observed revisions, along with its current state. Use those facts to decide whether
the branch still fits, needs rework, or has been superseded. `takeover` gathers PR
context and evidence for a handoff. Both are read-only and return `pull`,
`comparison`, `manifest`, `watch`, and `disposition` in JSON
(`rbit.pr-modernize/1` or `rbit.pr-takeover/1`). A disposition of
`head_already_in_base` identifies ancestry. `needs_assessment` calls for
reviewing the current code and the PR's goal. Other dispositions are
`merged`, `closed`, and `base_missing`. Rerun the command if it reports that
the PR moved while collecting evidence.

`babysit` waits for the PR to be ready for handoff or to reach an actionable
state. `full-stop` is an alias for `babysit`. Both are read-only. Inspect the
returned outcome and revisions before deciding to fix CI, address review, or
return the PR to a human. The default wait is 3,600 seconds. JSON results use
`rbit.pr-babysit/1` with an `outcome` and the latest watch snapshot.

| Exit | Babysit outcome |
| --- | --- |
| 0 | `ready` or `merged` |
| 1 | `checks_failed`, `queue_failed`, `closed`, `review_changed`, or `blocked` |
| 5 | `head_changed` or `base_changed` |
| 8 | `queued` or `timeout` |

`full-send` takes a full head SHA and waits up to 7,200 seconds by default.
Before queueing, it checks the observed readiness and unresolved threads.
The queue request includes the expected head and target branch. The merge
queue tests its candidate against the current base and applies the repository's
merge policy.
A queue result uses `rbit.queue/1`. Exit 0 means landed, 1 means failed, 5
means removed or a precondition changed, and 8 means still pending. A timeout
can leave an active entry. Inspect the returned queue state and the live PR
before making another request.

For review context, use `rbit api` under `repos/{owner}/{repo}` with
`pulls/NUMBER/reviews`, `pulls/NUMBER/comments`, or `pulls/NUMBER/inbox`.
To set a thread's resolution, use
`PUT pulls/NUMBER/comments/THREAD_ROOT_ID/resolution` with a JSON boolean
`resolved`.

## Any API call

```bash
rbit api repos/{owner}/{repo}/actions/permissions/workflow
rbit api -X PUT repos/{owner}/{repo}/actions/permissions/workflow -f default_workflow_permissions=write
rbit api repos/{owner}/{repo}/dispatches -f event_type=deploy
```

`{owner}` and `{repo}` are the current repository's. `-f key=value` sends a string field,
`-F key=value` keeps `true`, `false`, `null` and numbers typed, `--input FILE` (or `-`)
sends a body as it is; with fields the method is POST unless `-X` says otherwise, and a
GET sends them as the query. JSON answers are printed indented; the exit code is 1 when the
server refused.

The REST API's own additions for agents: `GET …/actions/runs?head_sha=COMMIT` lists the runs
of one commit (an empty list means none has started), `branch=` includes the runs of the
branch's pull requests, and every error body carries a `code` beside its `message`.
