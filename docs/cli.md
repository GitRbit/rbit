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

The CLI accepts HTTP(S) instance URLs with an optional deployment path. API requests
stay on that origin and within that path. Redirects are returned to the caller without
following them; configure the final instance URL when a proxy redirects requests.
Credentials in URLs, fragments and ambiguous path traversal are rejected.

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

## Plugin package recording

`rbit plugin install FILE [--operation-id ID] --hostname URL` streams a canonical
package archive using an active administrator's Admin-scoped PAT. The fixed host
package root must already be privately and durably provisioned. This records bytes
and a version pin without enabling code, approving trust or issuing grants.

The CLI prints a random opaque operation ID before sending the archive. Retain it:
`rbit plugin operation ID --hostname URL` inspects the durable journal after a lost
response. Pass the same ID explicitly to retry the same actor/archive; there is no
automatic retry or redirect replay. GET 404 can mean staging has not yet created a
journal row, so it does not prove absence of active work or rollback. An awaiting-durability
result is pending and exits 8; complete exits 0, a recorded error exits
1. Responses are capped at 64 KiB and the archive is streamed in bounded buffers.

Windows servers currently refuse uploads until supported host root verification is
available; operation inspection remains available.

## Passive plugin policy administration

`rbit plugin policy` uses an active administrator's Admin-scoped PAT against the
selected instance. These commands record policy metadata only. Successful policy
results report execution unavailable; approving native full service-user trust or selecting
an archive never starts a worker, enables a plugin or issues a broker grant.

| Command | Purpose |
| --- | --- |
| `rbit plugin policy status` | Inspect the gate; never initialize it. |
| `rbit plugin policy prepare` | Explicitly initialize a virgin policy ledger. |
| `rbit plugin policy inspect PLUGIN_ID` | Inspect current candidate and review state. |
| `rbit plugin policy inspect-package PLUGIN_ID FULL_VERSION ARCHIVE_DIGEST` | Read authoritative declarations for one installed archive. |
| `rbit plugin policy review --request FILE [--operation-id ID]` | Record an explicit review decision. |
| `rbit plugin policy select --request FILE [--operation-id ID]` | Select the exact candidate under the current allowed review. |
| `rbit plugin policy clear --request FILE [--operation-id ID]` | Clear selection while retaining its revision; does not revoke review. |
| `rbit plugin policy operation ID` | Recover an immutable receipt. |
| `rbit plugin policy renew --request FILE [--operation-id ID]` | Explicitly renew a sealed epoch within finite retention capacity. |
| `rbit plugin policy renewal-operation ID` | Recover an immutable renewal receipt. |

Use `--hostname URL` (or `RBIT_HOST`) consistently for all commands in an operation.
Prepare is an explicit change, not a prerequisite automatically performed by reads
or mutations. It cannot reopen a sealed gate. Before constructing a request file,
inspect the exact installed package and gate and choose the decision explicitly.
Request files contain strict SDK request JSON, without the HTTP command envelope;
unknown fields, invalid identities and noncanonical revision values are rejected.
Files and the resulting command envelope are each limited to 64 KiB. Current host
policy rejects all nonempty grants. Full version strings include SemVer build
metadata; digest changes never inherit approval.

Review/select/clear print a supplied or randomly generated operation key before
sending. Retain it and recover with `rbit plugin policy operation ID` on the same
instance, plus `rbit plugin policy status` and candidate inspection. A lost prepare
response instead recovers through status because prepare has no receipt. A 404
receipt does not prove rollback: a permanent capacity seal can commit without a
receipt. Timeouts/errors can also leave an owned transaction completing after the
HTTP waiter has gone away. There is no automatic retry, redirect replay or
server-provided URL follow.

Applied and historical-receipt outcomes exit 0, but historical output explicitly
states no new approval or selection was applied. Its summary retains the original
review intent or selected review revision, which may differ from current policy.
Structured capacity/already-sealed outcomes preserve their JSON evidence and exit
1. Other non-success responses also exit nonzero without claiming rollback.
Responses are bounded to 64 KiB. Native intent acknowledges full service-user OS
access; it does not create a broker sandbox. Wasm intent never permits native
fallback. Runtime admission and worker enforcement remain subsequent stages.

Renewal request files contain strict SDK renewal JSON, without a command envelope,
and have a separate 1 KiB complete-file limit. Renew never prepares implicitly,
chooses an epoch, inherits reviews or selections, or enables execution. It prints
a known key before the request; recover with `rbit plugin policy renewal-operation
-- ID` using the same instance (`--hostname`), plus policy status. Historical
renewal receipts report evidence only and never open another epoch. A structured
`retention_full` response exits 1 and preserves the sealed gate: the finite limit
of two retired epochs requires a future explicit export/pruning maintenance
protocol. Missing receipts and transport errors do not prove rollback.

## Local package store setup

On Unix, stop the server before provisioning a new store. Run as the service
account with the same deployment configuration:

```bash
rbit --config /etc/rbit/rbit.toml package-store provision
rbit --config /etc/rbit/rbit.toml package-store check
```

The configured data directory must already exist, have trusted ownership and stable
ancestors, and forbid group/world writes. Provision creates only missing `plugins`
and `packages` directories with mode 0700 and synchronizes them and their parents.
Existing unsafe paths or permissions are refused. Start the server after provision
completes. Check inspects permissions; it does not prove earlier crash durability.
These local commands resolve storage configuration without opening SQLite or redb.
Windows root verification/provisioning is unsupported; this command cannot make
Windows HTTP uploads available.
