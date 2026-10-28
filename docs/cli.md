# The rbit command-line client

The `rbit` binary is also a client for an rbit instance's REST API, in the manner of GitHub's
`gh`: it lists and follows workflow runs, reads their logs, runs workflows, downloads
artifacts and handles pull requests. It is the same binary as the server, so every release
has it for every platform, and it always matches its server's API.

## Logging in

Create a personal access token (**Settings → Access tokens**; `read` for looking, `write`
to run, re-run, cancel and merge) and store it:

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
`origin` remote points to, on the instance of that remote. `--hostname` (or `RBIT_HOST`)
picks the instance otherwise; with one login, it is that one.

## Workflow runs

```bash
rbit run list [--workflow ci.yml] [--branch main] [--event push] [--status failure] [-L 50] [--json]
rbit run view [RUN] [--log | --log-failed] [--job JOB] [--json] [--exit-status]
rbit run watch [RUN] [--exit-status]
rbit run rerun RUN [--failed | --job JOB]
rbit run cancel RUN
rbit run download [RUN] [-n NAME]... [-p PATTERN] [-D DIR]
```

Without a run id, `view`, `watch` and `download` use the newest run. `--log-failed` prints
the logs of the jobs whose conclusion is `failure` (not cancelled ones), each line prefixed
with its job's name. `rerun --job` wins over `--failed` when both are given. `--exit-status`
exits with 1 when the run did not succeed, so `rbit run watch --exit-status` waits for CI in
a script.

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
rbit pr list [--state open|closed|all]
rbit pr view NUMBER [--json]
rbit pr merge NUMBER [--merge | --squash | --rebase]
rbit pr checks NUMBER [--watch]
```

`pr create` takes the current branch as the head unless `--head` names one. `pr checks`
lists the commit statuses of the pull request's head, Actions jobs included, and exits with
0 when all passed, 1 when one failed and 8 while some are pending, as `gh` does; with no
status at all it prints "no checks reported" and exits with 0. `--json` prints whole objects:
unlike `gh`, it takes no field list, `--jq` or `--template`.

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
