# rbit ground up rewrite plan

Prepared 2026-11-05. Status: implementation authorized and active. Work proceeds in atomic commits against the pinned baseline; the phase exit gates below still govern completion.

The rewrite will retain Rust, Axum, Askama, HTMX, Tailwind CSS v4, SQLite and redb, preserve the platform's features, and reorganize the application around small, documented modules. A statically linked application microkernel will own lifecycle and resource coordination. Feature crates will own behavior, while the existing separate runner remains the execution boundary for CI jobs.

The baseline is **GitHub `GitRbit/rbit` main at `39f932a00b7aa35b324e06a9ff2343a57d4bed76`**, fetched on 2026-11-05. [Pinned source](https://github.com/GitRbit/rbit/tree/39f932a00b7aa35b324e06a9ff2343a57d4bed76). Neither the active pull request nor the local `main` branch is the baseline. Later main changes will enter through a reviewed baseline update and feature-ledger reconciliation.

## 1 Current system and implications

Main is version 0.1.5 and has 13 workspace crates: core, storage, auth, git, transport, services, mail, ssh, web, server, workflow, runner and e2e. Actions and the runner are already merged. The architecture already favors one server process, embedded persistence, compiled templates, native Git reads and streamed Git processes. These are useful foundations to preserve.

An inventory of checked-in Rust under `crates/` found 662 files and 201,936 physical lines, including tests, comments and blanks. Of these files, 163 exceed 400 lines. This is a file-size measurement, not a count of oversized classes. Rust files under `src/` alone total 160,244 physical lines, including inline tests.

| Main evidence | Current behavior or concentration | Rewrite consequence |
| --- | --- | --- |
| `docs/ARCHITECTURE.md`, crate manifests | Intended layered architecture; web and SSH can also depend directly on storage and other low-level adapters | Enforce feature boundaries through constructor types and dependency checks |
| `crates/rbit-services/src/lib.rs`, `crates/rbit-web/src/state.rs` | Broad `ServiceContext`, `Services` and application state | Replace global access with narrow feature dependencies |
| `crates/rbit-workflow/src/parse.rs` and `plan.rs` | 2,130 and 1,922 lines | Separate syntax, validation, semantic analysis and plan construction |
| `crates/rbit-web/src/routes/api/actions.rs`, `crates/rbit-storage/src/sqlite/actions_runs.rs` | 1,878 and 1,819 lines | Separate run lifecycle, job claiming, logs, artifacts, policy and queries |
| `crates/rbit-services/src/events.rs`, `actions/triggers.rs` | Bounded broadcast; subscriber lag can drop events, including Actions triggers | Persist required events and reconcile delivery |
| `.cargo/config.toml` | Per-workspace `.cargo-build` intermediates; SQLite QPSG compile flag | Share both build directories across worktrees and retain the database optimization |
| `scripts/dev.mjs` | Local live binary copies, local data and `target/live` output | Preserve Windows-safe executable copies while centralizing compiler output |
| `.github/workflows/ci.yml`, `release.yml` | Existing security controls, cache discipline, native packaging and container provenance | Improve the existing pipeline without losing those controls |
| `docs/performance-followups.md`, `scripts/perf/` | Paired measurement tooling and prior contention experiments | Reuse reproducible workloads; avoid repeating rejected optimizations |

The older `scripts/perf/BASELINE.md` explicitly describes noisy debug timings and an invalid browser run. It is useful fixture history, not the rewrite's performance baseline. Historical status sections in `docs/HANDOFF.md` and rendering reviews must likewise be distinguished from current main behavior.

## 2 Architecture and design rules

### Application microkernel

The kernel owns module startup and shutdown, health registration, cancellation, deadline propagation, resource admission and telemetry integration. It contains no repository, issue, workflow, database-query, HTML or authorization business logic. Configuration parsing belongs to the composition layer; features receive validated configuration subsets.

The normal request path is Axum or SSH adapter to a typed feature API, then a feature-owned persistence or Git port. Calls remain in process. Prefer concrete types and static dispatch on hot paths. Introduce traits at real storage, transport, clock or external-service boundaries; a helper function does not need an interface merely to be modular.

Feature registration is explicit at compile time. No universal service locator, dynamic plugin loader, per-feature network service or new async runtime is required. In-process modularity does not create a security sandbox. Untrusted Actions jobs continue to execute outside the server, with trust-specific runner isolation.

The server remains deployable as one binary with embedded assets and migrations, with Git available at runtime. The runner remains a separate deployable binary. Optional feature startup, routing and background work remain disabled when configured off; default builds retain the feature set.

### Proposed crate responsibilities

The following approximately 20 crates are a starting allocation. Confirm boundaries through a vertical slice and build measurements before freezing names. Modules can be split further within these crates without adding a crate per type.

| Crate | Responsibility |
| --- | --- |
| `rbit-kernel` | Lifecycle, cancellation, bounded admission, health contracts |
| `rbit-contracts` | Shared identifiers, pagination conventions, stable error codes and small event envelopes |
| `rbit-identity` | Accounts, sessions, tokens, recovery, MFA and external sign-in; private crypto submodules |
| `rbit-access` | Organizations, teams, collaborators, permission decisions and pure ref/branch-access policy |
| `rbit-repositories` | Repository lifecycle, browse/read models, edits, transfers, forks, mirrors, releases, LFS and insights |
| `rbit-collaboration` | Shared timeline, issues, pulls, reviews, merge/review requirements and statuses |
| `rbit-notifications` | Inbox, subscriptions, digests, outbound integration coordination and durable delivery |
| `rbit-actions` | Server scheduling, runners, jobs, leases, environments, secrets, caches and artifacts |
| `rbit-workflow` | Pure workflow discovery rules, parsing, expressions, validation and planning |
| `rbit-storage` | SQLite transaction ownership, migrations, sealing, outbox and redb adapters |
| `rbit-git` | Repository objects, native reads, diff/merge/edit algorithms and maintenance |
| `rbit-transport` | Bounded, streamed Git process transport and shared receive protection |
| `rbit-mail` | SMTP/log/disabled delivery and localized text/HTML email rendering |
| `rbit-localization` | Locale resolution, catalogs and presentation-format contracts |
| `rbit-presentation` | Shared Askama components, view contracts and safe content rendering |
| `rbit-web` | Axum routes, HTTP extractors, response negotiation and browser assets |
| `rbit-ssh` | SSH authentication, Git/LFS command protocol and limits |
| `rbit-server` | CLI, configuration, composition, backup/restore and deployment lifecycle |
| `rbit-runner` | Runner registration, execution backends, steps, logs and local cache |
| `rbit-e2e` | Compatibility, upgrade and cross-boundary acceptance harness |

Keep feature domain models and repository-port traits in their owning feature's public contract modules. Storage implements those ports; features do not import concrete storage. Root composition constructs the adapters and injects them. Shared contracts must not become a replacement for the current oversized core. Where a genuine dependency cycle appears, extract only the small stable contract that breaks it; do not introduce a generic framework to hide the cycle.

Dependency rules:

1. Kernel and contracts have no feature dependencies. Pure domain/policy code has no HTTP or persistence dependencies.
2. Feature application modules depend on contracts, explicit policies and narrow ports. Cross-feature dependencies follow a documented directed graph.
3. Storage and Git adapters implement those ports. A single authority owns migrations and transaction boundaries.
4. Web and SSH depend on feature APIs and protocol-specific transport contracts, with no raw SQL or redb access.
5. Only composition can access all concrete modules. Runner/workflow shared types do not depend on server implementation details.

The initial feature graph is explicit: identity and access each depend on shared identity contracts, not on each other's implementations; repositories depend on access policy contracts; collaboration may depend on repository public contracts, while repositories never import collaboration. Put pure ref/branch-access decisions in the access boundary and expose review/status requirements through a narrow gate port injected by composition. Actions depends on access and workflow contracts; notification/integration consumers receive typed feature events. Composition supplies bridges for cross-feature ports rather than creating reverse imports. Keep DTOs used by Git and storage ports feature-owned or in small shared contracts, so implementing a port never forces a feature to import its concrete adapter. The architecture check enforces these edges before the first slice merges.

If runner-protocol churn proves independent of workflow syntax, extract `rbit-runner-protocol` as a further crate. Do the same for repository or collaboration subdomains only when ownership, compilation or testing benefits justify it. Record the measured effect on build time, code size and runtime before accepting a split.

### Organization and the 400 line limit

Rust's equivalents of classes are structs, enums, traits and their implementation blocks. Apply the user's limit to **both authored source files and cumulative logical types**:

- Every authored Rust source file is at most 400 physical lines, counting comments and blanks. Aim for 100 to 250 when that produces a coherent module.
- A type's definition, all inherent implementations and all trait implementations total at most 400 authored lines across files. The checker resolves implementation ownership and counts each source span once; moving a large type between files cannot evade the limit.
- Separate tests into descriptive test modules and keep authored test files under the same file limit. Split by behavior, not numbered chunks.
- Apply the same file ceiling to first-party templates, JavaScript and scripts. Existing immutable SQL migrations, vendored assets, generated catalogs and data fixtures have narrow, documented exemptions; generated business logic is not an exemption.
- Each crate and module documents purpose, allowed dependencies, state ownership, invariants and resource costs. Public commands document authorization, errors and side effects.
- Use names such as `ClaimJob`, `RunnerLease`, `RepositoryAccess` and `AppendLogChunk`; avoid generic `Manager`, `Helper`, `Utils` and historical workstream labels.

Example feature structure:

```text
crates/rbit-actions/src/
  lib.rs
  module.rs
  domain/{run.rs,job.rs,lease.rs,environment.rs}
  application/{queue_run.rs,claim_job.rs,cancel_run.rs,append_log.rs}
  ports/{run_store.rs,runner_channel.rs,artifact_store.rs}
  read_models/{run_summary.rs,job_detail.rs,log_page.rs}
  events/{run_completed.rs}
```

Add an architecture check using Cargo metadata and an AST/span-aware size check. Fail new violations immediately; ported crates must have no grandfathered violations. Require public API documentation and meaningful examples. Keep architecture decisions in ADRs that explain alternatives and consequences.

## 3 Feature preservation and compatibility

Create a feature ledger before replacement begins. Derive it from route registration, OpenAPI, CLI parsing, configuration, schemas, README, Actions documentation and existing tests. Each row records current observable behavior, source/test evidence, destination module, migration status and acceptance evidence. Completion means every row is verified or an explicit behavior change has been agreed.

| Feature family | Required coverage |
| --- | --- |
| Identity | Setup token and initial admin, registration/login, session expiry/revocation, recovery, verified email, passkeys/TOTP/recovery codes, OIDC/GitHub linking, suspension, scoped tokens and account rename redirects |
| Repository and Git | HTTP/SSH smart Git, protocol v2, user/deploy keys, branches/tags, browsing/history/graphs, diffs/blame/compare, Markdown/documents/Mermaid, forks, imports, mirrors, editing and transfers |
| Data and distribution | LFS batch/streaming/locks/shared objects, archives, releases/attachments, language/contribution statistics, maintenance and garbage collection |
| Collaboration | Issues, labels/milestones/assignees, filters/search, personal Issues and Pull Requests dashboards, mentions/references/closing keywords, cross-fork pulls, reviews/requests/threads, base changes, merge/squash/rebase and stale-ref protection |
| Access and social | Organizations, teams, collaborators, stars/stargazers, read/write/admin, branch protection, required statuses, archived read-only behavior on every ingress and access-filtered listings |
| Integrations | API schemas/status codes/pagination, webhook payloads/HMAC/retries, notifications/digests/unsubscribe, email threading and external status checks |
| Actions | Discovery, supported workflow syntax/events/expressions/matrices/dependencies/reuse/schedules, concurrency, runners/backends, leases, permissions, fork safety, logs/masking, caches/artifacts, environments/approval/timers, rerun/cancel and status reporting |
| Operations and UI | CLI/client, environment/config behavior, admin monitoring/config/restart, backups/key material/restore, metrics/health, update privacy, themes, PWA, mobile, accessibility and no-JavaScript flows |

Preserve API paths and machine error codes, persisted IDs and issue numbers, repository layout, Git wire behavior, webhook semantics and documented configuration aliases. Version runner/server contracts and explicitly support a tested upgrade window. Keep stable machine states and audit event codes independent of translated display text.

Use old tests as compatibility specifications after reviewing what they assert. Add missing black-box tests for externally observable guarantees, especially permission parity across HTTP, SSH, REST, browser actions and runner APIs. Do not replace behavior tests with tests that simply mirror new implementation details.

## 4 Persistence and reliable background work

Keep one relational database, one transaction authority and the existing migration history. SQLite remains the source for durable forge state; redb remains for session state and rebuildable derived indexes/caches. Losing a session store may require everyone to sign in again, and is not equivalent to losing an ordinary render cache. Persistent Actions logs, artifacts, secrets and Git/LFS objects retain explicit ownership and retention rules.

Introduce a transactional outbox for required Actions triggers, webhook delivery and notification facts. Write business state and its outbox record in the same SQLite transaction. Workers claim bounded batches, retry with backoff and record idempotency/delivery state. Delivery is at least once, with consumer deduplication; do not promise exactly once. Keep a bounded in-memory broadcast for disposable UI hints, backed by authoritative reads.

Git refs, filesystem objects, SQLite and redb cannot commit atomically together. Use explicit operation records, Git compare-and-swap updates, staged file publication and reconciliation for edits, merges, transfers, imports and CI status transitions. Test crashes at each boundary and make recovery idempotent. A long Git/network operation must not hold a SQLite write transaction.

Retain WAL, foreign keys, statement caching, QPSG and periodic planner maintenance while collecting new workload evidence. Existing experiments in `docs/performance-followups.md` found that split reader/writer pools improved one tail while substantially reducing read throughput at higher concurrency. Reduce redundant statements, request writes and pool acquisitions before reconsidering that design. WAL permits reader/writer overlap but retains a single writer; more connections are not a universal speed improvement. [SQLite WAL reference](https://www.sqlite.org/wal.html).

Specify crash durability as a product contract before changing pragmas. Main currently uses SQLite `synchronous=NORMAL`; changing fsync policy to improve benchmarks can change power-loss guarantees. Security state, user data, outbox records and derived caches need documented durability and recovery behavior. Backup consistency must include the database, Git/LFS/artifact content and original sealing key. Test restoring without the key as an actionable failure, never silently regenerate an unreadable secret store.

## 5 Performance plan and decision gates

### Establish a reproducible main baseline

Build the pinned main commit and candidate with matching toolchain, release profile, assets and fixture snapshots. Keep builds idle during runtime measurements. Record source and executable hashes, CPU/RAM/disk/OS, configuration, compiler flags, dependency lockfiles, fixture refs and raw samples. Measure cold and warm behavior separately, alternate before/after runs, and report distributions across at least six paired rounds after warmup.

The workload matrix includes small and large repositories, a 20,000-line source file, blame, a 150-file diff, refs/history/search, private ACL reads, mixed comments/pushes/webhooks, Actions scheduling and long logs. Exercise anonymous and authenticated users at concurrency 1, 4, 16 and 64, plus controlled overload and cancellation. Browser measurements must use visible, styled pages at fixed desktop/mobile viewports; scripted scroll probes and physical input results remain distinct.

Collect p50/p95/p99 request latency, throughput at fixed concurrency, CPU/request, allocations/bytes copied, query and prepare counts, lock/acquire wait, Git objects/processes, cache hit/coalescing/eviction, HTML/wire bytes, DOM nodes, render/paint/long tasks, startup, idle/peak RSS and output size. For builds collect clean/check/edit/test/release duration, peak compiler RSS and total disk across worktrees.

Six paired rounds establish repeatability, not a sufficient p99 sample size by themselves. Sustained load runs need enough observations to estimate tails and error rates at the declared offered load. Keep local browser lab results separate from field-percentile claims.

Initial optimization goals below are proposed engineering targets, not measured promises. Ratify absolute latency and memory limits after the baseline:

| Area | Proposed acceptance rule |
| --- | --- |
| Feature and security parity | Every ledger row and adversarial contract passes; no waiver to gain speed |
| Routine requests | No unexplained p95/p99 regression over 5 percent; investigate against observed variance |
| Selected expensive paths | Seek at least 20 percent improvement in CPU, allocation, bytes or p95 on the named workload without material regressions elsewhere |
| Lightweight host | No material increase in idle RSS, startup or stripped server size from new abstractions; seek reductions against main |
| Overload | Configured bounds remain effective and memory reaches a stable plateau; unrelated lightweight requests retain service |
| CI and developer builds | Seek 25 percent lower median warm PR critical-path time and lower peak RSS; shared storage cost follows configured lanes rather than worktree count |
| Browser responsiveness | Target LCP at most 2.5 s, INP at most 200 ms and CLS at most 0.1 on the agreed profile; include correct navigation, selection and accessibility |

The browser targets use the current [Core Web Vitals thresholds](https://web.dev/articles/vitals). Field assessment uses the 75th percentile for a device cohort; local laboratory results are engineering evidence rather than a field-compliance claim.

No aggregate score can hide a regression in authentication, private content, large repositories or an individual supported platform. Repeat ambiguous measurements and keep raw evidence with the decision.

### Prioritized opportunities

| Priority and candidate | Mechanism | Required evidence |
| --- | --- | --- |
| P0 bounded CPU/memory work | Shared budgets for password hashing, Git, highlighting, diff, parsing and compression; separate short/long work admission | Cancellation retains the actual worker permit until completion; no password-strength reduction or queue amplification |
| P0 reliable events | Outbox and idempotent workers instead of required broadcast delivery | Crash/replay/lag tests and measured database write cost |
| P0 direct fragment rendering | Replace `crates/rbit-web/src/partial.rs` full-layout rendering and marker cutting with explicit Askama full/fragment entry points sharing component templates | Title/subnav/history/header parity and lower render CPU/allocations |
| P1 request query work | Narrow read models, batch lookups, avoid incidental writes, keyset pagination where compatible | Query counts/plans and mixed contention results; no N+1 or changed pagination contract |
| P1 content cache | Shared immutable rendered documents and same-key work coalescing; main's highlighter already demonstrates this pattern | Less copying/duplicate CPU, byte-accounted memory bound, permission-safe access and invalidation |
| P1 Actions log/read path | Batched metadata, chunk-indexed logs and incremental authorized delivery | Lower filesystem reads and bytes per update, complete log access and reconnect correctness |
| P1 component rendering | Component-sized fragments and view models that avoid building unused page sections | Fewer queries/allocations/HTML bytes with identical full-page behavior |
| P2 Git reads | Reuse validated handles/snapshots, avoid duplicate scans, use existing commit graph support | Staleness, ref-change, rename/delete and external-write tests plus CPU/I/O evidence |
| P2 build profiles | Compare fat/thin LTO, codegen units and supported linker/compiler-cache choices | Server/runner runtime, size, link RSS and total build/storage measurements |

Additional controlled alternatives include page-specific ES modules versus common initialization, cached external CSS versus inline first-paint CSS, narrower syntect syntax/theme loading versus another Rust highlighter, and mimalloc versus the system allocator. Evaluate profile-guided optimization only with a representative training workload. Retain highlighting fidelity, CSP, language/theme coverage and portable release targets; no alternative is an assumed performance win.

Offload expensive Markdown parsing/sanitization under the same bounded CPU policy while keeping trivial work cheap. `render/document_cache.rs` currently copies cached `Arc<str>` output into a `String`; change the view contract to reuse safe shared output and coalesce admitted concurrent misses. Preserve comment/reference rendering's access-dependent behavior rather than reusing document caching indiscriminately. Tokio notes that started blocking tasks cannot be aborted, which makes actual worker-lifetime ownership essential. [Tokio blocking-task reference](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

Caches are bounded by bytes as well as entries. Keys include immutable object identity where possible, renderer/catalog version and every relevant theme/locale/view dimension. Authorization runs before serving private cached output or `304`; cache keys cannot substitute for current permission checks. Mutable refs have explicit invalidation. Session and capability revocation remains synchronous at the authoritative boundary.

## 6 Components and page rendering

Keep Askama for compiled rendering, HTMX for interactions and Tailwind v4 for styling. Share semantic components for buttons, inputs, navigation, badges, empty/error states, tables, dialogs and pagination. Keep business logic out of templates. A typed view model should already contain permission decisions, display data and localized text appropriate to its component.

Compare these rendering options using representative pages:

| Option | Intended use and tradeoff |
| --- | --- |
| Full server-rendered page | Accessible canonical entry and no-JS behavior; baseline for compatibility |
| Dedicated Askama fragment | Update tabs, checks, summaries and lists without querying/rendering unrelated shell state |
| Deferred fragment | Expensive secondary counts or metadata; show stable placeholders and avoid layout shifts |
| CSS containment and existing block rendering | Large source/diff pages while retaining anchors, selection and browser search |
| Windowed logs or code | Bound DOM on very large output, with server pagination, deep-link recovery, copy/search and full download fallback |
| Authorized incremental stream | Logs and live job states using sequence/cursor updates, reconnect and bounded buffers; polling fallback |

Do not assume virtualization removes network or server work: some techniques only reduce layout/paint. Compare HTML size, server CPU, DOM cost and browser response separately. Preserve existing lazy ref pickers, highlighting/cache improvements, partial responses, preloader limits and diff/line behavior when they pass the new baseline.

Evaluate SSE against cursor-based conditional polling for Actions updates. SSE needs connection budgets, proxy/idle behavior, authorization rechecks, backpressure and resume semantics. It should replace repeated work only where evidence supports it; ordinary navigation does not need a persistent connection. Keep full HTTP page access and no-JS forms.

Apply CSP and escaping consistently to full and fragment output. Disable browser/HTMX history persistence for sensitive pages, and audit service-worker caching across logout, permission revocation and user changes. HTMX provides `hx-history="false"` to prevent sensitive snapshots in local storage. [HTMX history documentation](https://htmx.org/attributes/hx-history/).

## 7 Localization from the foundation

Add `Locale`, locale resolution and typed message arguments before porting the first page. Prefer explicit user choice, then account preference, then supported `Accept-Language`, then the instance default. Persist an anonymous choice in a bounded validated cookie. Locale changes update the page language/direction and invalidate incompatible browser fragments/history.

Use stable namespaced message identifiers and feature-owned catalogs. Domain/API errors remain stable machine codes with structured arguments; the presentation layer localizes them. Do not persist already-translated notification text as the only representation: retain the event/code/arguments and render for the recipient at delivery or display time.

Start with a small evaluation of Fluent-compatible Rust catalogs versus generated typed catalog access. Fluent supports grammatical selectors and plural categories beyond English singular/plural. Choose the implementation after measuring binary size, catalog memory, formatting latency, allocation and concurrency behavior; catalogs are loaded once, never parsed per request. [Fluent selector reference](https://projectfluent.org/fluent/guide/selectors.html).

Required scope includes HTML, HTMX fragments, validation/auth errors, dialogs, accessibility labels, mail subjects/text/HTML, notifications, Actions states and human CLI output. Logs/protocol enums, workflow keys, Git syntax and API machine fields remain stable. User content and source code are not automatically translated.

Use complete messages rather than string concatenation. Format dates, relative times, numbers and lists with the selected locale/time zone while retaining UTC storage and machine timestamps. Treat interpolated names as text; translations cannot inject arbitrary HTML. Use safe component composition for links and emphasis.

Set document `lang` and `dir`, use logical CSS spacing/alignment, isolate mixed-direction identifiers with appropriate bidi markup, and retain left-to-right code/log panels. Test keyboard order, CJK wrapping, long labels, plural variants, RTL and 30 to 50 percent text expansion. Start with English plus expansion and RTL pseudo-locales; add real translations only after catalog completeness and review.

CI checks missing/unused keys, argument mismatches, plural/fallback coverage and untranslated first-party UI strings with a reviewed allowlist. Include locale/catalog revision in render caches, ETags, email snapshots and component tests. Verify that a full page and its HTMX fragments resolve the same locale and that locale redirects cannot create open redirects or cache leaks.

## 8 Security and resource isolation

Create a threat model for anonymous clients, malicious repository data, compromised accounts, untrusted fork workflows, runner operators, administrators, external endpoints and build dependencies. Retain main's current hardening while moving it behind explicit contracts.

| Boundary | Required controls and verification |
| --- | --- |
| Identity | Argon2id strength, bounded hash admission, bootstrap race safety, token/session hashing, secure cookie policy, MFA/passkey/OIDC validation and immediate revocation |
| Authorization | Central policy, least-privilege inputs, fresh checks on each ingress and mutation, private listing/cache/ETag behavior, archived-state enforcement |
| Git and filesystem | Validated paths/refs, isolated Git environment/config, subprocess argument boundaries, compare-and-swap refs, symlink/reparse-point and archive traversal protection |
| HTTP and outbound network | Body/decompression/time limits, origin/CSRF checks, URL validation, SSRF and redirect/DNS pinning across hooks/imports/mirrors/sign-in |
| Content | Askama escaping, sanitized Markdown/HTML, safe Mermaid boundary, CSP and protection against unsafe workflow summaries or log markup |
| Stored secrets | Sealed credentials, key access/backup/rotation, redacted structured logging, least exposure and no secrets in artifacts or cache |
| Actions | Parser/graph/expression budgets, scoped short-lived task credentials, fork/environment policy, lease ownership and expiry, immutable run inputs and permissions |
| Runner | Containers for untrusted jobs, restricted mounts/capabilities/network policy as supported, workspace cleanup, safe artifact/cache extraction and process-tree cancellation |
| Operations | Dependency review/advisories, explicit configuration defaults, audit coverage, bounded telemetry cardinality, tested crash and restore behavior |

Host runner mode remains available for trusted workloads with an explicit operator policy; a container with an unrestricted host socket is not an effective isolation boundary. Preserve supported modes while preventing untrusted jobs from being silently routed to a higher-trust backend. Logs are untrusted data. Mask before persistence and again on presentation where appropriate, but do not treat masking as protection against a malicious job that can intentionally transform a secret.

Preserve main's exact fork policy: untrusted fork runs retain read-only credentials and no secrets or variables even after approval; approval does not elevate trust. Retain container capability dropping, no-new-privileges, process limits and restrictions on privileged/host-network/socket access. Keep log byte/line limits, indexed reads, idempotent chunk offsets and restart reconciliation. Translate the historical race and path regressions in `docs/security-audit-2026-10-17/results.md` into mandatory compatibility cases. Review custom WebAuthn/OIDC parsing against protocol corpora and evaluate maintained Rust alternatives only if security, compatibility and footprint improve.

Fuzz the workflow/YAML/expression, Git/ref/path, archive/LFS and protocol parsers. Use targeted property tests for permission lattices, state machines, cursors and idempotency. Add malicious archive, symlink, reconnect, token replay, cancellation and overload scenarios. Use Rust's unsafe prohibition for authored code where supported; review dependencies and any narrow platform exception explicitly.

Budget CPU work, queued requests, body/decompressed bytes, subprocesses, open files, log lines, artifact/cache bytes and retained output independently. Timeouts must stop or account for the underlying work, not just abandon the response. Instrument rejects, waits and cleanup so overload behavior is observable.

## 9 rbit Actions and CI/CD interface overhaul

Preserve the supported workflow language and runner behavior in `docs/actions.md` and current tests; do not claim general GitHub Actions compatibility beyond that contract. Refactor parser, expression evaluator, planner, scheduler, runner protocol, executor, log pipeline and artifact/cache handling as separately testable modules.

Use durable run/job state transitions, bounded scheduling batches and explicit dependency readiness. Claim jobs with atomic leases; renewal, expiration, cancellation and completion must reject stale attempts. Retry infrastructure failures according to policy without silently replaying non-idempotent deployment steps. Tie run inputs to immutable commits and authorized workflow revisions.

### Screen and interaction plan

| Screen | Primary content and behavior |
| --- | --- |
| Repository Actions overview | Compact run rows with workflow, branch, commit, actor, event, status, queue/run duration; search/filter, pagination and clear disabled/empty states |
| Workflow detail | Recent runs, supported dispatch inputs, enable/disable state, schedule and concurrency information |
| Run overview | Clear result/header, commit/event/attempt, job dependency visualization plus accessible list, failed-job summary, approvals, artifacts and rerun/cancel controls |
| Job detail | Job/step status and timings, progressive log view, search, copy, line anchors, follow/pause, reconnect status, raw download and sanitized summaries |
| Runners | Trust labels, backend/platform, online/busy/offline/draining, capacity, current assignment, health and credential rotation/removal |
| Secrets and variables | Scope and environment context, names/metadata without revealing values, safe create/replace/delete and audit feedback |
| Environments | Required reviewers, timers, deployment policy, pending decisions and authorized history |
| Artifacts and caches | Sizes, expiry/retention, integrity, ownership, quota and safe download/delete actions |

Match the platform's typography, spacing, themes and component states. Design desktop and 360 px layouts together. Status needs text/icons as well as color. Every asynchronous view defines loading, empty, queued, partial, error, permission-denied, disconnected and completed states. Preserve focused elements and scroll position during updates; avoid live-region announcements for every log line. Destructive or privileged actions use server-checked permissions and precise feedback.

Prototype the overview, run and job screens against realistic failed/matrix/long-log fixtures before implementing the full redesign. Validate keyboard and screen-reader navigation, mobile job navigation, no-JS page refresh/download paths, RTL, long translations and theme contrast. Component screenshots and browser profiling become acceptance artifacts.

## 10 Repository CI and release overhaul

Main already uses pinned Actions, read-only workflow defaults, PR cancellation, nextest on Linux/Windows, shared CSS artifacts, selective cache writes and native release targets. Release preparation requires successful main CI for the tag; OCI publication includes provenance/SBOM support. Preserve these strengths. Current global single-job compilation and hosted-runner swap provisioning indicate that template compilation memory needs attention.

Proposed pipeline:

1. **Fast validation:** format, dependency graph/400-line/public-doc checks, catalog checks, workflow/action linting and asset/source consistency. Produce readable annotations with exact file locations.
2. **Asset build:** locked npm install, build/test and catalog generation once for a source digest. Share immutable output with compatible jobs; validate digest and never execute untrusted artifacts in a privileged release job.
3. **Rust checks:** Clippy and nextest with bounded parallelism by measured memory, Linux and Windows on PRs, supported minimum Rust and additional packaging/platform coverage on scheduled/main runs. Run doctests explicitly if documentation introduces them.
4. **Security:** Rust advisory/license checks, npm advisory review, dependency/config policy and secret/static checks with expiring justified exceptions. Review newly changed dependencies on PRs; scan advisories on a schedule because unchanged source can become vulnerable.
5. **Compatibility:** Git HTTP/SSH/LFS, API/workflow protocol fixtures, migrations/restore and runner backend tests. A stable aggregate required check must report skipped/failed dependencies correctly.
6. **Performance:** deterministic work-count/size checks on PRs; controlled paired latency/browser/load experiments on trusted scheduled or main runners. Avoid noisy hosted-runner timing failures.
7. **Release candidate:** exact-commit rebuild/package, archive/container smoke tests, checksums, SBOMs and artifact attestations for binaries and images. Verify downloads and documented installation paths for all five existing binary targets.
8. **Promotion:** publish only verified artifacts from the selected revision, use minimal job-scoped credentials, preserve prerelease/latest semantics and validate server/runner version matching. Track rollback artifacts and release evidence.

Separate low-trust PR execution from jobs with publication/signing credentials. Pin external actions and tools with an update policy; use short-lived identity for signing/publication where supported. Keep permissions per job, avoid unsafe context-to-shell interpolation, and never check out untrusted code in a privileged workflow. These boundaries follow [GitHub's secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use).

Cache compatibility includes OS/architecture, toolchain, lockfile, profile, target, relevant flags and trust scope. PR jobs restore approved base caches; only trusted workflows save shared caches. Do not store secrets, runtime data or unbounded workspace artifacts. Measure cache transfer time and hit rate before adding more caches.

Use a dependency-aware test-selection optimization only after validating it against full-workspace runs. Keep full checks on main and an aggregate PR gate so path filtering cannot leave required checks pending or miss shared-contract impact. Retain the required PR runner-image build/start smoke check. Schedule heavier live container conformance and reproducibility checks according to measured cost, while retaining release smoke tests.

Polish workflow names, grouped logs and job summaries: show revision, platforms, validation results, failure annotations, artifacts, security results and relevant timing/cache data. Use the same state vocabulary in rbit's Actions UI. GitHub's own UI cannot be reskinned; the redesign concerns the application UI and the quality of repository workflow output.

## 11 Worktrees and shared build storage

All new worktrees live under the canonical repository's `worktrees/` directory. Use `feature/` public branches. Do not relocate, remove, clean or prune existing worktrees owned by other work without a separate migration operation.

```text
<repository>/
  worktrees/
    rewrite-foundation/
    rewrite-actions/
  .build-cache/
    <compatibility-key>/
      shared/{target,intermediates}/
      lane-1/{target,intermediates}/     # optional bounded parallel capacity
  .worktree-state/
    <worktree-id>/bin/<build-id>/
    <worktree-id>/{data,logs,ports.json}
```

The default is one shared build lane for compatible developer builds. A measured parallel workload may use a small configured lane pool, for example two lanes total, **never a lane or target directory per worktree**. The budget is total bytes/lanes with retention, not a claim that Cargo will deduplicate every source path. Different worktree paths and source revisions can still cause recompilation and retained artifacts.

Add a cross-platform repository helper for create/list/build/run/test/remove/cache-report/cache-prune. Resolve the canonical repository from Git's common directory and registered worktree metadata, validate it, then derive all storage paths. Do not assume the current directory is the primary checkout or that `../target` points to shared storage. Validate branch names, collisions and absolute path containment before creating/moving/deleting anything.

For each build, the helper sets absolute `CARGO_TARGET_DIR` **and** `CARGO_BUILD_BUILD_DIR` (or equivalent explicit Cargo config) to its assigned shared lane. Main's `build.build-dir = "{workspace-root}/.cargo-build"` would otherwise recreate intermediates in every checkout even with a shared target. Cargo documents both controls separately. [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html).

Key lanes by compatible compiler identity, host/target, profile family and relevant compiler flags, including SQLite compilation flags. Cargo retains normal feature/dependency fingerprints; changing source is not a reason to create an unbounded new lane. Preserve `SQLITE_ENABLE_QPSG` through the helper and CI. Test inherited parent Cargo configuration under nested worktrees so values are not accidentally merged or lost.

The helper holds a cross-process lane lease through **build and copying the exact produced executables**. Cargo's internal lock alone does not protect the interval after Cargo exits and another build overwrites a shared executable. Resolve produced executable paths from Cargo JSON output and copy to `.worktree-state/<id>/bin/<build-id>/` before releasing the lease. Run that generation-specific private copy on Windows so the compiler output stays relinkable and the next build never overwrites a still-running private executable. Retain active generations and a small bounded number of recent generations; remove exited ones safely. Tests similarly need the lease through execution when shared output paths can be overwritten.

Keep separate runtime data, configuration secrets and ports per running worktree. Do not copy a live production database by default. Use deterministic seeds or explicit consistent backup copies. The small runtime executable copy is acceptable; compiler outputs and dependency trees are the storage problem. Debug assets and generated CSS stay associated with the correct source checkout. Shared npm download cache is useful; mutable `node_modules` and generated assets must not be linked across divergent worktrees.

Update `scripts/dev.mjs`, performance launchers, docs and CI helpers together so none hard-code `target/live`, a checkout-local `.cargo-build` or a stale binary path. Preserve performance fixture/report storage separately from compiler caches. All writers sharing a lane, including IDE check/Clippy commands and direct Cargo invocations, must participate in its external lease. Directory configuration alone does not enforce that lease. Configure IDE commands through the wrapper, or give analysis a separate fixed lane within the same cap. Document direct-command constraints and verify rust-analyzer does not silently create per-worktree output. Acquire the same lease for direct clean operations.

Ignore `/worktrees/`, `/.build-cache/` and `/.worktree-state/` in the implementation. Add disk reports, a configurable high-water mark and least-recently-used pruning of inactive compatibility groups. Refuse prune/clean with an active lane lease and never let a worktree-local `cargo clean` unexpectedly erase another user's shared active build. Pruning must verify canonical paths stay inside the configured cache root. Do not put worktrees inside a disposable target directory.

Acceptance scenarios: build two different worktrees sequentially with reused dependencies; build/test concurrently without wrong-binary races; run one on Windows while rebuilding another; edit assets independently; create/remove a worktree without touching shared cache or another runtime; verify no nested `target`/`.cargo-build`; change toolchain/profile; interrupt a build and recover leases; reach the disk budget and prune only inactive cache entries.

## 12 Delivery sequence and ownership

Every change is delivered in an atomic commit: one coherent behavior, extraction or
tooling change with its required manifests, lockfile, tests and documentation. Each
commit must build and pass the checks appropriate to its scope. Keep mechanical moves,
behavior changes and unrelated dependency updates separate. Integrate parallel work
only after review and focused validation; fold integration corrections into the owning
unpublished commit so intermediate revisions remain reviewable and usable.

Implementation follows plan review. A ground-up code organization does not require discarding tested algorithms, wire protocols or immutable data history. Build the new workspace in successive vertical slices, retaining an independently runnable main reference. During transition, old-code exceptions are explicit; all new/replaced modules meet the final standards.

| Phase | Deliverables | Exit gate |
| --- | --- | --- |
| 0 Baseline and contracts | Feature ledger, source/fixture hashes, threat model, baseline measurements, architecture ADRs, size/dependency policy and migration contract | Every supported surface mapped; reproducible measurements and critical compatibility tests |
| 1 Worktree and build foundation | Nested worktree helper, bounded shared lanes, exact executable handoff, disk reporting, new host and CI fast checks | Windows/concurrency/storage acceptance cases; no per-worktree compiler trees |
| 2 First vertical slice | Identity/access, SQLite/redb adapters, locale/errors, repository listing, full HTML/HTMX, startup/shutdown/health | Login, ACL, locale, restart, overload and cache isolation pass; build/runtime architecture costs measured |
| 3 Git and repository platform | Browse, transports/SSH, protection, edits, forks/imports/mirrors, LFS/releases/insights | Real Git clients and cross-ingress policy parity; recovery and performance gates |
| 4 Collaboration and integrations | Timeline/issues/pulls/reviews/statuses, outbox, notifications/mail/hooks/API | Ledger/API parity, stale-ref and event replay/crash tests |
| 5 Actions engine and runner | Language/planner modules, scheduler/leases, backends, logs/artifacts/cache/secrets/environments | Workflow corpus, malicious inputs, protocol upgrade and execution/cleanup parity |
| 6 Actions interface and release pipeline | Screen redesign, incremental updates, localization/accessibility, hardened fast CI and signed packaging | Realistic browser fixtures, RTL/mobile/no-JS, measured update/build savings and release rehearsal |
| 7 Upgrade and rollout | Full feature ledger, sanitized production-size restore, migration/cutover rehearsal, operational docs and rollback package | Parity/security/performance/build budgets pass and recovery drill succeeds |

Architecture, persistence, rendering/localization and CI/runner work can proceed as separate workstreams after shared contracts and the first vertical slice stabilize. Give each workstream owned paths and acceptance fixtures; use a single integrator for shared migrations, public contracts and dependency graph changes. Keep changes reviewable by feature slice. Rebase from GitHub main regularly and update the ledger for changes that land during the rewrite.

Estimate schedule and staffing after phase 0 measures the feature inventory and phase 2 proves the architecture. Reserve explicit time for compatibility, translation review, security testing, platform packaging and migration rehearsals rather than assigning an unsupported completion date now.

## 13 Upgrade cutover and rollback

Preserve existing SQL migrations and checksums; add expand/contract migrations. Keep the new implementation compatible with the current schema initially where possible. Reuse IDs and Git objects instead of exporting/reimporting repository history. Version redb derived entries and rebuild caches when formats change; document session invalidation when necessary.

Rehearse against sanitized copies at representative scale. Verify counts and relationship invariants, private access, Git refs/object integrity, LFS/releases/artifacts, workflow/run history, sealed secrets, search, redirects, backups and runner compatibility. Measure migration duration, free-space requirement and recovery time. A migration must report preflight failures before mutating user data where practical.

Use one production writer at cutover. Drain writes and runner assignments, account for in-flight jobs, take a consistent backup including key material, apply the tested migration, then validate health and key user journeys before reopening writes. Shadow reads use separate data copies and suppress notifications/webhooks/actions; no duplicate external effects.

Rollback before incompatible writes may permit the prior binary against compatible data. After an incompatible schema/data change, restore the tested backup or use a verified reverse migration. A backup restore can lose post-cutover writes; define and communicate the recovery point, not just the recovery time. Keep old binaries, configs and runner versions with the recovery package and reconcile any external jobs or deliveries.

## 14 Decisions to settle through early evidence

The default direction is concrete: static in-process modules, explicit dependencies, approximately 20 crates, 400-line limits, one embedded persistence authority, durable required events, compile-time templates, locale-aware rendering and bounded shared build lanes.

Phase 0 and the first slice must settle the catalog implementation, precise crate splits, absolute latency/RSS/binary budgets, durability policy, supported runner upgrade window and shared-lane concurrency. Rendering experiments choose between existing block containment, windowed views and incremental streaming by workload. Security and feature parity remain mandatory under every option.

The first implementation milestone is a localized, authorized repository-listing slice running through the new host and shared worktree build helper. It proves the architecture's cost and boundaries before the rest of the platform is ported.
