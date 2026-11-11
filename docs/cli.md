# The rbit command-line client

The `rbit` binary is also a client for an rbit instance's REST API, in the manner of GitHub's
`gh`: it lists and follows workflow runs, reads their logs, runs workflows, downloads
artifacts and handles pull requests. It is the same binary as the server, so every release
has it for every platform, and it always matches its server's API.

## Logging in

Create a personal access token (**Settings → Access tokens**; `read` for looking, `write`
to run, re-run, cancel and merge, `admin` for settings such as `actions/permissions`) and store it:

```bash
rbit auth login --hostname https://git.example.com --with-token < token.txt
rbit auth status
```

Logins are kept in `hosts.toml` in your configuration directory (`$XDG_CONFIG_HOME/rbit`,
`~/.config/rbit`, or `%APPDATA%\rbit` on Windows; `RBIT_CONFIG_DIR` overrides it), readable
only by you on Unix. `RBIT_TOKEN` (or `GH_TOKEN`) in the environment wins over a stored
login, which suits CI. `rbit auth token` prints a stored token for scripts, and
`rbit auth logout --hostname …` forgets it.

`rbit auth setup-git --hostname https://git.example.com` makes Git use that login for the
instance: it installs `rbit auth git-credential` as the credential helper for that URL in
your global Git configuration (resetting other helpers for it). A push then never stops at
a password prompt: without a login the helper tells Git to stop at once
(`rbit: no login for …`), and a revoked token fails with `Authentication failed`.

## Which repository

Commands work on `--repo owner/name` (`-R`), or on the repository the current directory's
`origin` remote points to, on the instance of that remote. `--remote NAME` (or
`RBIT_REMOTE`) reads another remote instead, for a clone whose `origin` is elsewhere.
`--hostname` (or `RBIT_HOST`) picks the instance otherwise; with one login, it is that one.

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
`hint` is a command that can be run as it is.

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
rbit run rerun RUN [--failed | --job JOB]
rbit run cancel RUN
rbit run download [RUN | --sha COMMIT] [-n NAME]... [-p PATTERN] [-D DIR]
rbit run failure [RUN | --sha COMMIT]
rbit run log JOB --step N [--tail K | --from LINE [--lines K]] [--attempt A] [--raw]
```

`run failure` is the one to read first when a run is red: for each failed job, how it
ended (the reason, the step and its exit code, whether its log is complete), its error
annotations (`path:line: message`) and the last 30 lines of the step that ended it, cleaned
of timestamps and colours, with repeated lines folded. Without an id it covers every failed
run of `--sha` or `HEAD`. It exits with 1 when something failed, 0 when nothing did, 8
while a run has not finished, and 9 when a failure's log is incomplete or nobody recorded
its cause (a lost runner): then the answer is not the whole story. `run log` reads one
step's lines, the last 100 by default, of any attempt.

Runs are found by the commit they tested. Without a run id, `view`, `watch` and `download`
work on the runs of `--sha` (a commit id, or a prefix of at least 7 hex digits), or of the
current checkout's `HEAD`: never on "the newest run" of the repository, which may be another
branch's. A commit with no run yet is `pending` (exit 8) with a hint to watch it. `watch` on a
commit long-polls the server: it returns as soon as a job fails (exit 1 with
`--exit-status`), when everything passed (0), when no run will come (`none_expected`, 0),
when nobody can say why a job failed (`indeterminate`, 9), when the branch moves on to a
newer commit (`head_changed`, exit 5, naming it; watching `HEAD` follows the current branch,
`--branch` names another), or after `--timeout` seconds (exit 8). `--branch` also matches the runs of pull requests from that
branch. `--log-failed` prints the logs of the jobs that did not succeed (failed, timed out
or cancelled), each line prefixed with its job's name. `rerun --job` wins over `--failed`
when both are given. `--exit-status` exits with 1 when a run did not succeed and 8 while one
has not finished.

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
of them starts no run. The push uses Git's credentials for the instance (`rbit auth
setup-git`).

Scratch refs are not branches: no branch list shows them and no pull request comes from
them. You push only under your own login, hold at most 20 at a time in a repository, and
the server deletes each one a day after it was pushed. Workflows see a scratch ref as the
branch `scratch/<login>/<id>`, so `on: push` with `branches: ["**"]` or no filter runs on it
and `branches: [main]` does not. A subset run reports only its jobs, so it never stands in
for the full set of required checks.

## Issues

```bash
rbit issue list [--state open|closed|all] [--label NAME]... [--mine] [-L 20]
rbit issue view NUMBER
rbit issue create --title "…" [--body "…" | --body-file FILE] [--label NAME]...
rbit issue comment NUMBER [--body "…" | --body-file FILE]
rbit issue close NUMBER [--reason completed|not_planned]
rbit issue label NUMBER LABEL... [--remove]
```

`list` leaves pull requests out and gives one short row per issue; `view` gives the body
and the three newest comments, each cut at 4,000 characters with `--json`. `--body-file -`
reads standard input.

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
rbit pr merge NUMBER [--merge | --squash | --rebase] [--head COMMIT]
rbit pr checks NUMBER [--watch]
```

`pr create` takes the current branch as the head unless `--head` names one. `pr merge`
always names the head it may merge: `--head`, or the head it reads just before merging. If
the branch moved since, the merge is refused with `head_changed` (exit 5) instead of landing
a commit nobody tested. `pr checks` lists the commit statuses of the pull request's head,
Actions jobs included, and exits with 0 when all passed, 1 when one failed and 8 while some
are pending, as `gh` does. No status at all is pending too (exit 8): straight after a push
the checks may not have started, and that is not green.

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
