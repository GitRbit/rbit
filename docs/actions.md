# rbit Actions

rbit Actions runs workflows from your repository on **rbit-runner**, a separate program you run
on machines you choose. The rbit server plans the jobs, hands them out, keeps their logs and
artifacts, and shows the results. Builds never run inside the server.

Workflows use a subset of GitHub Actions' syntax, so most simple workflows run unchanged. This
guide covers turning Actions on, registering a runner, the workflow syntax rbit supports and
how it differs from GitHub. Operators should also read
[Runners in deployment.md](deployment.md#actions-and-runners).

## Turning it on

1. An administrator opens **Admin → Plugins** and enables **Actions**, without restarting
   the server. `[actions] enabled` remains a startup default until runtime intent is saved;
   saved intent survives restart and overrides that default. A core-only build excludes
   Actions and shows it as unavailable.
2. In a repository, open **Settings → Actions** and turn on **Run workflows in this
   repository**. `[actions] enable_for_new_repositories` turns it on for new repositories.
3. Register a runner (see below), then push a workflow under `.rbit/workflows/`.

The **Actions** tab shows the repository's runs. A run page lists its jobs in `needs` order
with Cancel, Re-run failed, Re-run all and per-job re-run buttons for writers. A job page
shows each step's log as it runs and links to the raw log. These pages refresh themselves
every few seconds while something on them runs (the newest runs also look for new runs every
10 seconds), and a step you open or close on a job page stays that way. Each job also
reports a commit status, so required checks and the pull request merge box see Actions like
any other CI.

Disabling Actions immediately stops new admission, removes navigation and API documentation,
and cancels readers and uploads. The host waits for admitted work, then cancels unfinished
runs and tasks, revokes task tokens and reports terminal checks. History, repository settings,
runner registrations, encrypted secrets and retained files stay on disk. Re-enabling creates
a fresh activation; old task credentials remain revoked. Existing runner installations may
need restarting after receiving the disabled endpoint's `404` response.

Cleanup is bounded. If work cannot stop in time, Plugins reports incomplete cleanup and
offers a retry; it does not report successful unload or start an overlapping activation.
Interrupted cancellation and terminal status reporting are journaled for recovery. New
runners also cancel local execution after 120 seconds without an accepted state heartbeat;
an offline older runner may continue its process, but its server credentials are revoked.

Other Actions configuration changes still take effect on server restart. Runtime disable
does not uninstall the compiled plugin; the minimal server build excludes its implementation
and parser entirely. See [plugin core](plugin-core.md) for build commands and lifecycle details.

## Runners

A runner takes jobs from one repository, from every repository of an owner (a user or an
organisation), or — for runners an administrator registers — from the owners the
administrator allows.

1. Open the runners page: the repository's **Settings → Actions → Runners**, the
   organisation's **Settings → Actions → Runners**, your own **Settings → Actions →
   Runners**, or **Administration → Runners**.
2. Create a registration token. It is shown once, is single-use and expires after an hour
   unless you choose otherwise, and can pin which labels the runner may declare.
3. On the runner machine:

   ```bash
   read -rs RBIT_RUNNER_TOKEN && export RBIT_RUNNER_TOKEN
   rbit-runner register --url https://git.example.com \
     --labels ubuntu-latest:container:docker.io/library/ubuntu:24.04
   rbit-runner run
   ```

   Each label is `name:backend[:image]`. A job's `runs-on` picks runners by label:

   - **container** runs the job in Docker or Podman, in the label's image (or the job's
     `container.image`), without privileges, the Docker socket or host networking, on a
     network of its own. This is the default and the right choice for anything you did not
     write yourself. Built-in steps (checkout, cache, artifacts) run on the host, and the
     job's container is paused while they work on its files, so the engine must support
     `pause`; rootless Podman needs cgroup v2 for that. In a container job a checkout into a
     directory that already has a `.git` starts a new repository there, because the job
     could have changed that `.git`.
   - **host** runs the job as the runner's own user, with **no isolation**: a job can read
     everything that user can, the runner's credential included. Use it only for
     repository runners of repositories you trust. The runners page marks such runners
     "host — no isolation". Jobs of untrusted runs (pull requests from forks and from
     people without write access) are never given to a runner through a host label, even
     once approved; they wait for a container label. A runner with both kinds of label
     shares its work directory between them: Node.js and fetched actions are checked at
     each use, but a host job can still write the `rbit/cache` entries a later container
     job restores.

`rbit-runner run --once` makes an ephemeral runner that takes one job and unregisters.
`rbit-runner install`, `uninstall`, `start`, `stop` and `status` run it in the background
with the system's service manager (see [Deployment](deployment.md#actions-and-runners)).
`rbit-runner check` reports whether Docker or Podman, Git and the shells are usable.
`rbit-runner exec .rbit/workflows/ci.yml` runs a workflow from a local checkout without a
server, using the same parser and executor.

## Where workflows live

rbit reads `.rbit/workflows/*.yml` (and `.yaml`). If that directory does not exist it reads
`.forgejo/workflows`, then `.github/workflows`: only the first that exists counts, so a
repository can keep GitHub's workflows and add rbit's own beside them.

## Triggers

| Event | Notes |
|---|---|
| `push` | `branches`, `branches-ignore`, `tags`, `tags-ignore`, `paths`, `paths-ignore` |
| `pull_request` | `types` (default `opened`, `reopened`, `synchronize`) and the same filters |
| `workflow_dispatch` | `inputs` of type `string`, `boolean`, `number`, `choice`, `environment`; started with **Run workflow** on the workflow's page or through the REST API |
| `schedule` | `cron`, in UTC |
| `workflow_run` | `workflows` (by `name:`), `types` (`requested`, `in_progress`, `completed`; all when absent), `branches`, `branches-ignore` (the triggering run's branch); the event carries `github.event.workflow_run` (`conclusion`, `head_sha`, `id`, …) |
| `repository_dispatch` | `types`; started with `POST /repos/{owner}/{repo}/dispatches` by a writer, `event_type` up to 100 characters and `client_payload` (at most 10 top-level properties, 65,535 characters) in `github.event` |

`workflow_run` and `repository_dispatch` run the default branch's workflows at its newest
commit, as on GitHub, with secrets and the repository's token permission, even when the
run that started a `workflow_run` had neither (a fork's pull request): treat that run's
artifacts and outputs as untrusted input. A run that three `workflow_run`s in a row led to
starts no more, so a workflow that waits for itself ends there.

A commit message or pull request title containing `[skip ci]` (or `[ci skip]`, `[no ci]`,
`[skip actions]`, `[actions skip]`) starts nothing. Pushes made by runs themselves, by
mirrors and by imports never start runs.

**Pull requests from forks and from people without write access** run with a read-only
token and without secrets or variables, and wait until a writer approves the run, as the
`approval_policy` under [Permissions](#permissions) decides (by default an author with a merged
pull request runs without approval). A writer can choose to always run a person's pull
requests without approval.

## Workflow syntax

rbit parses every key and refuses unknown ones with the line and column, because a typo
must not silently change what a job does.

- **Workflow:** `name`, `run-name`, `on`, `env`, `defaults.run`, `permissions`,
  `concurrency`, `jobs`.
- **Job that calls a workflow:** `name`, `uses`, `with`, `secrets` (or `secrets: inherit`),
  `strategy`, `needs`, `if`, `permissions` (see [Reusable workflows](#reusable-workflows)).
- **Job:** `name`, `needs`, `if`, `runs-on`, `permissions`, `env`, `defaults.run`,
  `outputs`, `timeout-minutes` (default 360, capped by `[actions] endless_timeout_seconds`, 3 hours
  unless set higher), `continue-on-error`, `concurrency`,
  `strategy` (`matrix` with `include`/`exclude`, `fail-fast`, `max-parallel`), `environment`
  (a name, or `{name, url}`), `container`
  (`image`, `env`, `credentials`, `options`, `volumes`), `services` (`image`, `env`,
  `credentials`, `options`, `volumes`; container runners only), `steps` (at most 256).
- **Step:** `id`, `name`, `if`, `run` or `uses`, `shell`, `working-directory`, `env`, `with`,
  `continue-on-error`, `timeout-minutes`.
- **Container options:** `options` passes what shapes a container (`--user`, `--hostname`,
  `--env NAME=value` (its value goes into the env file), the `--health-*` checks, `--memory`,
  `--cpus`, `--pids-limit` and other limits (the runner's own `[container]` limits come
  after and win), `--ulimit`, `--shm-size`, `--tmpfs`, `--add-host`, `--dns`,
  `--label` (except `rbit.*`),
  `--init`, `--read-only`, stop options, and `--entrypoint` for services). Anything that
  could give a job the runner host (`--privileged`, `--cap-add`, `--device`, `--network`,
  `-v`/`--mount`, `--security-opt`, `--pid`, …) is an error. Services with a health check are
  waited for, as on GitHub, and an unhealthy one fails the job. `volumes` takes named volumes
  (`data:/var/lib/data`, `:ro` for read-only), which the job's container and services share
  and which go with the job; a host path is an error.
- **Shells:** `bash` (`bash --noprofile --norc -eo pipefail`), `sh` (`sh -e`), `pwsh`,
  `powershell`, `cmd`, `python`. The default is `bash` (or `sh` without it), or `pwsh` (or
  `powershell` without it) on Windows hosts.
  A custom shell is a command with `{0}` for the script's path, as on GitHub
  (`shell: perl {0}`); the script is written as it is, to a file without an extension.

A job's `name`, `runs-on`, `strategy` (a matrix from `fromJSON(needs.setup.outputs.matrix)`),
`timeout-minutes`, `continue-on-error`, `concurrency`, `container`, `services` and
`environment` may read `needs`, as on GitHub: such a job is planned when the jobs it needs are
done, with their outputs, and a plan that cannot be made (a matrix that is not JSON) fails it.
A called workflow's jobs cannot do this yet.

A job without its own `if:` runs only when every job it needs succeeded (`success()`);
`if: always()`, `failure()` and `cancelled()` change that. The server decides a job's `if:`
when its needs are done, so a skipped job never takes a runner. With `fail-fast` (the
default) a failed matrix job cancels its unfinished siblings, and the jobs that need it see
it as failed. A job with `continue-on-error: true` that fails counts as succeeded: it fails
neither the run nor the jobs that need it, and cancels no siblings; its failed steps still
show.

### Reusable workflows

A workflow with `on: workflow_call` can be called by a job of another workflow, as on
GitHub:

```yaml
jobs:
  build:
    uses: ./.rbit/workflows/build.yml          # or owner/repo/path/to/build.yml@v1
    with: {target: release, version: '${{ needs.prepare.outputs.version }}'}
    secrets: inherit                           # or {deploy-key: '${{ secrets.DEPLOY_KEY }}'}
  publish:
    needs: build
    runs-on: linux
    steps:
      - run: echo "${{ needs.build.outputs.artifact }}"
```

- `./path` is read from the calling workflow's repository and commit; `owner/repo/path@ref`
  from a repository of this rbit instance, at a branch, tag or commit. A public
  repository's workflows can be called by anyone who may read it; a private one's only by
  the same owner's other private repositories, and only once **access** (`PUT
  …/actions/permissions/access` with `user` or `organization`) allows it. A workflow that
  cannot be read or used is a workflow error that never says which.
- The called workflow's jobs run as jobs of the caller's run, named `caller / job`, and
  wait for what the calling job waits for. Its `inputs` (`string`, `number`, `boolean`;
  required ones, defaults) and declared `secrets` are checked when the run starts; a
  `with:` that reads `needs` is evaluated when the job is handed out. Its `outputs` are
  what jobs that need the calling job read in `needs.<caller>.outputs`.
- The called workflow sees the caller's `github` context and `vars`, its own `inputs` and
  `env` (not the caller's). Secrets reach it only through `secrets:` (all of them with
  `inherit`). Its jobs may lower the token's permissions but never ask for more than the
  calling job (or the calling workflow) allows.
- Calls nest up to ten levels deep, the run's own workflow included, and a run may call
  at most 50 different workflows, GitHub's limits. A calling job's `strategy.matrix` calls
  the workflow once per combination, `call (a) / job`.

### Built-in actions

Checkout, cache and artifacts are compiled into rbit-runner. The common `actions/*` names are
aliases of them, in any later major version and when pinned to a commit
(`actions/checkout@<sha>`).

| `uses:` | Also accepted as | `with:` |
|---|---|---|
| `rbit/checkout@v1` | `actions/checkout@v2` and later | `repository` (`owner/name` on this instance; another private one needs a `token`), `ref`, `token` (instead of the run's token, also for later pushes), `path`, `fetch-depth` (1; 0 for all), `fetch-tags`, `submodules`, `lfs`, `clean`, `persist-credentials` |
| `rbit/cache@v1` | `actions/cache@v3` and later, `/restore`, `/save` | `key`, `path`, `restore-keys`, `fail-on-cache-miss`; outputs `cache-hit`, `cache-primary-key`, `cache-matched-key` |
| `rbit/upload-artifact@v1` | `actions/upload-artifact@v3` and later | `name`, `path`, `if-no-files-found`, `retention-days`, `overwrite`, `include-hidden-files`, `compression-level` |
| `rbit/download-artifact@v1` | `actions/download-artifact@v3` and later | `name`, `path`, `pattern`, `merge-multiple`, `run-id` (another run of this repository, as a `workflow_run` job fetches its trigger's; `github-token` is accepted and not needed), `repository` (this one only) |

An unknown `with:` key of a built-in is an error.

With `persist-credentials` (the default) the checkout keeps the task's token for later steps
as `actions/checkout` v6 does: in `git-credentials-<id>.config` in `RUNNER_TEMP`, which the
checkout's `.git/config` includes for that repository only (`includeIf.gitdir`), at the
paths the runner host, the job's container and a Docker action see. The workspace never
holds the token, so no artifact or cache of it carries it; the file goes with the task.

The `path` of the cache and of `upload-artifact` takes paths and glob patterns, one per line,
as GitHub's `@actions/glob` reads them: `*` and `?` within a name, `[a-z]` sets, `**` for any
number of directories, and `!` lines excluding; the last line that matches a file decides, and
a directory brings everything in it. `upload-artifact` leaves out names starting with a dot
(so `path: .` never takes `.git` along) unless `include-hidden-files: true`, and roots the
artifact where GitHub does: at the least common ancestor of the lines' literal starts
(`dist` for `dist/*.zip`), or at a single file's directory. Nothing is reached through a
symbolic link: a link is stored as a link.

### Other actions

Any other `uses:` is an action rbit-runner fetches and runs, as GitHub's runner does:

- `owner/repo@ref` or `owner/repo/path@ref` (`actions/setup-node@v4`,
  `github/codeql-action/init@v3`, or pinned: `actions/setup-python@<sha>`) is fetched with
  `git` from the runner's own rbit instance, so actions need nothing outside it:
  mirror the actions your workflows use there as public repositories (a pull mirror of
  `https://github.com/actions/setup-node` named `actions/setup-node`, kept current by the
  mirror's schedule). `[actions] url = "https://github.com"` in `runner.toml` fetches them
  from GitHub instead, as GitHub's own runner does. The reference is resolved to its
  commit, which the log names, and the action is kept in the runner's work directory at that
  commit. No token is sent with the fetch.
- `./path` is an action in the workspace, usually checked out by an earlier step.
- `docker://image` runs the image, with `with.args` and `with.entrypoint`.

JavaScript actions (`using: node12` … `node24`) run with a Node.js 24 the runner downloads on
first use and checks against a pinned SHA-256 (or the `node` that `[actions] node` names on
the host); in container jobs it is mounted at `/__e` and needs a glibc image. Composite
actions run their steps as part of the step, nested up to 9 deep. Docker actions are built
or pulled and run with Docker or Podman, with the workspace at `/github/workspace`. A `pre`
script runs right before its `main`; a `post` script runs after the job's last step as a
step of its own (`Post Run …`), when its `post-if` holds. Inputs are passed as `INPUT_*`
with the action's defaults, and `GITHUB_STATE` reaches the `post` script.

`actions/setup-*` and other tool installers keep what they download in the runner's
`RUNNER_TOOL_CACHE` (`<work_dir>/tool-cache` on the host, so a version is downloaded once;
the task's temporary directory in a container). Actions that use GitHub's own cache or
artifact services (`actions/setup-node`'s `cache:`) skip that part with a warning; use
`actions/cache` instead.

Actions that call GitHub's own API (`api.github.com`) with their default token, such as
release lookups in some installers, get the workflow's rbit token, which GitHub rejects
("Bad credentials"). Pass such an action a GitHub token from a secret, or `''` to call
unauthenticated, through its token input. Actions that talk to `GITHUB_API_URL`
(`actions/github-script`) reach rbit and work with the default token.

On a Windows host, actions assume what GitHub's Windows machines have: Git for Windows,
PowerShell 7 installed from its MSI (the Microsoft Store build is not usable from
Node.js), and administrator rights for installers that write to `Program Files` or the
registry (`actions/setup-python`; `actions/setup-dotnet` unless `DOTNET_INSTALL_DIR` is
set). Linux runners have none of these limits. As on GitHub's machines, `bash` in a step is
Git's, not WSL's: its directory goes first on the job's `PATH` when WSL's would win.

A runner's administrator decides which actions it runs with `[actions] allow`: `owner/repo`
patterns such as `actions/*` or `my-org/*`, and `docker://` image patterns. The default,
`["*"]`, allows every action; `[]` allows only the built-ins and the workspace's own.

The server keeps the cache, as GitHub does, so any runner of the repository restores what
another saved. A run reads its own branch's entries, then the default branch's, and writes
only its own; pull requests from forks write only to their pull request's scope. The server
decides these scopes from the run, never from the runner. An entry matches its key, or the
newest entry starting with a `restore-keys` prefix, and only with the same `path` list on
the same kind of job (on a Windows host, another host, or in a container). Entries never change:
a save of a key the scope has is skipped. A restore gives files the modification times they
were saved with, as GitHub's does, so a restored `target/` is not newer than the checkout. Each repository keeps up to
`[actions] max_cache_storage_bytes` (10 GiB), and saving beyond it removes the least
recently used entries; an entry nobody restores for `cache_retention_days` (7) goes. A
runner whose server is older keeps the cache itself, in `<work_dir>/cache`, bounded by its
own `[cache] max_bytes`, which also caps the size of one entry either way. Artifacts are stored by the server, listed on the run page
and downloadable by anyone who may read the repository, for 90 days unless the workflow asks
for fewer. They download as a zip, as on GitHub (the run page's link, and the REST API's
`archive_download_url`, `…/artifacts/{id}/zip`), or as a tar.gz (`…/tar.gz`), with the
uploaded files at the top of either.

### Expressions

`${{ }}` supports literals, the contexts below, `!`, comparisons with GitHub's loose
equality (strings compare case-insensitively), `&&`, `||`, and the functions `contains`,
`startsWith`, `endsWith`, `format`, `join`, `toJSON`, `fromJSON`, `success`, `failure`,
`always`, `cancelled` and, in steps, `hashFiles`.

| Context | Where |
|---|---|
| `github`, `inputs`, `vars`, `matrix`, `strategy` | everywhere |
| `needs` | job `if`, `name`, `runs-on`, `strategy`, `timeout-minutes`, `continue-on-error`, `concurrency`, `container`, `services`, `environment`, a calling job's `with:`, and in steps |
| `env`, `secrets`, `steps`, `job`, `runner` | steps, `container`/`services` env and credentials, job `outputs` |

The server never evaluates `secrets`. A secret reaches a step only where the step maps it,
for example `env: TOKEN: ${{ secrets.TOKEN }}`; the task's token is `secrets.GITHUB_TOKEN`
(or `secrets.RBIT_TOKEN`). Secret values, the token and anything a step marks with
`::add-mask::` are replaced by `***` in logs, as are their base64 forms.

### Files and commands steps can use

`GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH` and `GITHUB_STEP_SUMMARY` (with `RBIT_*` aliases),
in both the `name=value` and heredoc forms. What steps write to `GITHUB_STEP_SUMMARY` is shown
as Markdown below the jobs on the run page: up to 64 KiB per step and 256 KiB per job, with
secrets masked; a longer summary is cut with a note. Actions' `post` scripts read what
their step wrote to `GITHUB_STATE`. Steps can also use `::add-mask::`,
`::group::`/`::endgroup::`, `::error::`, `::warning::`, `::notice::`, `::debug::` (shown
only when the `ACTIONS_STEP_DEBUG` secret or variable is `true`, which also sets
`RUNNER_DEBUG=1`) and `::stop-commands::`; the deprecated `::set-output` and `::save-state`
work with a warning, while `::set-env` and `::add-path` are refused, as on GitHub.
`::add-matcher::<file>` adds GitHub's problem matchers (one-line and looping patterns): for
the rest of the job, a line a matcher matches is followed by its `##[error]`, `##[warning]` or
`##[notice]` annotation with the file, line and column, until `::remove-matcher owner=…::`.
In a container job a matcher file is read when its step ends, with the container paused, so
the matcher applies from the next step on.
The environment has
`CI=true`, `GITHUB_ACTIONS=true`, `RBIT_ACTIONS=true` and the usual `GITHUB_*` and `RUNNER_*`
variables.

## Secrets and variables

Repository administrators manage a repository's secrets and variables in **Settings →
Actions**; organisation owners and users manage their own for all their repositories or
selected ones. A repository's own secret or variable wins over an owner's of the same name.
Secret values are sealed with AES-256-GCM under a key file that is not part of the database
or its backups, and are never shown again after you save them.

## Environments

A job with `environment: production` (or `environment: {name: production, url: ...}`) deploys
to that environment, which is created the first time a job names it. Repository
administrators manage environments in **Settings → Actions → Environments**:

- **Required reviewers**: up to six people who can read the repository. A job in the
  environment waits, before any runner takes it, until one of them approves it on the run's
  page (**Review deployments**) or through the API; rejecting it fails the job. Re-running a
  job needs a new approval. **Prevent self-review** stops whoever started the run from
  reviewing it.
- **Wait timer**: minutes (up to 43200, 30 days) a job in the environment waits once it is
  ready (after review, if the environment has reviewers), before a runner may take it (`wait_timer` in
  the API).
- **Deployment branches and tags**: any ref, protected branches only, or refs matching
  patterns: a plain pattern names branches (`release/*`), a tag pattern starts with
  `refs/tags/` (`refs/tags/v*`), and `!` excludes, the last matching pattern deciding. A job
  from another ref fails when a runner takes it.
- **Secrets and variables** of its own, which win over the repository's and the owner's of
  the same name and reach only jobs in that environment.

`environment.url` is evaluated after the job's steps (it may use `steps.*.outputs`) and shown
on the run's page. Deleting an environment, or removing its last reviewer, lets the jobs
waiting for it run.

## Status badges

Each workflow has a badge for a README:

```markdown
![CI](https://git.example.com/owner/repo/actions/workflows/ci.yml/badge.svg)
```

It shows "passing" or "failing" for the newest run of the workflow that succeeded or failed
on the default branch, or "no status" before one has; cancelled and skipped runs are passed
over. `?branch=release` picks another branch and `?event=push` counts only runs that event
started. A public repository's badge can be cached by anyone for a minute; a private
repository's is shown only to people who may read the repository (in an image on rbit's own
pages, their browser sends their session) and answers everyone else 404, exactly like a
repository that does not exist.

## Running a workflow by hand

A workflow with `on: workflow_dispatch` has **Run workflow** on its page for anyone who can
write to the repository. Choose the branch or tag to run it from (the form then shows the
workflow's inputs at that ref), fill in the inputs and run it; the new run opens. Workflows on
the default branch are listed on the Actions tab even before they ever ran.

**Disable workflow** on the same page (or the REST API) stops a workflow without deleting
its file: a disabled workflow starts no run for any event, on any branch, and cannot be run
by hand until **Enable workflow**. Its page says so to everyone.

## The workflow's token

`secrets.GITHUB_TOKEN` (or `RBIT_TOKEN`) works with `git` and with a part of the REST API at
`GITHUB_API_URL`: reading the repository, its refs, commits and releases, and listing and
downloading workflow runs' artifacts (a `workflow_run` job fetching what the run that
started it uploaded, `github.event.workflow_run.id`); with
`contents: write`, posting commit statuses and creating, editing and deleting releases and
their files (a release may create its tag from a branch); and commenting on issues (with
`issues: write`) and pull requests (with `issues: write` or `pull-requests: write`).
Statuses, releases, files and comments it writes show as `rbit-actions[bot]`, and a tag it
creates starts no runs, as on GitHub. With `issues` or `pull-requests: read` it reads issues
and their comments; with `write` it also edits and deletes the comments rbit Actions wrote
(a person's comment stays theirs). It cannot open issues yet.
Untrusted runs (forks, outside authors) get a read-only token.

## Permissions

**Settings → Actions** (or the REST API below) decides, for a repository, whether workflows
run, which actions they may use, the workflow token's default permission and which pull
requests wait for approval. An organisation, or a user for their own repositories, sets the
same policies as ceilings their repositories can only narrow (for the workflow token, a
ceiling on the default; a workflow's own `permissions:` replace it, as on GitHub), and
chooses which repositories
may run workflows at all (all, none or selected ones).

- **Allowed actions**: `all`; `local_only`, the repository's own actions (`./path`) only; or
  `selected`, GitHub's own actions (`actions/*`, `github/*`) if allowed and those matching a
  pattern: `octo-org/*`, `octo-org/deploy@v2` (without `@`, any ref) or `docker://alpine:*`.
  rbit's built-in checkout, cache and artifact actions always run. The owner's list and the
  repository's both apply, and the runner checks every action against them before fetching
  it, as well as against its own `[actions] allow`.
- **Workflow token**: `read` (the default) or `write`: the token's permissions when a
  workflow declares no `permissions:`. As on GitHub, a declaration replaces it, above or
  below (a release workflow asks for `contents: write`); an owner set to `read` makes
  `read` every repository's default. `can_approve_pull_request_reviews` is kept and read back for
  GitHub's tools, but has nothing to allow: the token cannot review pull requests.
- **Pull requests from outside contributors** (`approval_policy`):
  `first_time_contributors` (the default) runs an author's pull requests without approval
  once a writer trusted them or one of their pull requests was merged;
  `all_external_contributors` waits for everyone the repository has not trusted
  explicitly. rbit cannot tell who is new to the platform, so
  `first_time_contributors_new_to_github` waits like `first_time_contributors`.

## The REST API and webhooks

The `rbit` binary is also a command-line client for this API, like GitHub's `gh`: `rbit run
list`, `rbit run watch --exit-status`, `rbit run view --log-failed`, `rbit workflow run`,
`rbit artifact download`, `rbit pr checks` and `rbit api` (see [the client's guide](cli.md)).

`/api/v1/repos/{owner}/{repo}/actions/...` lists runs and jobs, cancels, re-runs and approves
runs, dispatches workflows, shows, disables and enables workflows and lists their runs
(`…/actions/workflows/{file}`, `…/disable`, `…/enable`, `…/runs`), serves job logs (one job's
as text, a run's as a zip: `…/runs/{run_id}/logs`) and artifacts, lists and deletes cache entries
(`…/actions/caches`, `…/actions/cache/usage`, as `gh cache` uses them), manages secrets,
variables and runners (with registration tokens), and reviews pending deployments
(`…/runs/{run_id}/pending_deployments`), in GitHub's shapes (see the OpenAPI document at
`/api/v1/openapi.json`); organisations (`/orgs/{org}/actions/...`) and users
(`/user/actions/...`) have their own secrets, variables and runners.
`/api/v1/repos/{owner}/{repo}/environments[/{name}]` lists, creates, changes and deletes
environments. `…/actions/permissions`, `…/permissions/selected-actions`, `…/permissions/workflow`
and `…/permissions/fork-pr-contributor-approval` read and change the permissions above for a
repository (`/repos/{owner}/{repo}/actions/…`), an organisation (`/orgs/{org}/actions/…`, with
`…/permissions/repositories` for its selected repositories) and the caller (`/user/actions/…`,
rbit's own), with the `admin` scope; a value past the owner's ceiling is `409`. Webhooks can subscribe
to `workflow_run` and `workflow_job`.

## Differences from GitHub Actions

- A called workflow is read from this rbit instance, never from github.com. A calling job
  takes no `concurrency` yet (set it on the called workflow's jobs), and a called job's
  fields (`runs-on`, the matrix, containers, …) can use only the inputs known when the run
  starts, not `needs` nor inputs a `with:` reads from `needs`; nor can a calling job's own
  `strategy` read `needs`. A
  `workflow_call` input's `default` is taken as written, not evaluated.
- The `workflow_run` payload has the run's `head_branch`, `head_sha`, `conclusion` and
  repository, but no `head_repository` or `head_commit`.
- Events other than those under [Triggers](#triggers) (`release`, `issues`,
  `pull_request_target`, …) are accepted in `on:` and never start a run.
- `permissions:` scopes other than `contents`, `issues` and `pull-requests` are accepted and
  have no effect: there is no OIDC token (`id-token: write`), for instance.
- There is no Checks API (`check-runs`, `check-suites`): actions that report through it, such
  as `dorny/test-reporter`, fail. Jobs report as commit statuses instead.
- A runner has only the labels it declares; GitHub's automatic `self-hosted`, OS and
  architecture labels are not added. Declare them (`--labels self-hosted:host,linux:host`) to
  match `runs-on: [self-hosted, linux]`.
- A runner talks plain HTTP only to a server on its own machine. A runner elsewhere needs the
  server on HTTPS, or an encrypted tunnel (SSH, WireGuard) to the server's port on its own
  loopback.
- Runner groups (`runs-on: {group: …}`), `cancel-timeout-minutes` and `snapshot` are
  refused with a readable error.
- A job runs at most `[actions] endless_timeout_seconds` (3 hours by default), where GitHub
  allows 6 hours; raise it for longer jobs.
- Artifact names may use letters, digits, `.`, `_` and `-` only; GitHub also allows spaces
  and most punctuation.
- Actions are fetched when their step comes, not all at the start of the job, so an
  action's `pre` script runs right before its `main`. GitHub's cache and artifact services that
  actions reach through `ACTIONS_RUNTIME_*` do not exist; the built-ins stand in for
  `actions/cache` and `actions/*-artifact`.
- Environments have no team reviewers or custom protection rules, and runs create no
  deployment objects: there is no Deployments API yet.
- No container `ports` or host-path `volumes`, and
  `options` take only what cannot give a job the runner host (see [Workflow
  syntax](#workflow-syntax)). Each is a readable error.
- Secrets are set through the API as plain values over TLS, not with GitHub's sealed box.
- A workflow is identified by its file name in the API.
- A job's log and an artifact are downloaded directly from the API, not through a redirect.
- The workflow's token edits and deletes only rbit Actions' own comments, where GitHub's may
  change anyone's, and cannot open issues.

### Known limits of the container backend

- A host label's job runs as the runner's own user, so it can read and change whatever
  that user can: the runner's configuration and token, its work directory, and the files of
  other jobs on the same runner, container jobs included. Separate directories for host and
  container labels would not change that, as one user owns both. What the runner keeps
  between jobs (Node.js for JavaScript actions, fetched actions) is checked at each use
  against a SHA-256 the running process recorded, and downloaded again when it was changed,
  so a job's changes do not reach later jobs through those caches. To run untrusted code in
  containers next to host labels, give the host labels a runner of their own, under another
  OS user or on another machine.
- Containers reach the network like any Docker container, as on GitHub's runners: the
  internet, the host's addresses and cloud metadata endpoints (`169.254.169.254`). rbit
  does not filter it, since what a runner may reach depends on where it runs; on a cloud
  host, block the metadata endpoint for containers, for example
  `iptables -I DOCKER-USER -d 169.254.169.254 -j REJECT`, or give `[container] network` a
  network of your own.
- Jobs keep Docker's default capabilities, as GitHub's runner leaves them (so `ping`,
  which needs `NET_RAW`, works); only `MKNOD` is dropped, and `no-new-privileges` is set.
  They have no memory or CPU limit unless `[container] memory` and `cpus` are set, as a
  hosted runner's job has its whole machine; set them on a runner that runs several jobs
  at once.
- A rootful engine's containers write the task's files as root. Before each built-in step
  works on them, after each Docker action and when the job ends, a helper container gives
  them back to the runner's user: `chown`, set-user-ID and set-group-ID bits cleared, and
  every directory opened to its owner. The helper is never the job's image or container
  (which could have replaced `chown`): it runs `[container] helper_image`, a `busybox`
  pinned by digest, with no network, only the `CHOWN`, `FOWNER` and `DAC_OVERRIDE`
  capabilities and nothing mounted but the task's directories. Under rootless Docker or
  Podman it hands them to the container's root, which is the runner's user. An engine
  with `userns-remap` is not supported. The runner keeps `<work_dir>/tasks` private to its
  user.
- A Docker action's image is built from the action's own directory only, with
  `[container] memory`, `cpus` and `pids_limit` where the builder takes them (Podman,
  Docker's classic builder). BuildKit takes no limit for one build: there it runs under
  the engine's own, within the step's timeout, and the log says so. Builds use the engine's
  default network, never the host's, as GitHub's runner builds them, and Buildx is told to
  run no `git` in the build's context (`BUILDX_GIT_INFO=0`), which can be the job's
  workspace.
- The default `[actions] allow = ["*"]` lets any workflow use any action or image; narrow
  it for runners that take pull requests from forks.
- A container's environment goes through an env file, which cannot hold a multi-line value;
  such a value travels in the environment of the engine's CLI on the host instead. So a
  multi-line value cannot go to a variable that CLI, its loader or a program it starts reads
  (`LD_*`, `DOCKER_*`, `BUILDKIT_*`, `BUILDX_*`, `OTEL_*`, `*_PROXY`, `PATH`, `GIT_CONFIG*`,
  …): the step fails and names it. Single-line values of any name, and multi-line values of
  any other name (a key in `SSH_KEY`, `GOOGLE_CREDENTIALS`), reach the container.
- Masking covers what GitHub's runner masks: each secret, each line of a multi-line one,
  its base64 in every alignment, and its URL-, JSON-, XML-, expression- and command-line-
  escaped forms. It works on each printed line, so a secret split across two lines, or
  across the 1 MiB cut of one very long line, is not masked.
- Values shorter than four characters are not masked on their own lines or in their
  encodings, so that masking a short secret such as `1` cannot hide every number in a log;
  the whole value is still masked.
