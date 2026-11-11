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

## Workflow runs

```bash
rbit run list [--workflow ci.yml] [--branch main] [--sha COMMIT] [--event push] [--status failure] [-L 50]
rbit run view [RUN | --sha COMMIT] [--log | --log-failed] [--job JOB] [--exit-status]
rbit run watch [RUN | --sha COMMIT] [--exit-status] [--timeout SECONDS]
rbit run rerun RUN [--failed | --job JOB]
rbit run cancel RUN
rbit run download [RUN | --sha COMMIT] [-n NAME]... [-p PATTERN] [-D DIR]
```

Runs are found by the commit they tested. Without a run id, `view`, `watch` and `download`
work on the runs of `--sha` (a commit id, or a prefix of at least 7 hex digits), or of the
current checkout's `HEAD`: never on "the newest run" of the repository, which may be another
branch's. A commit with no run yet is `pending` (exit 8) with a hint to watch it; `watch`
waits for its first run to appear, follows every run of the commit, and gives up after
`--timeout` seconds (exit 8). `--branch` also matches the runs of pull requests from that
branch. `--log-failed` prints the logs of the jobs that did not succeed (failed, timed out
or cancelled), each line prefixed with its job's name. `rerun --job` wins over `--failed`
when both are given. `--exit-status` exits with 1 when a run did not succeed and 8 while one
has not finished.

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
