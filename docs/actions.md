# rbit Actions

rbit Actions runs workflows from your repository on **rbit-runner**, a separate program you run
on machines you choose. The rbit server plans the jobs, hands them out, keeps their logs and
artifacts, and shows the results. Builds never run inside the server.

Workflows use a subset of GitHub Actions' syntax, so most simple workflows run unchanged. This
guide covers turning Actions on, registering a runner, the workflow syntax rbit supports and
how it differs from GitHub. Operators should also read
[Runners in deployment.md](deployment.md#actions-and-runners).

## Turning it on

1. An administrator sets `[actions] enabled = true` in the configuration and restarts rbit.
   While it is off, Actions does not exist: there are no pages, no runner endpoint and no
   background work, and workflow files are ignored.
2. In a repository, open **Settings → Actions** and turn on **Run workflows in this
   repository**. `[actions] enable_for_new_repositories` turns it on for new repositories.
3. Register a runner (see below), then push a workflow under `.rbit/workflows/`.

The **Actions** tab shows the repository's runs. A run page lists its jobs in `needs` order
with Cancel, Re-run failed, Re-run all and per-job re-run buttons for writers. A job page
shows each step's log as it runs and links to the raw log. Each job also reports a commit
status, so required checks and the pull request merge box see Actions like any other CI.

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
     "host — no isolation".

`rbit-runner run --once` makes an ephemeral runner that takes one job and unregisters.
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
| `workflow_dispatch` | `inputs` of type `string`, `boolean`, `number`, `choice`; started with **Run workflow** on the workflow's page or through the REST API |
| `schedule` | `cron`, in UTC |

A commit message or pull request title containing `[skip ci]` (or `[ci skip]`, `[no ci]`,
`[skip actions]`, `[actions skip]`) starts nothing. Pushes made by runs themselves, by
mirrors and by imports never start runs.

**Pull requests from forks and from people without write access** run with a read-only
token and without secrets or variables, and wait until a writer approves the run. A writer
can choose to always run a person's pull requests without approval.

## Workflow syntax

rbit parses every key and refuses unknown ones with the line and column, because a typo
must not silently change what a job does.

- **Workflow:** `name`, `run-name`, `on`, `env`, `defaults.run`, `permissions`,
  `concurrency`, `jobs`.
- **Job:** `name`, `needs`, `if`, `runs-on`, `permissions`, `env`, `defaults.run`,
  `outputs`, `timeout-minutes` (default 360), `continue-on-error`, `concurrency`,
  `strategy` (`matrix` with `include`/`exclude`, `fail-fast`, `max-parallel`), `container`
  (`image`, `env`, `credentials`), `services` (`image`, `env`, `credentials`; container
  runners only), `steps` (at most 256).
- **Step:** `id`, `name`, `if`, `run` or `uses`, `shell`, `working-directory`, `env`, `with`,
  `continue-on-error`, `timeout-minutes`.
- **Shells:** `bash` (`bash --noprofile --norc -eo pipefail`), `sh` (`sh -e`), `pwsh`,
  `powershell`, `cmd`, `python`. The default is `bash` (or `sh` without it), or `pwsh` (or
  `powershell` without it) on Windows hosts.

A job without its own `if:` runs only when every job it needs succeeded (`success()`);
`if: always()`, `failure()` and `cancelled()` change that. The server decides a job's `if:`
when its needs are done, so a skipped job never takes a runner. With `fail-fast` (the
default) a failed matrix job cancels its unfinished siblings, and the jobs that need it see
it as failed. A job with `continue-on-error: true` that fails counts as succeeded: it fails
neither the run nor the jobs that need it, and cancels no siblings; its failed steps still
show.

### Built-in actions

Checkout, cache and artifacts are compiled into rbit-runner. The common `actions/*` names are
aliases of them, in any later major version and when pinned to a commit
(`actions/checkout@<sha>`).

| `uses:` | Also accepted as | `with:` |
|---|---|---|
| `rbit/checkout@v1` | `actions/checkout@v2` and later | `ref`, `path`, `fetch-depth` (1; 0 for all), `fetch-tags`, `submodules`, `lfs`, `clean`, `persist-credentials` |
| `rbit/cache@v1` | `actions/cache@v3` and later, `/restore`, `/save` | `key`, `path`, `restore-keys`, `fail-on-cache-miss`; output `cache-hit` |
| `rbit/upload-artifact@v1` | `actions/upload-artifact@v3` and later | `name`, `path`, `if-no-files-found`, `retention-days`, `overwrite` |
| `rbit/download-artifact@v1` | `actions/download-artifact@v3` and later | `name`, `path`, `pattern`, `merge-multiple` |

An unknown `with:` key of a built-in is an error.

### Other actions

Any other `uses:` is an action rbit-runner fetches and runs, as GitHub's runner does:

- `owner/repo@ref` or `owner/repo/path@ref` (`actions/setup-node@v4`,
  `github/codeql-action/init@v3`, or pinned: `actions/setup-python@<sha>`) is fetched with
  `git` from the runner's `[actions] url`, `https://github.com` unless you point it at a
  mirror. The reference is resolved to its commit, which the log names, and the action is
  kept in the runner's work directory at that commit. No token is sent with the fetch.
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
set). Linux runners have none of these limits.

A runner's administrator decides which actions it runs with `[actions] allow`: `owner/repo`
patterns such as `actions/*` or `my-org/*`, and `docker://` image patterns. The default,
`["*"]`, allows every action; `[]` allows only the built-ins and the workspace's own.

The cache lives on the runner, so a hit needs the same runner. A run reads its own branch's
entries, then the default branch's, and writes only its own; pull requests from forks write
only to their pull request's scope. Artifacts are stored by the server, listed on the run page
and downloadable by anyone who may read the repository, for 90 days unless the workflow asks
for fewer.

### Expressions

`${{ }}` supports literals, the contexts below, `!`, comparisons with GitHub's loose
equality (strings compare case-insensitively), `&&`, `||`, and the functions `contains`,
`startsWith`, `endsWith`, `format`, `join`, `toJSON`, `fromJSON`, `success`, `failure`,
`always`, `cancelled` and, in steps, `hashFiles`.

| Context | Where |
|---|---|
| `github`, `inputs`, `vars`, `matrix`, `strategy` | everywhere |
| `needs` | job `if`, and in steps |
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
The environment has
`CI=true`, `GITHUB_ACTIONS=true`, `RBIT_ACTIONS=true` and the usual `GITHUB_*` and `RUNNER_*`
variables.

## Secrets and variables

Repository administrators manage a repository's secrets and variables in **Settings →
Actions**; organisation owners and users manage their own for all their repositories or
selected ones. A repository's own secret or variable wins over an owner's of the same name.
Secret values are sealed with AES-256-GCM under a key file that is not part of the database
or its backups, and are never shown again after you save them.

## Running a workflow by hand

A workflow with `on: workflow_dispatch` has **Run workflow** on its page for anyone who can
write to the repository. Choose the branch or tag to run it from (the form then shows the
workflow's inputs at that ref), fill in the inputs and run it; the new run opens. Workflows on
the default branch are listed on the Actions tab even before they ever ran.

## The workflow's token

`secrets.GITHUB_TOKEN` (or `RBIT_TOKEN`) works with `git` and with a part of the REST API at
`GITHUB_API_URL`: reading the repository, its refs, commits and releases; with
`contents: write`, posting commit statuses and creating, editing and deleting releases and
their files (a release may create its tag from a branch); and commenting on issues (with
`issues: write`) and pull requests (with `issues: write` or `pull-requests: write`).
Statuses, releases, files and comments it writes show as `rbit-actions[bot]`, and a tag it
creates starts no runs, as on GitHub. It cannot list or edit comments yet.
Untrusted runs (forks, outside authors) get a read-only token.

## The REST API and webhooks

`/api/v1/repos/{owner}/{repo}/actions/...` lists runs and jobs, cancels, re-runs and approves
runs, dispatches workflows, serves job logs and artifacts, and manages secrets, variables and
runners (with registration tokens), in GitHub's shapes (see the OpenAPI document at
`/api/v1/openapi.json`); organisations (`/orgs/{org}/actions/...`) and users
(`/user/actions/...`) have their own secrets, variables and runners. Webhooks can subscribe
to `workflow_run` and `workflow_job`.

## Differences from GitHub Actions

- No reusable workflows (`workflow_call`, `jobs.<id>.uses`).
- Actions are fetched when their step comes, not all at the start of the job, so an
  action's `pre` script runs right before its `main`. Problem matchers
  (`::add-matcher::`) are accepted and ignored. GitHub's cache and artifact services that
  actions reach through `ACTIONS_RUNTIME_*` do not exist; the built-ins stand in for
  `actions/cache` and `actions/*-artifact`.
- No `environment`, `secrets: inherit`, `container.options`, service `options`/`volumes`, or
  custom shells (`shell: my {0}`): each is a readable error.
- `permissions:` can only lower the repository's default token permission (**Settings →
  Actions**), never raise it: a workflow that publishes releases needs that default set to
  write.
- `runs-on` and matrices cannot depend on another job's outputs.
- `hashFiles` works in steps only.
- Secrets are set through the API as plain values over TLS, not with GitHub's sealed box.
- A workflow is identified by its file name in the API.
- A job's log and an artifact are downloaded directly from the API, not through a redirect,
  and artifacts are tar.gz archives (`…/artifacts/{id}/tar.gz`), not zip.
- The workflow's token cannot list and edit comments yet.
