# Actions — CI/CD on external runners

The working contract for **rbit Actions**: GitHub/Forgejo Actions–style workflows
(`.forgejo/workflows/*.yml`, `.github/workflows/*.yml`) executed by **external, unmodified
runners** — [Forgejo Runner](https://code.forgejo.org/forgejo/runner) and Gitea
[`act_runner`](https://gitea.com/gitea/act_runner). House rules from
[milestone-3.md](milestone-3.md#rules-for-shared-files-no-pre-wired-stubs-this-milestone),
[../HANDOFF.md](../HANDOFF.md) and [../../CONTRIBUTING.md](../../CONTRIBUTING.md) apply
unchanged unless this document says otherwise.

rbit is the **control plane only**: it reads workflow files, decides which runs a push or
pull request starts, expands matrices, schedules jobs onto runners, hands out short-lived
credentials, stores logs, reports commit statuses and shows the results. It never executes a
build. [ROADMAP.md](../../ROADMAP.md)'s "a built-in Actions runner" stays under *Not
planned*; the "runner protocol" idea under *Longer-term ideas* is what this delivers.

Research behind this contract (protocol field numbers, runner behaviour, CVEs in other
forges): the protocol report produced for this work, summarised where it matters below.
Source snapshots: forgejo/runner `c4a92ff`, forgejo/actions-proto `v0.8.0`, gitea
actions-proto-def `76f6081`, as of 2026-10-23.

---

## 1. Decisions

| Question | Decision |
|---|---|
| Who runs builds? | External Forgejo Runner (first target) and Gitea act_runner, speaking the Connect protocol with binary protobuf at `POST /api/actions/{service}/{method}`. One union `.proto` serves both forks (field numbers do not conflict). |
| On by default? | **No.** `[actions] enabled = false`. Instance switch, read once at startup; a change needs a restart. Then **per repository**: repository administrators turn Actions on in the repository's settings (default off; `[actions] enable_for_new_repositories` changes the default for *new* repositories only). |
| What does "disabled" mean? | `Services::actions` is `None` — the service is never constructed. Its routers are not merged, no subscriber, sweeper or scheduler exists, no UI entry renders, no webhook event is offered, workflow files are ignored, task tokens are refused without a database lookup, and **nothing is written** to any `actions_*` table or under `<data_dir>/actions`. The schema still exists (migrations always run). §8 is the proof. |
| Protocol code | `prost` 0.14 types generated once by a pure-Rust tool (`protox` + `prost-build`, no `protoc`) and **checked in**. Normal builds have no `build.rs`. A test regenerates into a temp dir and fails on drift. |
| Connect server | Hand-written in Axum (~200 lines): `application/proto` only (415 otherwise), identity encoding only, a body cap per method, Connect JSON errors, `unimplemented` (501) for RPCs we don't serve. The official `connectrpc` crate is pre-1.0 and brings a second protobuf runtime. |
| Workflow parsing | A new pure crate **`rbit-workflow`** (no I/O, no async, no database): YAML → typed model, trigger matching, GitHub's filter-pattern dialect (ported from act's `workflowpattern`, not `globset` — negation order differs), cron, matrix expansion, single-job payload emission. YAML via **`saphyr`** (node tree with spans) + **`serde-saphyr`** (typed, panic-free, budgets) — both MIT OR Apache-2.0, actively released in 2026. Not `yaml-rust` (unmaintained; only syntect pulls it) and not `serde_yaml`/`serde_yml` (deprecated). |
| Cron | Own 5-field POSIX parser in `rbit-workflow` (UTC, `*`, lists, ranges, steps, names; plus `@hourly`/`@daily`/`@weekly`/`@monthly`/`@yearly`). ~250 lines, tested exhaustively — the TOTP precedent: small, specified, no dependency. |
| Job-level `if:` | Always dispatched; the runner evaluates it and reports `RESULT_SKIPPED`. Correct, costs a runner slot per skipped job. Server-side evaluation is a later optimisation. |
| Expressions on the server | Only `${{ matrix.* }}` (and literals) inside `runs-on`, `name` and `concurrency.group`'s `github.*` subset (§4.6). Everything else is passed to the runner verbatim. |
| Task credential | `rbat_` + 43 base64url characters (256 bits), only the SHA-256 digest stored (unique index → O(1) lookup), valid **only while its task is running**; the digest column is cleared when the task stops. Accepted by Git smart HTTP (Basic password, any username) and by the REST API (`Bearer`/`token`). |
| Commit statuses | Posted by a **system actor** (creator `NULL`, shown as "rbit Actions", webhook `sender` `rbit-actions[bot]`). Not the pusher: fork pull request authors have no write access, scheduled runs have no user, and a status must not appear to be something a person typed. `pending` as soon as a job is queued. |
| Secrets at rest | AES-256-GCM (`aes-gcm` 0.11, RustCrypto, already in `Cargo.lock`), key in a **separate key file** (`<data_dir>/actions/secrets.key`, `0600`), *not* in SQLite — a leaked `rbit.db` or backup archive reveals no secret. §6.3. |
| Logs | Plain append-only files under `<data_dir>/actions/logs/` while a task runs, gzip-compressed on finalise; SQLite stores only counts and offsets. **Not backed up** (like `tmp/`): backups keep format 3. Retention 90 days. §5.5. |
| Live view | htmx polling of a fragment with a byte-offset cursor every 2 s while the job runs; without JavaScript the page shows the current log and a "Refresh" link; full raw log download always available. No SSE/WebSocket. |
| Fork pull requests | Read-only token, **no secrets and no variables**, and runs wait for **approval** unless the author is trusted (§6.2). No `pull_request_target`. |
| Pushes made by a task | Do **not** start runs (GitHub's `GITHUB_TOKEN` rule: no recursive builds). `workflow_dispatch` through the API with a task token is refused for the same reason. |
| Mirrors | Pull mirror fetches and imports never start runs. |

## 2. Scope

**In scope (this branch):** triggers `push` (`branches`, `branches-ignore`, `tags`,
`tags-ignore`, `paths`, `paths-ignore`), `pull_request` (`types`, `branches`,
`branches-ignore`, `paths`, `paths-ignore`), `workflow_dispatch` (with `inputs`: `string`,
`boolean`, `choice`, `number`), `schedule` (`cron`); skip markers (`[skip ci]`, `[ci skip]`,
`[no ci]`, `[skip actions]`, `[actions skip]`); matrix (`include`/`exclude`, cartesian,
≤ 256 combinations), `needs` DAG with outputs and results, `runs-on` label matching,
workflow-level `concurrency` (`group`, `cancel-in-progress`), cancel, re-run (all jobs,
failed jobs, one job), approval of fork runs, zombie/endless/abandoned sweeps; runners at
instance, owner (user or organisation) and repository scope with hashed registration tokens;
encrypted secrets and plain variables at owner and repository scope; per-step logs with live
tail; commit statuses; `workflow_run` / `workflow_job` webhooks; a GitHub-shaped REST
subset; metrics; audit.

**Out of scope (deliberately, each may come later):**

- **Artifacts server** (`/api/actions_pipeline/`, twirp v4). `upload-artifact` steps fail
  with the runner's own error; the docs say so. It needs a runtime JWT, a storage tier and
  its own retention — a feature of its own, and the source of two recent Gitea CVEs.
- **Caches**: none needed on the server — Forgejo Runner embeds its cache server.
- **OIDC ID tokens** (`id-token: write`).
- `pull_request_target`, `workflow_call` (reusable workflows), `workflow_run` as a
  *trigger*, `issues`/`issue_comment`/`release`/`create`/`delete` triggers.
- Job-level `concurrency`, `fail-fast`, `max-parallel` (Forgejo has none of them either),
  `environment` and deployment protection, `permissions:` narrowing (Forgejo ignores it;
  our token is already least-privilege per §6.1).
- Server-side `if:` evaluation, `needs.*` in `runs-on`/matrix ("incomplete" jobs).
- Forgejo `FetchSingleTask`, batch fetch (`task_capacity`), `UpdateStepSummary` — answered
  `unimplemented`; Forgejo Runner degrades gracefully on each.
- Offline runner registration (`create-runner-file`), ephemeral-runner autoscaling helpers.
- Rendering ANSI colours (stripped — monochrome UI), annotations, problem matchers.
- `.gitea/workflows` and `.rbit/workflows`: two locations are enough; `.forgejo` first.

## 3. Crates and layering

```
rbit-server ── rbit-web ──┬── rbit-services ──┬── rbit-storage ── rbit-core
                          │                   ├── rbit-git
                          │                   └── rbit-workflow ── (saphyr, serde-saphyr)
                          └── (prost, prost-types: wire types only)
tools/protogen  (workspace member, not a default member, publish = false; protox + prost-build)
```

| Crate / module | Owns |
|---|---|
| `rbit-core::config::actions` | `ActionsConfig` (§7). Pure. |
| `rbit-core::actions` | Value types shared by storage, services and web: `RunId`, `JobId`, `TaskId`, `RunnerId`, `RunStatus`, `JobStatus` (Forgejo's numeric order: `Success=1, Failure=2, Cancelled=3, Skipped=4, Waiting=5, Running=6, Blocked=7`, plus rbit's `AwaitingApproval=8`), `RunnerScope` (`Instance`, `Owner(AccountId)`, `Repository(RepoId)`), `RunnerLabel` (1–64 chars, `[A-Za-z0-9._-]`), `SecretName` (`[A-Z_][A-Z0-9_]*`, ≤ 100, not `GITHUB_`/`GITEA_`/`FORGEJO_`/`RBIT_`), `VariableName`, pure decisions: `aggregate(job statuses) -> RunStatus` (all skipped → skipped; all success-or-skipped → success; else running > waiting > blocked > awaiting > cancelled > failure), `runs_on(job labels, runner labels)` (non-empty subset), `runner_sees(scope, repo, owner)`. Exhaustively tested. |
| `rbit-workflow` (new crate) | `pattern` (filter-pattern port + its test table), `cron`, `model` (`Workflow`, `Job`, `On`, `Trigger`, `Strategy`, `DispatchInput`; unknown keys preserved as raw nodes for re-emission), `parse` (limits §6.5), `trigger` (`matches(&Workflow, &Event) -> bool` for push / pull_request / dispatch / schedule, given changed paths lazily), `plan` (matrix expansion, `runs-on` interpolation, `needs` validation — unknown or cyclic needs reject the workflow with a readable error), `payload` (one YAML document per job × combination). Depends on nothing internal. |
| `rbit-storage::sqlite::actions_*` | Stores (§5). |
| `rbit-services::actions` | `ActionsService` and its submodules (§3.1). |
| `rbit-web::actions_proto` | Generated `ping.v1` and `runner.v1` code + `Struct ↔ serde_json::Value` conversion. Only web knows prost. |
| `rbit-web::routes::actions_runner` | The Connect endpoint. |
| `rbit-web::routes::actions` | Pages: runs, run, job, logs, dispatch form; repository/owner/admin settings for runners, secrets, variables. |
| `rbit-web::routes::api::actions` | REST subset (§4.10). |

### 3.1 `rbit-services::actions` modules

| Module | Responsibility |
|---|---|
| `mod.rs` | `ActionsService` (cheap `Clone`, `Arc<Inner>`), `Inner { ctx, shutdown, tasks, started, versions, runners: RunnerCache, eval: Semaphore, keys: SecretKeys }`, `spawn_background`, `shutdown`. |
| `triggers.rs` | The `DomainEvent` subscriber: `RepositoryPushed` → push runs and schedule refresh; `Issue{kind: Pull, Opened/Reopened/Synchronized/Closed}` → pull request runs and cancellation of superseded ones. Evaluation runs on the bounded `eval` semaphore inside `spawn_blocking` (Git reads + YAML), never on the request path. `RecvError::Lagged` → log, count, rescan nothing (a missed push is recoverable by re-pushing; a rescan of every repository is not worth it). |
| `runs.rs` | Creating a run from a planned workflow (one transaction: run row, job rows, `pending` statuses after commit), concurrency, cancel, re-run, approve, listing. |
| `context.rs` | The `Task.context` document (§4.5) and GitHub-shaped `event` payloads (reusing `webhooks` payload builders for `push` and `pull_request`). |
| `dispatch.rs` | `workflow_dispatch`: input validation against the workflow's `inputs`. |
| `schedules.rs` | Schedule table refresh on default-branch pushes; the 60 s ticker. |
| `trust.rs` | Fork-PR trust decision (§6.2). |
| `runners.rs` | Registration tokens, `register`, `declare`, the in-memory `RunnerCache` (uuid → id, digest, scope, labels, disabled), `last_online` coalescing. |
| `scheduler.rs` | `fetch_task` (§4.3): `tasks_version`, claim, task-token minting, `Task` assembly (secrets, vars, needs). |
| `tasks.rs` | `update_task` (states, steps, outputs, heartbeat, finish → job → dependents → run → statuses), cancellation answer. |
| `logs.rs` | Log append/ack, finalise (gzip), reads by step and byte offset, retention. |
| `tokens.rs` | Task tokens: mint, digest, `authenticate(secret) -> Option<TaskGrant>` (re-checks task running + repository readable + Actions still enabled for it). |
| `secrets.rs` | `SecretKeys` (load or create the key file), seal/open, owner and repository secrets and variables CRUD. |
| `sweeper.rs` | Zombie (no heartbeat 10 min → failure), endless (running 3 h → failure), abandoned (waiting 24 h → cancelled), log retention, expired registration tokens. Every 60 s. |
| `hooks.rs` | `workflow_run` / `workflow_job` webhook payloads. |
| `statuses.rs` | Status context naming and the system-actor bridge into `StatusService`. |

## 4. Behaviour

### 4.1 Workflow discovery

On a trigger the service reads, at the triggering commit, the **first existing** of
`.forgejo/workflows` and `.github/workflows` (Forgejo precedence; a repository with both uses
only `.forgejo`). Files: `*.yml` / `*.yaml`, directly in that directory, at most 64 files,
each ≤ 256 KiB (`[actions] max_workflow_bytes`). A file that fails to parse or plan produces
**no run** but a visible "workflow error" entry on the Actions page for that commit (stored as
a run with status `failure`, zero jobs and the error text, ≤ 2 KiB), so authors learn why
nothing happened. Errors carry line/column from saphyr spans.

### 4.2 Triggers

`rbit-workflow::trigger` implements Forgejo's `detectMatched` semantics with GitHub's
pattern dialect:

- **push**: every listed filter kind must match; if both a branch and a tag filter are
  present one suffices; `paths*` are skipped for tag pushes; branch deletion never triggers;
  changed paths are computed between `before` and `after` (new branch: against the default
  branch's merge base, capped at 3,000 files — over the cap, `paths` filters count as
  matched, as on GitHub). One run per workflow per updated ref (a push of 3 branches can
  start 3 runs; at most 16 ref updates per push are considered).
- **pull_request**: default types `opened`, `reopened`, `synchronize`; also `closed`
  (`merged` in the payload). `branches*` match the **base** branch, `paths*` the pull
  request's changed files. `ref` is `refs/pull/{n}/head`, `sha` the head commit — fork
  heads already live in the base repository. Workflow files come from the **head** commit
  (the `pull_request` contract); the trust rules of §6.2 decide what that run may touch.
- **workflow_dispatch**: from the UI or API, a ref chosen by a writer; workflow read at that
  ref; inputs validated (required, `choice` options, `boolean`, `number`), ≤ 10 inputs set,
  values ≤ 1 KiB.
- **schedule**: workflows on the **default branch** only; refreshed on each push that moves
  it and when the repository's Actions switch is turned on; minimum interval 5 minutes;
  a run uses the default branch head at fire time. Paused automatically (and shown so) when
  the repository is archived, Actions is off for it, or 3 consecutive scheduled runs of a
  workflow fail to plan.
- Skip markers in the head commit message (push) or the pull request title skip push and
  pull_request runs.

### 4.3 Scheduling and fetch

- A run's jobs start `waiting` (no needs) or `blocked`. When a job finishes, each blocked
  dependent whose needs are **all** finished moves to `waiting` regardless of result — the
  runner's `if:` (default `success()`) decides. A failed/cancelled need therefore costs one
  quick skipped task, which matches Forgejo exactly.
- **`tasks_version`**: one in-memory `AtomicI64`, initialised to the current Unix time in
  milliseconds (never 0, never equal to a pre-restart value a runner might hold), bumped
  whenever any job becomes `waiting` or a runner's labels change. `FetchTask` with the
  current version answers `{task: none, tasks_version}` after the cache-only auth check —
  **no SQLite access on an idle poll**.
- Otherwise the scheduler reads the waiting jobs visible to the runner's scope
  (`actions_jobs` partial index `WHERE status = 5`, oldest first, limit 200), filters by
  labels in Rust (`rbit_core::actions::runs_on`), and claims the first with
  `UPDATE actions_jobs SET status = 6, task_id = ?, runner_id = ? WHERE id = ? AND status = 5`
  inside one write transaction that also inserts the task row (affected rows must be 1;
  otherwise try the next). An ephemeral runner that already holds a task gets none.
- Answer within 5 s (Gitea runners' fetch timeout); never long-poll.
- Forgejo's `x-runner-request-key` is stored on the task; a retry with the same key returns
  the same task with a **freshly minted** token (only digests are stored), so a lost response
  does not create a zombie.

### 4.4 Task updates, cancellation and logs (runner side)

- `UpdateTask`: the task must belong to the calling runner (`permission_denied` otherwise);
  updates to a finished task are ignored and answered with the stored result. A non-final
  update is the heartbeat (coalesced to one write per 10 s per task unless steps changed).
  Steps are keyed by index; outputs are inserted if absent (key ≤ 255 bytes, value ≤ 1 MiB,
  ≤ 1 MiB in total per job; oversize ignored and logged once); `sent_outputs` lists every
  stored key. The response carries `RESULT_CANCELLED` when the job was cancelled — that is
  how cancellation reaches the runner (pull-based, ≤ 1 s).
- A final result finishes the task (token digest cleared), the job, unblocks dependents,
  recomputes the run (`aggregate`), updates statuses, publishes `ActionsJob` / `ActionsRun`
  events, deletes an ephemeral runner.
- `UpdateLog` follows the ack protocol exactly (§5.5).

### 4.5 What a task carries

- `workflow_payload`: the single-job YAML from `rbit-workflow::payload` (top-level `name`,
  `run-name`, `on`, `env`, `defaults`, `permissions` copied; `jobs` with exactly one key;
  matrix rewritten to the chosen combination as one-element lists; `runs-on` interpolated;
  job `name` defaulting to `"<id> (<values>)"` for matrix jobs).
- `context`: `event` (GitHub-shaped payload), `event_name`, `ref`, `ref_name`, `ref_type`,
  `sha`, `head_ref`, `base_ref`, `repository`, `repository_owner`, `repository_id`,
  `actor`, `triggering_actor`, `run_id`, `run_number`, `run_attempt`, `job`, `workflow`,
  `workflow_ref`, `token`, `server_url`, `api_url` (`<public_url>/api/v1`),
  `retention_days`, `forgejo_default_actions_url` **and** `gitea_default_actions_url`
  (Forgejo Runner fails the task without one), `forgejo_server_version` /
  `gitea_server_version` (`rbit <version>` unless `[server]` hides the version). Ids are
  strings.
- `secrets`: repository then owner secrets (repository wins on name clash), plus
  `GITHUB_TOKEN`, `GITEA_TOKEN`, `FORGEJO_TOKEN` = the task token. Empty except the tokens
  for untrusted contexts (§6.2).
- `vars`: repository then owner variables; empty for fork runs.
- `needs`: an entry for **every** job id in the job's `needs` (outputs merged across matrix
  instances, result aggregated) — the runner rebuilds `needs` from these keys.

### 4.6 Concurrency, cancel, re-run, approval

- Workflow-level `concurrency.group` is evaluated with the `github.*` subset rbit knows
  (`github.ref`, `github.head_ref`, `github.base_ref`, `github.workflow`, `github.event_name`,
  `github.repository`, `github.run_id`) and literals; any other expression is a planning
  error. `cancel-in-progress: true` cancels earlier non-finished runs of the same group in
  the repository; `false` (default) makes the new run wait (`blocked`) until the group is
  free.
- A new `synchronize` cancels nothing by itself — that is what `concurrency` is for.
  Closing a pull request cancels its waiting and awaiting-approval runs.
- **Cancel** (write access): waiting/blocked/awaiting jobs → cancelled at once; running
  jobs → cancelled in the database, the runner learns on its next `UpdateTask`.
- **Re-run** (write access, finished runs only): all jobs, failed jobs (plus their
  dependents), or one job (plus dependents). `attempt += 1`, new task rows, `run_id` and
  `run_number` unchanged, `run_attempt` incremented. Re-running a fork run requires the same
  approval rule.
- **Approve** (write access): moves an awaiting run to waiting/blocked; optional "always run
  workflows from this user in this repository" adds a trusted-user row (audited).

### 4.7 Commit statuses

One status per job: context `"<workflow name> / <job name> (<event>)"` (truncated to 255
characters on a character boundary), `target_url` the job page, descriptions "Waiting for
approval", "Queued", "In progress", "Successful in 1m 3s", "Failing after 12s", "Cancelled",
"Skipped" (`success`). Posted on queue, start and finish — never on heartbeats. Scheduled
runs post none (no commit is being judged). Implemented by a crate-private
`StatusService::create_as_system`: the store's `creator` and `DomainEvent::CommitStatus.actor`
become `Option<AccountId>` (the column is already nullable); webhooks render a `None` sender as
`{"login": "rbit-actions[bot]", "type": "Bot", "id": 0}`. Required checks (branch
protection) work unchanged.

### 4.8 Task tokens in Git and the API

- `rbit_auth::task_token` (new module): `TaskTokenSecret` (`rbat_` + 43 base64url),
  `looks_like_task_token`, digest. Same construction as personal access tokens.
- Git smart HTTP `authenticate`: a Basic password that `looks_like_task_token` is **never**
  tried as a password. With Actions disabled → `401` without touching the database. Enabled
  → `ThrottleScope::GitAuth`, then `ActionsService::tokens().authenticate`. `GitCaller`
  becomes `enum GitCaller { User { user, token_scope }, Task(TaskGrant) }`; `authorize`
  grants `TaskGrant.access` (read, or write) on the task's repository and anonymous access
  elsewhere. A task is never an allowed pusher under branch protection and never bypasses it
  (the deploy-key rule).
- `actions/checkout` fetches `<origin>/<owner>/<repo>` **without `.git`** and fetches by
  SHA: covered by an e2e test (§8) — fetch of an unadvertised reachable SHA over protocol v2
  and v0. If v0 refuses, task callers' upload-pack runs with
  `-c uploadpack.allowReachableSHA1InWant=true`.
- Pushes authenticated by a task token mark the event: `RepositoryPushed` gains
  `by_task: Option<TaskId>`; `triggers.rs` ignores those pushes (statuses, webhooks and
  pull request synchronisation still happen).
- REST API: `Caller` becomes `enum { User { user, scope }, Task(TaskGrant) }`. Task callers
  may use repository endpoints of their own repository at the granted level and public
  repositories read-only; user-scoped endpoints (`/user`, tokens, keys, admin) answer `403`.
  `POST …/dispatches` with a task token answers `403` (no recursive runs).

### 4.9 UI

Monochrome minimal (`.box`, `.row`, `.btn`, `.badge`, `.tab`, `.state-badge`), colour only
for status (`--success`, `--danger`, `--warning`, `--ink-muted`), 375 px first, ≥ 44 px
targets, no inline styles or scripts, every page works without JavaScript. Icons: the
existing `macros.html` set plus `play`, `stop`, `rerun`, `clock`, `skip` if missing.

- **Repository tab "Actions"** (`data-repo-tab="actions"`, between Pull requests and Settings,
  inside the `subnav` partial markers) — present only when the instance *and* the repository
  have Actions on (`RepoHeader.actions_url: Option<String>`).
- **Runs list** `/{o}/{r}/actions`: workflow sidebar (stacked above the list below `md:`),
  filters (workflow, branch, event, status, actor) as GET parameters, newest first, paged by
  keyset (`id < ?`). Each row: state icon, run title (commit subject / PR title / "Scheduled"),
  `#run_number`, workflow, event, branch, actor, relative time, duration. Workflow-error rows
  show the error inline.
- **Run** `/{o}/{r}/actions/runs/{n}`: summary (trigger, commit, actor, total duration,
  attempt picker), jobs grouped in `needs` order as rows, buttons Cancel / Re-run all / Re-run
  failed / Approve (with the "always trust" checkbox) for writers; a warning box for awaiting
  approval explaining why. `hx-trigger="every 3s"` refresh of the job list while not
  finished, as `import_status.html` does.
- **Job** `/{o}/{r}/actions/runs/{n}/jobs/{job}` (`?attempt=`): steps as `<details>`
  elements (no JS needed to expand), step duration and state; the running step is open.
  Log lines in `.code-lines` (virtualised `.vblock` for long logs), escaped, ANSI stripped,
  timestamps toggle via a GET parameter. Live tail (§5.5). "Raw log" link
  (`…/logs/raw`, `text/plain; charset=utf-8`, `Content-Disposition: inline`, served
  pre-gzipped when accepted).
- **Dispatch form** on the workflow's page for writers when the workflow has
  `workflow_dispatch`: ref picker (existing `ref_picker`), inputs rendered by type.
- **Repository settings → Actions card** (admins; card only when the instance has Actions
  on): the repository switch; runners (repository runners + those inherited from owner and
  instance, read-only for inherited), "New registration token" (shown once, with the
  `forgejo-runner register` command), remove/disable runner; secrets (write-only: name,
  updated time, replace, delete — the value is never rendered back); variables (shown);
  trusted users.
- **Organisation settings → Actions**: owner runners, secrets, variables (owners only).
  User accounts: the same under personal settings for user-owned repositories.
- **Admin → Runners**: instance runners and registration tokens, all runners with their
  scope, last seen, version, labels; disable/remove. In the admin nav only when enabled.
- Secret pages carry `hx-history="false"`; registration tokens are shown once like personal
  access tokens.

### 4.10 REST API (`/api/v1`, GitHub-shaped subset)

`GET /repos/{o}/{r}/actions/runs` (filters `branch`, `event`, `status`, `actor`),
`GET …/actions/runs/{run_id}`, `GET …/actions/runs/{run_id}/jobs`,
`GET …/actions/jobs/{job_id}`, `GET …/actions/jobs/{job_id}/logs` (302-free: streams the raw
log), `POST …/actions/runs/{run_id}/cancel`, `POST …/actions/runs/{run_id}/rerun`,
`POST …/actions/runs/{run_id}/rerun-failed-jobs`, `GET …/actions/workflows`,
`POST …/actions/workflows/{file}/dispatches` (`{ref, inputs}`, 204),
`GET/PUT/DELETE …/actions/secrets/{name}` (PUT takes the plain value over TLS — no GitHub
libsodium sealed box; documented difference) and `…/actions/variables/{name}`,
`POST …/actions/runners/registration-token`. Scopes: `read` for reads, `write` for
cancel/rerun/dispatch, `admin` for secrets, variables and runners. One `openapi/actions.json`
fragment.

### 4.11 Webhooks

`WebhookEvent::WorkflowRun` (`workflow_run`: `requested`, `in_progress`, `completed`) and
`WorkflowJob` (`workflow_job`: `queued`, `in_progress`, `completed`, `waiting`), GitHub-shaped
subsets. New `DomainEvent::ActionsRun { repo, run, action }` and
`ActionsJob { repo, run, job, action }` appended at the end of the enum. The webhook form
offers them only when Actions is enabled (`WebhookEvent::subscribable(&config)` replaces the
const array at its use sites); stored subscriptions to them are kept but never fire when
disabled.

## 5. Storage

Migration **`0044_actions.sql`** (next free number; `0023`, `0025`–`0031` and `0035` stay
unused). `STRICT`, Unix seconds, explicit `ON DELETE`. Status columns are integers with
`CHECK` against the `JobStatus` values.

| Table | Key columns | Notes |
|---|---|---|
| `actions_repositories` | `repo_id` PK → repositories CASCADE, `enabled`, `updated_at` | The per-repository switch. No row = default from config at creation time (a row is written when the repository is created *only if* the instance has Actions on). |
| `actions_runners` | `id`, `uuid` UNIQUE, `token_digest` BLOB UNIQUE, `name`, `scope_kind`, `owner_id` → accounts CASCADE, `repo_id` → repositories CASCADE, `labels` (JSON array), `version`, `ephemeral`, `disabled`, `last_online`, `created_at` | CHECK: scope columns consistent with `scope_kind`. |
| `actions_registration_tokens` | `id`, `digest` UNIQUE, scope columns, `created_by` → accounts SET NULL, `created_at`, `expires_at` NULL, `max_uses` NULL, `uses`, `revoked_at` NULL | Plain value shown once. Default: no expiry, unlimited uses, revocable (Forgejo-compatible workflow, hashed). |
| `actions_runs` | `id`, `repo_id` CASCADE, `run_number` (UNIQUE with repo), `workflow_path`, `workflow_name`, `title`, `event`, `ref`, `sha`, `actor_id` SET NULL, `trigger_actor_id` SET NULL, `pull_number` NULL, `is_fork`, `status`, `attempt`, `concurrency_group` NULL, `event_payload` (JSON, ≤ 256 KiB), `dispatch_inputs` NULL, `error` NULL, `approved_by` SET NULL, `created_at`, `started_at`, `finished_at` | Indexes `(repo_id, id DESC)`, `(repo_id, workflow_path, id DESC)`, `(repo_id, concurrency_group, status)` partial on unfinished. |
| `actions_run_numbers` | `repo_id` PK, `last` | Monotonic per repository, survives run deletion. |
| `actions_jobs` | `id`, `run_id` CASCADE, `job_key`, `name`, `matrix` (JSON) NULL, `needs` (JSON array), `runs_on` (JSON array), `payload` BLOB, `status`, `attempt`, `task_id` NULL, `runner_id` SET NULL, `outputs` JSON, `created_at`, `started_at`, `finished_at` | Partial index `(id) WHERE status = 5` for fetch; `(run_id)`. |
| `actions_tasks` | `id` INTEGER PRIMARY KEY **AUTOINCREMENT** (never reused), `job_id` CASCADE, `attempt`, `runner_id` SET NULL, `token_digest` BLOB UNIQUE NULL, `request_key` NULL, `status`, `started_at`, `stopped_at`, `heartbeat_at`, `log_lines`, `log_bytes`, `log_state` (`live`, `final`, `expired`, `truncated` flag) | Partial index `(heartbeat_at) WHERE status = 6` for the zombie sweep. |
| `actions_steps` | `(task_id, number)` PK, `name`, `status`, `started_at`, `stopped_at`, `log_index`, `log_length` | |
| `actions_secrets` | `id`, scope columns, `name`, `key_id` BLOB(8), `nonce` BLOB(12), `ciphertext` BLOB, `created_at`, `updated_at`, `updated_by` SET NULL | UNIQUE per (scope, name). ≤ 100 per scope, value ≤ 64 KiB. |
| `actions_variables` | same shape with `value` TEXT | ≤ 100 per scope, ≤ 48 KiB each. |
| `actions_trusted_users` | `(repo_id, user_id)` PK, `approved_by`, `created_at` | |
| `actions_schedules` | `(repo_id, workflow_path, cron)` PK, `next_at`, `failures`, `paused` | Index `(next_at) WHERE paused = 0`. |

Stores (`Database::actions_runners()`, `actions_runs()` — runs, jobs, tasks, steps, the state
machine in single transactions — `actions_secrets()` — secrets and variables —
`actions_schedules()`, `actions_settings()`). `tests/upgrade.rs` checks the tables exist, start
empty, and their CHECKs and cascades.

### 5.5 Logs

- Path: `<data_dir>/actions/logs/<repo_id>/<task_id>.log` while live, plus
  `<task_id>.idx` (little-endian `u64` byte offset of each line; 8 bytes per line — random
  access to a step's line range without scanning).
- Line format: RFC 3339 UTC timestamp, a space, the content with `\r` and `\n` escaped. Rows
  longer than 64 KiB are truncated with a marker.
- `UpdateLog`: `ack = log_lines`. Empty rows, `index > ack` or `index + len <= ack` → return
  `ack`. Otherwise append rows from `ack - index`, `flush` + `sync_data` both files, then
  update `log_lines`/`log_bytes` in SQLite. On start-up and before the first append of a
  process, files longer than `log_bytes` are truncated to it (crash between file append and
  row update). Beyond `max_log_bytes` (default 64 MiB) rows are acknowledged but dropped, one
  "log truncated" marker is written and `truncated` set. `no_more` → finalise on the
  blocking pool: gzip (level 6, `flate2` — already in the tree) to `<task_id>.log.gz`,
  keep `.idx`, remove the plain file, `log_state = final`; further `UpdateLog` →
  `already_exists`.
- Reads: live — plain file from a byte offset; final — streaming gunzip. A step view renders
  at most the last 10,000 lines / 2 MiB and links to the raw log for the rest.
- Live tail: the job page's open step polls `…/jobs/{job}/logs?step=N&after=<byte offset>`
  every 2 s with `hx-swap="beforeend"`; the fragment carries the next cursor as an
  out-of-band swap of the poller element and drops `hx-trigger` once the step finished.
- Retention: `[actions] log_retention_days` (90); the sweeper deletes files and sets
  `log_state = expired` (run and job rows stay; `run_retention_days` (365) deletes old runs).
- Backups: `<data_dir>/actions/` is **not** copied (logs are derived build output, the key
  file is backed up separately on purpose — §6.3). The backup manifest stays format 3;
  `docs/deployment.md` lists `actions/` as "not backed up" and documents backing up the key.

## 6. Security

### 6.1 Threat model

| Asset / actor | Threat | Control |
|---|---|---|
| Secrets | DB file, backup or replica leaks | AES-256-GCM with a key **outside** SQLite and outside backups; AAD binds scope + name (a ciphertext moved to another repository fails to open). |
| Secrets | Shown back in UI/API/logs | Write-only everywhere; `SecretValue` wraps `Zeroizing<String>` with a redacting `Debug`; never logged by tracing; runner-side masking plus a server defence-in-depth exact-match pass over log rows for values the task received (≥ 4 characters; replaced with `***` before writing). |
| Secrets | Exfiltrated by a fork pull request | Fork runs get no secrets and no variables, read-only token, and need approval unless trusted (§6.2). |
| Repository | Task token reused after the job / against other repos | Valid only while the task runs (digest cleared on finish, lookup requires `status = running`), re-checked against current repository readability and the repository's Actions switch on every use; write only on its own repository; never a protection bypass. |
| Runners | Impersonation / cross-tenant writes | Runner token hashed, constant-time compare via digest lookup; every `UpdateTask`/`UpdateLog` checks `task.runner_id == caller`; runners only see jobs in their scope (`runner_sees`). |
| Registration | Token theft | Stored hashed, shown once, revocable, optional expiry and use limit; registration throttled (`ThrottleScope::RunnerRegister`). |
| Server | DoS via RPC bodies, logs, outputs, YAML | Body caps per RPC (§6.4), log cap per task, outputs caps, YAML budgets and limits (§6.5), bounded evaluation concurrency, `RunnerAuth` throttle on failed auth. |
| Server | Push-path latency | Disabled: zero work. Enabled: the push handler only publishes the existing event; evaluation is async on a semaphore. |
| Users | Untrusted workflow content on shared runners | Operator guidance: container or VM runners, ephemeral runners for public repositories, repository-scoped runners for sensitive work; the "new registration token" page says host runners give no isolation. |
| Users | Cache poisoning across PRs | Correct `event_name`, `ref = refs/pull/N/head` and `pull_request.merged` in the context — Forgejo Runner isolates cache writes on them. |
| Cross-object IDs | Acting on another repository's run/runner/secret by id | Every route resolves the repository first and queries by `(repo_id, id)`; tests for each kind (Forgejo 10.0.1 lesson). |

### 6.2 Fork pull requests and trust

A run is a **fork run** when its event is `pull_request` and the head repository differs from
the base repository. A fork run:

- gets a token with **read** on the base repository, **no secrets**, **no variables**;
- is created `AwaitingApproval` (jobs not dispatched, statuses "Waiting for approval")
  **unless the pull request author is trusted**, where trusted means any of:
  write access to the base repository (`repo_access`), a row in `actions_trusted_users`,
  or at least one previously **merged** pull request by that author into the base repository;
- approval is per run (per head SHA): a new `synchronize` creates a new awaiting run;
  approving requires write access and is audited.

Same-repository pull requests (author pushed a branch, so has write access) are normal runs
with a write token and secrets. A suspended author's runs are never approved automatically.

### 6.3 The secrets key

`<data_dir>/actions/secrets.key` (override `[actions] secrets_key_file`, e.g. a systemd
credential path): `rbit-actions-key-v1:` + base64 of 32 random bytes, created with mode `0600`
(Unix) on first start with Actions enabled, refused at start-up if group/world readable.
`key_id` = first 8 bytes of SHA-256(key) stored per secret, so a wrong or rotated key is
detected rather than producing garbage. Missing or wrong key: rbit starts, secret pages show
"cannot be decrypted — set it again", and a run that needs an unreadable secret **fails
closed** with that message in its log. Rotation is out of scope (re-enter secrets; documented).
Rationale: SQLite and `rbit backup` archives are the things people copy around; a key beside
them in the same file protects nothing. An attacker with the whole data directory and the key
reads secrets — as with every self-hosted forge; the runner must receive plaintext anyway.

### 6.4 RPC limits

| RPC | Body cap | Other |
|---|---|---|
| Ping, Register, Declare, FetchTask, UpdateTask | 1 MiB | ≤ 64 labels, ≤ 1,000 step states |
| UpdateLog | 8 MiB | ≤ 10,000 rows per call |
| unknown method | — | `unimplemented` (501) |

Identity encoding only (`Content-Encoding` other than identity → `invalid_argument`).
No page timeout layer (runners hold connections briefly anyway) but a 30 s handler timeout
and its own `DefaultBodyLimit`; merged outside `page_routes` (no CSRF/session), declined by
`preload_guard`. Failed runner auth spends `ThrottleScope::RunnerAuth` per client address.

### 6.5 Parsing limits

256 KiB per file, 64 files, YAML nesting depth 64, alias expansions budgeted by serde-saphyr
(no billion laughs), ≤ 256 jobs per workflow, ≤ 256 matrix combinations per job and ≤ 1,024
jobs per run, `needs` acyclic, job ids `[A-Za-z_][A-Za-z0-9_-]*`, evaluation of one trigger
bounded by a 10 s budget on the blocking pool (exceeding it → workflow error).

## 7. Configuration

```toml
[actions]
# CI/CD on external Forgejo or Gitea runners. Off by default; when off, Actions is fully
# absent: no pages, no runner endpoint, no background work, workflow files are ignored.
# Changing it needs a restart.
enabled = false
# Turn Actions on for repositories created from now on (existing ones keep their setting).
enable_for_new_repositories = false
# Where `uses: owner/repo@ref` actions are fetched from: a URL, or "self" for this instance.
default_actions_url = "https://data.forgejo.org"
# Encryption key for secrets; empty = <data_dir>/actions/secrets.key. Back it up separately.
secrets_key_file = ""
log_retention_days = 90
run_retention_days = 365
max_log_bytes = 67108864
max_workflow_bytes = 262144
# Fail a task with no heartbeat, a task running too long, cancel a job nobody picks up.
zombie_timeout_seconds = 600
endless_timeout_seconds = 10800
abandoned_timeout_seconds = 86400
# Workflow evaluations (Git reads + YAML) running at once.
evaluation_concurrency = 2
```

Env: `RBIT_ACTIONS__ENABLED=true`. `validate_configuration` checks the URL (`http(s)` or
`self`), non-zero timeouts, `evaluation_concurrency` 1–16. The admin configuration schema gets
an "Actions" section with a `Kind::Toggle` per switch (the schema-completeness test requires
it). Admin-staged changes take effect on restart, like every section there.

## 8. Tests — and the proof that "disabled" means disabled

Per level, as CONTRIBUTING requires:

- **`rbit-core`**: `aggregate`, `runs_on`, `runner_sees`, name/label validation — exhaustive
  tables.
- **`rbit-workflow`**: the ported `workflowpattern` table verbatim; trigger matrices for
  every filter combination (including tag-plus-paths, branch-plus-tag, deletion); cron
  against hand-computed next-fire times including month ends and DST-free UTC; matrix
  expansion golden files **adapted from forgejo/runner `act/jobparser/testdata`** (MIT; noted
  in `THIRD-PARTY.md`); limit and budget rejections; proptest: parse never panics on
  arbitrary input.
- **Storage**: claim races (two claims, one wins), cascades, CHECKs, `upgrade.rs`.
- **Services**: trigger → run → jobs → claim → update → dependents → run result → statuses
  without the network; fork trust table; secrets seal/open, wrong key, AAD swap; token
  lifecycle (dead after finish, dead when repository becomes unreadable).
- **Web**: Connect codec (golden protobuf bytes for every message, error bodies, 415, 501),
  routes per page and permission (reader, writer, admin, anonymous, other repository's ids).
- **e2e** (`rbit-e2e`): a **fake runner** (reqwest + the generated prost types) registers,
  declares, fetches, streams logs, finishes; then `git` fetches the repository at
  `/{o}/{r}` (no `.git`) **by SHA** with the task token as Basic password, and fails after
  the task finished. An optional test runs a real `forgejo-runner` (path in
  `RBIT_E2E_FORGEJO_RUNNER`, host label `self-hosted:host`) end to end, skipped when unset
  (the git-lfs precedent).

**Disabled-is-disabled suite** (`crates/rbit-web/tests/actions_disabled.rs` and
`crates/rbit-e2e/tests/e2e/actions_disabled.rs`), all with the default config:

1. **Route inventory**: `routes::actions::PATHS` and `actions_runner::PATHS` (consts listing
   every path template) — each, with each method and a representative parameter set, answers
   `404` (the Connect `Register` included: 404, not 501). The OpenAPI document contains no
   `/actions` path; `api::table(&config)` registers none (the equality test runs for both a
   disabled and an enabled config).
2. **No writes**: an e2e server pushes a repository containing `.forgejo/workflows/ci.yml`
   and `.github/workflows/ci.yml` (push, pull_request, schedule triggers), opens a pull
   request from a fork, dispatches via API (404), waits for the event bus to drain, then
   asserts every `actions_*` table has `0` rows (queried via `sqlite_master` so new tables are
   covered automatically) and `<data_dir>/actions` does not exist.
3. **No UI**: repository page, settings, organisation settings, admin pages and the webhook
   form contain no `actions` tab, card, nav entry or `workflow_run`/`workflow_job` option.
4. **No credentials**: a `rbat_…` password on Git HTTP and as API bearer → `401`, with a
   database spy (statement counter in the test harness) proving no query ran.
5. **No background work**: `Services::actions.is_none()`; `spawn_background` spawns the same
   number of tasks as before this feature (a `TaskTracker` count exposed for tests).
6. **Config**: `ActionsConfig::default().enabled == false`.

## 9. Observability

Metrics (bounded labels only): `rbit_actions_runs_total{event,outcome}`,
`rbit_actions_jobs_total{outcome}`, `rbit_actions_job_queue_seconds` (waiting → started),
`rbit_actions_job_duration_seconds`, `rbit_actions_rpc_total{method,code}`,
`rbit_actions_jobs_waiting` (gauge), `rbit_actions_runners_online` (gauge, seen in the last
2 minutes), `rbit_actions_log_bytes_total`, `rbit_actions_workflow_errors_total{reason}`
(`parse`, `limit`, `plan`, `timeout`).

Audit (`AuditCategory::Actions`, new): repository Actions switched on/off, runner
registered/disabled/removed, registration token created/revoked, secret created/updated/
deleted (name only), variable changed, run approved, user trusted/untrusted.

Tracing: run/job/task ids, never tokens, secrets, payloads or log content.

## 10. Implementation plan

Branch `feature/actions` (worktree `D:\Development\tools\rbit-actions`, from upstream `main`
`f6f9af7`). Every commit: `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets --locked -- -D warnings`, `cargo test --workspace`, and when dependencies
change `cargo deny --locked check licenses advisories`; CSS rebuilt with `npm run build` and
committed when templates add classes. Imperative subjects ≤ 72 characters, bodies explain
why, **human attribution only — no AI co-author trailers**. Never push, never touch `main`.

### Workstreams

| Stream | Commits | Runs | Owns (exclusive) |
|---|---|---|---|
| **F** foundation | 1–8 | first, sequential, on `feature/actions` | `rbit-core::{config::actions, actions}`, `crates/rbit-workflow/**`, `migrations/0044_actions.sql`, `rbit-storage/src/sqlite/actions_*.rs`, `rbit-storage/tests/actions.rs`, `rbit-services/src/actions/mod.rs` (skeleton with every submodule declared, each file a `//!` doc stub), `routes/actions/mod.rs` + `routes/actions_runner.rs` stubs, the disabled suite. |
| **R** runner protocol | 9–14 | parallel after commit 8, worktree `rbit-actions-R`, branch `actions/runner` | `tools/protogen/**`, `crates/rbit-web/proto/**`, `rbit-web/src/actions_proto/**`, `routes/actions_runner.rs`, `services/actions/{runners,scheduler,tasks,logs,tokens,sweeper}.rs`, `rbit-auth/src/task_token.rs`, `routes/git_http.rs`, `routes/api/auth.rs`. |
| **T** triggers and runs | 15–20 | parallel after commit 8, worktree `rbit-actions-T`, branch `actions/triggers` | `services/actions/{triggers,runs,context,dispatch,schedules,trust,statuses}.rs`, `services/statuses.rs`, `rbit-storage/src/sqlite/statuses.rs`, `services/pushes.rs` (the `by_task` field), `events.rs` (`RepositoryPushed.by_task`, `CommitStatus.actor` option). |
| **U** interface and secrets | 21–25 | parallel after commit 8, worktree `rbit-actions-U`, branch `actions/ui` | `services/actions/secrets.rs`, `routes/actions/**` (except `mod.rs` route list lines added by F), `views/actions/**`, `templates/actions/**`, `templates/repo/subnav.html`, `views/repo.rs` (`actions_url`), settings/admin nav files, `assets/` CSS output. |
| **I** integration | 26–30 | after R, T, U are rebased onto `feature/actions` in order R → T → U | webhooks, REST API, e2e, docs. |

Three parallel streams at most (concurrent `cargo` builds starve the machine beyond that);
`CARGO_BUILD_JOBS=8` each. Shared registration points (`services/lib.rs`, `routes/mod.rs`,
`config/mod.rs`, `sqlite/mod.rs`, `events.rs`, `api/mod.rs`, `webhook.rs`, `audit.rs`,
`throttle.rs`) are wired by **F** for everything F creates; R/T/U append one marked block
each where they must (`// Actions: <stream>`), so rebases conflict only as "keep both
sides". R depends on T only through the store (built by F) — R tests seed runs through
the store; T tests claim through the store; U renders fixtures from the store.

### Commits

**F — foundation**

1. `Add the [actions] configuration section, off by default` — `config/actions.rs`,
   `Config.actions`, `validate_configuration`, admin configuration schema section + toggles,
   example in the module doc. Test: defaults, validation, schema completeness.
2. `Add Actions domain types and scheduling rules to rbit-core` — `rbit_core::actions`.
3. `Add the rbit-workflow crate with GitHub filter patterns` — crate skeleton, workspace
   member, `pattern` port + test table, THIRD-PARTY.md entry.
4. `Parse workflow files into a typed model with limits` — saphyr + serde-saphyr (new
   workspace dependency block `# --- Actions ---`), `model`, `parse`, budgets, proptest.
5. `Match push, pull request, dispatch and schedule triggers` — `trigger`, `cron`.
6. `Expand matrices and emit single-job workflow payloads` — `plan`, `payload`, golden files.
7. `Store Actions runs, jobs, runners and secrets` — `0044_actions.sql`, stores, store tests,
   `upgrade.rs`.
8. `Wire an ActionsService that exists only when Actions is enabled` — `Services.actions:
   Option<ActionsService>`, `spawn_background`/`shutdown` lines, conditional router merge
   (`if let Some(actions) = &state.services.actions`), `api::table(&config)`, the
   disabled-is-disabled suite (inventory, no writes, no UI, no background work) against the
   stub routes. This is the base the three streams fork from.

**R — runner protocol** (on `actions/runner`)

9. `Add a protobuf generator and the runner protocol types` — `tools/protogen` (protox +
   prost-build), `proto/{ping,runner}.proto` (union of Forgejo v0.8.0 and Gitea fields, with
   source notes), generated `actions_proto/*.rs`, drift test, `Struct ↔ JSON`.
10. `Serve the Connect runner endpoint with ping and errors` — codec, limits, 415/501,
    `PATHS`, golden-byte tests.
11. `Register runners with hashed registration tokens` — `Register`, `Declare`,
    `RunnerCache`, `RunnerAuth`/`RunnerRegister` throttles, audit.
12. `Assign waiting jobs to runners with task tokens` — `tasks_version`, claim, request key,
    `rbat_` tokens, task assembly (needs, vars; secrets via a trait U implements — F stubs
    it as "none").
13. `Record task state, outputs and logs from runners` — `UpdateTask`, `UpdateLog`, log
    files, finalise, cancellation answer, ephemeral deletion.
14. `Accept task tokens for Git over HTTP and the API` + sweeper — `GitCaller`/`Caller`
    enums, `uploadpack` fallback if needed, zombie/endless/abandoned/retention sweeper.
    (If this exceeds one reviewable commit, split the sweeper out as 14b `Fail stalled
    tasks and expire old logs`.)

**T — triggers and runs** (on `actions/triggers`)

15. `Let commit statuses come from the system` — `Option` creator/actor, bot sender,
    `create_as_system`.
16. `Start workflow runs on pushes` — subscriber, discovery, skip markers, `by_task`
    suppression, workflow-error runs, `context.rs` for push, semaphore + budget.
17. `Start workflow runs for pull requests with fork trust` — `trust.rs`, approval state,
    closed-PR cancellation.
18. `Run workflows on demand and on schedules` — `dispatch.rs`, `schedules.rs`.
19. `Cancel, re-run and approve workflow runs` — plus `concurrency`.
20. `Report Actions jobs as commit statuses` — `statuses.rs`.

**U — interface and secrets** (on `actions/ui`)

21. `Encrypt Actions secrets with a key outside the database` — `secrets.rs`, key file,
    seal/open, the task-assembly trait implementation, audit.
22. `Show workflow runs in a repository Actions tab` — runs list, run page, subnav entry.
23. `Show job steps and follow their logs live` — job page, log fragments, raw download.
24. `Manage runners, secrets and variables in settings` — repository card, organisation and
    personal settings, registration-token page.
25. `List all runners in the administration panel`.

**I — integration** (on `feature/actions`, after rebasing R, T, U in that order)

26. `Send workflow_run and workflow_job webhooks`.
27. `Add the Actions REST API` — `openapi/actions.json`, both-config equality test.
28. `Test Actions end to end with a fake runner` — e2e scenarios incl. checkout by SHA, the
    optional real-runner test.
29. `Document Actions` — `README.md` (feature line + configuration row), `docs/deployment.md`
    (data-dir table: `actions/` not backed up; the key; runner setup with `forgejo-runner
    register`; metrics table; background jobs; outbound network: none from rbit itself),
    `deploy/rbit.example.toml` (`[actions]`), `docs/ARCHITECTURE.md` (Actions section, crate
    graph with `rbit-workflow`), `ROADMAP.md` (move the runner protocol to *Available today*;
    keep "built-in runner" under *Not planned*), `CHANGELOG.md` Unreleased (feature + upgrade
    note: migration `0044`, off by default, nothing changes until enabled).
30. `Record the Actions design as built` — this document gains an "As built" section listing
    deviations.

Every stream merges by **rebase** (linear history). After each rebase the full gate runs
again on the rebased tip before the next stream lands.

## 11. Security review amendments

An adversarial review of this contract (before implementation) found the issues below. Each
resolution here **supersedes** the earlier section it names.

### 11.1 Trust is decided per event, from the actor's current access (supersedes §6.2)

Opening a pull request needs only read access, so "same repository ⇒ author can write" is
false. Every `pull_request` run is classified when it is created:

- **Trusted** when the actor who caused the event has **write access to the base repository
  now**: the author for `opened`/`reopened`/`ready_for_review`/`edited`, the **pusher** of the
  new head for `synchronize` (from the push that moved the head; unknown pusher ⇒ untrusted).
- Otherwise **untrusted** — this covers fork pull requests *and* same-repository pull
  requests from readers or from former writers. An untrusted run always gets a **read-only
  token, no secrets and no variables**, whoever approves it.
- An untrusted run **waits for approval** (per run, per head SHA; approver needs write
  access; audited) unless the author is in `actions_trusted_users` for that repository or has
  a previously merged pull request into it *and* the pusher is that author. Approval and
  trust never grant secrets to an untrusted run; they only skip the wait.
- A suspended author's runs are never approved automatically.

Workflows of an untrusted run come from the pull request head (GitHub's behaviour); the docs
state that a required check from such a run proves only what the head's workflow does.

### 11.2 Where a push came from (supersedes `RepositoryPushed.by_task` in §10/T)

`RepositoryPushed` carries `source: PushSource` — `User(AccountId)`, `DeployKey`, `Mirror`,
`Import`, `Task(TaskId)`, `System` — replacing the ambiguous `pusher: Option<AccountId>` for
this decision (keep `pusher` for existing consumers). Only `User` and `DeployKey` pushes start
runs. `Mirror`, `Import`, `Task` and `System` never do — including the `synchronize` of a pull
request whose head moved because of such a push.

### 11.3 Task callers are allowlisted (supersedes §4.8 for the API)

A task token is a **caller kind of its own**, never a synthetic user. It is accepted only by:
Git smart HTTP (fetch; push when the token has `contents: write`), LFS on its own repository
(download; upload when `contents: write`), and an explicit API allowlist — reading the
repository's contents, refs, commits, releases; creating commit statuses; creating releases
and release assets (when `contents: write`); commenting on issues and pull requests of its own
repository (when the repository token permission is write). Everything else answers `403`. A
test walks `api::table` and fails for any route that accepts a task caller without being on
the list. Task callers may never dispatch, re-run, cancel, approve, fork, create tokens,
webhooks, runners or secrets, or open/close/reopen pull requests.

### 11.4 Token permissions (supersedes §1 "Task credential" permissions)

- Each repository has a **default workflow token permission**, `read` (default) or `write`,
  in its Actions settings.
- Top-level and job-level `permissions:` are honoured **coarsely**: `contents` and
  `issues`/`pull-requests`, `read` or `write`, `read-all`/`write-all`/`{}`. They can only
  **lower** the repository default, never raise it.
- Untrusted runs are always read-only.
- A task push that creates, changes or deletes anything under `.forgejo/workflows/` or
  `.github/workflows/` is refused, as GitHub refuses it without the `workflows` scope. The same
  applies to tag pushes that move an existing tag.

### 11.5 Recursion

Pushes, pull request events, statuses and comments made with a task token never start runs
(§11.2, §11.3). Re-run and dispatch through the API refuse task callers.

### 11.6 Concurrency (supersedes §4.6 concurrency)

- Concurrency is applied **when a run becomes runnable**: on creation for trusted runs, on
  approval for untrusted runs. An awaiting run cancels nothing.
- Untrusted runs' groups are namespaced `pr-<number>/<group>`, so they can only affect runs of
  the same pull request.
- The expression subset grows to cover the common idioms: `github.workflow`, `github.ref`,
  `github.head_ref`, `github.base_ref`, `github.event_name`, `github.run_id`,
  `github.event.pull_request.number`, string literals, and `||` / `&&` with GitHub's
  falsy rules.

### 11.7 `workflow_dispatch`

The ref must be one of the repository's own `refs/heads/*` or `refs/tags/*`. Never
`refs/pull/*`, and never a SHA that is reachable only from such a ref.

### 11.8 Scheduling fairness and caps (supersedes §4.3 fetch selection)

- Job labels are stored relationally (`actions_job_labels`). The claim query matches a
  runner's labels **in SQL**, so no window of unmatched jobs can hide newer ones.
- Selection is fair across repositories: the oldest waiting job of the repository whose
  oldest-claimable job waited longest.
- Caps (configurable): ≤ 256 waiting jobs per repository (new runs beyond the cap fail with
  a readable error), ≤ 32 runs and ≤ 1,024 jobs created per push event, ≤ 20 `schedule`
  entries per repository. A scheduled run is skipped while the previous scheduled run of the
  same workflow is still waiting.
- An instance-wide log budget (`max_log_storage_bytes`, default 10 GiB): when it is
  exceeded, the logs of the oldest finished runs are pruned first. Their runs keep a "log
  expired" marker.

### 11.9 Registration tokens and instance runners (supersedes §6.1 Registration)

- Registration tokens expire after 1 hour and are **single-use** by default. Both can be
  changed when the token is created, up to 30 days and unlimited uses. A token may pin
  labels: a runner registered with it can declare only those labels.
- Instance runners serve only owners an administrator has allowed (`actions_owner_policies`).
  When `[actions] instance_runners_for_all = true` (default `false`), they serve everyone. The
  registration page warns that host-mode runners give no isolation and that a runner sees the
  secrets of every job it runs.

### 11.10 Secrets and outputs (supersedes §6.1 Secrets and §6.3 in part)

- Owner secrets have a visibility: **all repositories** of the owner, or **selected
  repositories** (`actions_secret_repositories`).
- The AAD binds `(scope kind, scope id, secret id)`, not names.
- The server masking pass covers secrets, the task token, and every line (≥ 4 characters) of
  a multi-line secret. It runs over log rows **and** job outputs before they are stored.
  Outputs are capped at 1 MiB per job.
- The key file is never auto-created while sealed secrets exist. rbit then fails closed for
  secret operations and states the reason.
- The `0600` mode check applies on Unix only. On Windows the docs require the data directory's
  ACLs to be restricted, and rbit logs a start-up warning once.

### 11.11 Task lifecycle

- Cancelling a task clears its token digest immediately. The token stops working at once,
  not when the runner acknowledges.
- Fetch-retry recovery (`x-runner-request-key`) is looked up per **(runner, key)**.
- The sweeper also deletes ephemeral runners whose task ended by timeout.
- Evaluation keeps its semaphore permit until the blocking task actually returns. Parsing and
  planning check a cooperative deadline so the 10 s budget really stops work.

### 11.12 Smaller items

- Raw log downloads are throttled per client (`ThrottleScope::ActionsLogs`).
- Statuses created by rbit Actions record their source (`creator NULL` + a `source = actions`
  column). Requiring a check to come from Actions is a later protection option. Documented.
- When Actions is switched off, `pending` statuses it posted stay as they are. The docs say
  so; switching it back on lets the sweeper settle them.
- Pull request trigger types gain `ready_for_review` and `edited`, as far as rbit emits them.
- The docs state plainly that write access to a repository means being able to read its
  secrets through a workflow.
