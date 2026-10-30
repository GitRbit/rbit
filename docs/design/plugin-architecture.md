# Rbit runtime plugin architecture implementation plan

Status: proposed implementation plan. Baseline: `main` at `9f07d6f`, inspected on October 30, 2026.

Implementation progress: Actions is now an optional embedded plugin with live administrator
controls, request and stream admission, persisted intent and crash-safe revocation. See
[plugin core](../plugin-core.md) for the delivered behavior and remaining stages. Source
reference links below follow their current crate locations.

Build a runtime extension platform around scoped resources, typed services and atomic publication. Extract Actions first into an optional compiled module, make that module safely activatable and disposable while Rbit is running, then introduce an i18n plugin and installable language packs. Complete the platform with independently installable executable plugins and a sandboxed execution adapter. Runtime loading must eventually work on a previously unknown package without rebuilding or restarting Rbit.

The first extraction is an intermediate milestone. Cargo features alone do not meet the runtime installation requirement. An embedded Rust module can release its live resources, but its machine code stays in the binary. Separately managed plugin processes can release code and memory when terminated. These operations must have distinct descriptions in the administrator interface.

## 1 Product requirements and boundaries

### Required outcomes

- Install, validate, enable, disable, reload, update, unload and uninstall plugins through CLI and administrator UI.
- Keep desired state across server restarts. Show actual state, dependencies, version, resource use, failures and pending operations separately.
- Load services, event consumers, routes, UI contributions, assets and language catalogs at runtime without restarting Git hosting.
- Remove all live contributions and revoke authority when a plugin is disabled. Keep persisted data until a separate purge operation.
- Support dependent plugins that wait for required services and dispose when providers disappear. Optional integrations have independently disposable child scopes.
- Keep the forge useful with every optional plugin disabled, including i18n. English rendering, authentication, repository access and protection continue working.
- Preserve Actions runner compatibility, URL paths, stored history, permissions, status contexts and existing secrets through extraction.
- Allow plugins to extend functionality and UI while keeping host security policy, accessibility and performance enforceable.

### Platform boundary

Retain accounts, authentication, authorization, repositories, Git transports, branch protection, generic commit statuses, service identities, audit records, configuration, backups and plugin administration in the platform. Keep existing collaboration features where they are for the first delivery. Do not combine the Actions extraction with a rewrite of the entire forge.

The runtime kernel owns lifecycle and capability enforcement. It does not own workflow parsing, scheduling, translations or language catalogs. A small message interface with English fallback belongs to the shared UI contract so pages remain renderable when i18n is absent.

Repository-level enablement is a policy applied inside a globally active plugin, not a new runtime instance for every repository. Avoid one process, dependency graph or timer set per repository. Runtime v1 permits one active instance of each plugin ID per server.

### Meaning of each operation

| Operation | Meaning |
| --- | --- |
| Install | Validate and stage immutable package contents; execute no plugin code |
| Enable | Persist enabled intent, resolve dependencies, prepare and publish a live instance |
| Disable | Persist disabled intent; remove contributions, revoke authority, stop resources |
| Unload | Dispose the current live instance; administrative unload also disables automatic reactivation until explicitly loaded again |
| Reload | Replace an instance of the same version with a new generation |
| Update | Stage a new version and perform a compatible generation replacement |
| Uninstall | Disable and remove the installed package; retain feature data and required migration history |
| Purge | Explicitly delete retained plugin data under a separately reviewed deletion plan |

Dependency loss unloads the affected instance but preserves enabled intent. It enters a waiting state and may reactivate when a compatible provider returns. An explicit administrator disable never silently reverses itself.

## 2 Cordis principles adapted to Rbit

Cordis supplies the conceptual model: a plugin receives a context, declares required services, and registers effects whose lifetime belongs to its scope. Disposing the scope removes its effects. Rbit should adopt these concepts with typed Rust interfaces, bounded asynchronous cleanup and persistent administrative state. This is a Rust implementation inspired by Cordis; adding a production Node runtime is unnecessary.

| Concept | Rbit implementation |
| --- | --- |
| Context | `PluginContext` containing only granted host interfaces and a resource scope |
| Service injection | Typed service key, protocol version range and required or optional dependency |
| Disposable effects | Host-owned registration leases for routes, subscriptions, tasks, timers, streams and UI |
| Nested scopes | Child scope for an optional integration or configuration-dependent feature |
| Provider replacement | Stop or rebind affected scopes using a dependency transaction |
| Scoped configuration | Validated revision with an explicit reload plan |

Model the dependency graph and reject cycles, incompatible versions, conflicting providers and duplicate route registrations before activation. Resolve one explicit provider for each required service; do not select an arbitrary last registrant. Changing a required provider disposes and restarts its consumers in dependency order. Optional integrations restart only their child scopes. Provider loss must never lower authorization requirements or turn required checks into success.

## 3 Execution model

Use one logical plugin contract with separate execution adapters.

| Adapter | Initial use | Runtime installation | Trust boundary |
| --- | --- | --- | --- |
| Embedded native Rust | First Actions extraction; first i18n implementation | Only modules compiled into this distribution | Fully trusted server code; no memory or crash isolation |
| Managed native executable | Installable first-party Actions and i18n packages | Yes, using platform-specific packages | Fully trusted executable; process separation is fault containment, not a sandbox |
| Sandboxed Wasm component | Third-party functionality with granted host APIs | Yes | Explicit imports, resource limits and a restricted host API |
| Data package | Language packs | Yes | Parsed and validated data; no executable hooks |

Do not build native shared-library loading into v1. Avoid a Rust ABI contract, dangling library callbacks and unsafe code unloading. Embedded native plugins cannot promise forced reclamation of a stuck thread. Independent native executables run only when explicitly trusted by an administrator; signatures identify publishers but do not make their code safe.

Run Wasm in an optional `rbit-plugin-host` executable rather than making Wasmtime a mandatory server dependency. A minimal Rbit install has no Wasm engine loaded or linked into its main binary. Give each sandboxed plugin an independently terminable worker, with caps on total workers and aggregate memory. The host enforces imports, memory, execution fuel or epochs, deadlines and host-call quotas. No filesystem preopens, raw sockets, environment access or subprocess execution by default.

Actions remains a native control plane and retains the independent `rbit-runner` for builds. Running plugins and running repository build steps are separate trust domains.

The sandboxed adapter is part of the completion scope, not an unspecified future option. The embedded and managed-native milestones establish the interfaces first; they must not be presented as safe execution of arbitrary third-party code.

## 4 Crates and dependency direction

Introduce these crates incrementally as the first extraction needs them:

| Crate | Responsibility |
| --- | --- |
| `rbit-plugin-api` | Plugin IDs, manifests, service keys, typed envelopes, generations, error codes and protocol negotiation; no Axum or SQLx |
| `rbit-plugin-runtime` | Graph resolution, lifecycle transactions, resource scopes, admission leases and immutable snapshots |
| `rbit-plugin-web` | Dynamic route dispatcher, host middleware profiles, UI slots, assets and OpenAPI composition |
| `rbit-ui` | Shared components, semantic tokens, page contracts, English messages and reusable Askama templates |
| `rbit-actions-core` | Actions-specific domain types and configuration |
| `rbit-actions-storage` | Actions stores and future plugin migrations |
| `rbit-actions` | Actions services, scheduler and event handling |
| `rbit-actions-web` | Actions pages, API, runner endpoints, assets and documentation fragments |
| `rbit-plugin-actions` | Embedded registration adapter and managed executable entry point |
| `rbit-i18n` | Catalog parsing, resolution, formatting and immutable locale snapshots |
| `rbit-plugin-i18n` | Plugin registration, locale settings and language-pack management |
| `rbit-plugin-host` | Optional Wasm worker execution and wire adapter |

Keep `rbit-workflow` shared with `rbit-runner`. The server composition root depends on selected plugin implementations; generic services and the runtime never import Actions or i18n implementation crates. `rbit-actions-web` can depend on shared web/UI contracts, but those shared crates cannot depend back on Actions. Move reusable pieces out of `rbit-web` before introducing that dependency.

Use `embedded-actions` and `embedded-i18n` server features for transitional distributions. A core-only build must exclude their implementation dependencies. Feature checks belong at the composition root and adapter modules, rather than scattered across every handler. Managed plugin executables use the same business crates through a different adapter.

## 5 Lifecycle and atomic publication

### Persistent and observed state

Persist desired state, selected package version, configuration revision, dependency bindings and approved capabilities. Track observed state as `installed`, `waiting`, `preparing`, `active`, `quiescing`, `disabled`, `failed` or `restart_required`. Give every activation a unique generation. Use optimistic revisions and a host-wide lifecycle coordinator so concurrent enable, update and disable requests cannot race.

Suggested host tables are `plugin_installations`, `plugin_settings`, `plugin_grants`, `plugin_operations` and `plugin_migrations`. Lifecycle operation records contain progress, actor, failure stage and recovery status. They must not contain credentials or secret configuration values.

### Activation transaction

1. Check package integrity, host/protocol compatibility, configuration, approved capabilities and dependency versions.
2. Reserve route names, service keys and UI IDs. Acquire a migration lease if required.
3. Apply eligible forward migrations through the host migration service. Migrations are durable and cannot be undone by simply abandoning activation.
4. Prepare the instance in a private scope. Routes and services remain unpublished; side-effecting business work is not admitted. Readiness checks must pass.
5. Build one immutable snapshot containing routes, service bindings, UI descriptors, asset references, API documentation and a new revision.
6. Atomically publish the snapshot and admit the new generation. Start subscriptions and business work at the specified event cursor.
7. Record completion. On failure, dispose staged resources, leave prior contributions active where safe, and record any migrations already applied.

Published snapshots prevent a page from observing a new tab with an old route table. Evaluate `ArcSwap` or an equivalent read-mostly structure; choose its read/reclamation strategy through benchmarks rather than introducing a lifecycle mutex on every request.

Capture activation event boundaries through the host event sequencer while coordinating publication. Events committed before admission opens are either part of an explicit replay range or outside this activation; events committed afterward must be available to the consumer even if its worker has not started polling. Test publication and cursor assignment interleavings rather than relying on timestamps.

### Disable and unload transaction

1. Persist disabled intent and close admission for the generation. A request must acquire an operation lease atomically against this gate, even if it holds an older snapshot.
2. Publish a snapshot without its public routes, services, tabs, settings and API fragments. Required consumers enter quiescing in reverse dependency order.
3. Stop triggers and new job claims. Revoke ordinary plugin/task authority. Retain only narrowly scoped teardown authority needed to record cancellation and terminal states.
4. Drain admitted reads and bounded operations. Cancel streams, timers and subscriptions. Cancel or complete jobs under the plugin-specific policy below.
5. Run asynchronous disposers in reverse dependency order. Within a scope, run order-dependent cleanup sequentially; independent cleanup may run concurrently under a bounded deadline.
6. Revoke teardown authority, stop workers, release scopes, remove retained dispatch references and verify resource counters.
7. Report success only after cleanup completes. A timeout is a failure or `restart_required`, never a successful unload.

Embedded plugins must use tracked tasks, cooperative cancellation and bounded blocking work. `spawn_blocking` work cannot be assumed forcibly cancellable: keep the instance quiescing until it ends, or report restart required. For managed workers, terminate the process tree after the grace period and reconcile interrupted writes on recovery.

Reject reactivation of a generation with unresolved resources. No generic promise of transactional rollback for arbitrary side effects is possible. Separate reversible registrations from durable operations and require idempotency for the latter.

### Update and crash recovery

Stage code and perform health checks before cutover. Read-only compatible providers may overlap briefly. Actions schedulers and other writers require an exclusive ownership lease with fencing tokens so only one generation can claim work. Cutover stops old writes before enabling new writes.

Keep the old package for rollback, but allow rollback only when schema and persisted payload compatibility permit it. Use expand-and-contract database changes across releases. Do not auto-downgrade schemas. After a crash, reconcile operation records, incomplete activation, active generations and job leases before publishing contributions. Crash loops use capped backoff and become quarantined after a configurable threshold.

## 6 Host interfaces and capability enforcement

`PluginContext` exposes a resource scope plus narrowly typed host services. Registration handles must remain owned by the scope even if plugin code drops its local handle. Mandatory tracked resources include tasks, blocking jobs, timers, event subscriptions, route handlers, service providers, uploads, downloads, long polls, browser modules and metrics registrations.

The initial SDK should expose operations equivalent to the following conceptual API. Exact Rust signatures follow the phase 0 prototype:

```text
Plugin.prepare(context, validated_config) -> PreparedPlugin
PreparedPlugin.contributions() -> ValidatedContributionSet
PreparedPlugin.activate(admission, event_cursor) -> ActivePlugin
ActivePlugin.quiesce(reason, deadline) -> QuiesceResult
PluginScope.service(key, provider) -> RegistrationLease
PluginScope.subscribe(event_type, delivery, handler) -> RegistrationLease
PluginScope.spawn(name, cancellable_task) -> TaskLease
PluginScope.child(required_services) -> PluginScope
PluginScope.dispose(deadline) -> DisposalReport
```

Consumers receive revocable service handles rather than unchecked long-lived provider pointers. A disposal report enumerates surviving tasks, in-flight calls and failed cleanup, making resource ownership inspectable in tests and administrator diagnostics.

Host APIs include repository metadata and bounded Git reads, commit-status writes, release operations, notification delivery, secrets handles, scoped storage, events, structured logs and audited service identities. Avoid handing third-party plugins `Services`, raw SQL pools, repository filesystem paths, session tokens or a generic HTTP client.

Every call checks the intersection of installation grants, instance policy, repository policy, current user or workload authority, and generation admission. Checking authority only during activation is insufficient. Revocation affects existing handles, not just future lookups. Long operations check cancellation and revalidate authority before committing mutations.

Replace Actions-specific `TaskGrant` imports in core services with generic workload grants. Preserve workflow-specific push restrictions through a registered mandatory policy. If the policy provider disappears, grants issued under it fail closed; the host must never interpret their disappearance as unrestricted permission. Authentication, branch protection and secret storage are host policy and cannot be replaced by a plugin.

Native trusted modules may retain internal SQL access during extraction. Record this as trusted implementation access, not a security capability boundary. Sandboxed plugins receive a host-owned namespace store with quotas and transactions; arbitrary SQL and arbitrary schema migration are unavailable. Cross-plugin access requires an explicit published service.

## 7 Events and durability

Keep the existing broadcast bus for optional observations. Introduce a durable event stream for Actions triggers and plugins that require replay. Delivery is at least once, with event IDs, schema versions, publication timestamps, repository scope, source identity, consumer cursors and idempotency keys.

Write events in the same SQLite transaction as database changes where possible. Git ref updates and SQLite cannot form one atomic transaction: use a durable push-operation journal around ref updates and a reconciler that compares recorded before/after refs. Specify deduplication keys per operation and test crashes on both sides of the boundary. A push journal by itself does not prove exact delivery.

On reload or crash, enabled consumers resume from their acknowledged cursor. During explicit disable, triggering is off: re-enabling establishes a fresh activation boundary and skips events from the disabled interval. Preserve prior committed runs and acknowledge skipped ranges explicitly. Do not accidentally replay months of builds after re-enablement.

Use bounded consumer concurrency, storage retention and dead-letter records. If a cursor falls outside retained history, surface an operational error and a deliberate recovery choice rather than silently inventing a new cursor. Authorization and payload minimization apply to event subscriptions, including private repository events.

## 8 Runtime web and UI extension contract

### Routing and middleware

Keep core routes stable. Add a dynamic dispatcher for registered plugin routes, with host-owned precedence and collision checks. Preserve Actions' existing repository, owner, administrator and runner paths through reserved compatibility mounts. New plugins use an ID namespace by default.

Route descriptors specify method, path parameters, authentication mode, permission class, body cap, timeout, streaming behavior, CSRF profile and response type. The host applies middleware; plugins cannot disable security headers, Origin checks, rate limits or permission checks. Browser pages, bearer APIs and large streaming bodies use different approved profiles. Streamed responses retain a generation lease until the body finishes, errors or is dropped.

Compose OpenAPI from the same active descriptors used for dispatch. Enforce unique operation IDs and schema names. Preserve 404 behavior for absent or unauthorized feature resources. An admitted operation interrupted by quiescing receives the appropriate cancellation or transient failure response.

### UI contributions

Start with named slots for repository tabs, repository settings cards, user and organization settings, administrator panels, navigation items, badges and bounded page panels. Contributions carry stable IDs, ordering, message IDs, icons, routes and declared visibility requirements. Evaluate permissions on the server; hidden UI is not authorization.

Render declarative contributions with Rbit components and semantic tokens. Provide a versioned component schema covering forms, tables, lists, notices, code blocks and pagination. Validate sizes, URLs, nesting, accessible names and form actions. Reject arbitrary HTML, scripts, CSS classes and external asset URLs in sandboxed descriptors. Include keyboard, focus, error-state, empty-state and 375 px layout requirements in the SDK.

Trusted first-party native modules may retain compiled Askama pages and approved enhancement modules. This is an explicitly stronger privilege than declarative UI. Serve their assets by package content hash and load enhancements only on pages using them. Actions' JavaScript currently enters through the global app import; extraction must remove that unconditional load.

For arbitrary third-party interactive interfaces, provide an isolated iframe with `sandbox="allow-scripts"`, without `allow-same-origin`, forms, navigation or popup permissions by default. Communicate through a per-frame `MessageChannel` with bounded schemas, request IDs and explicit grants. Treat messages as untrusted and validate source/channel identity. Do not use an opaque `null` origin as authentication. Add a separate asset delivery and restrictive CSP design before enabling network access. Require explicit approval for any increased iframe privilege.

Never execute arbitrary third-party JavaScript in Rbit's origin. Backend Wasm isolation provides no protection against a script that can read the forge DOM and act as the signed-in user.

### Browser lifetime and caching

Enhancement modules expose `mount(root, context)` and `dispose()`. The host owns AbortControllers, event listeners, observers, timers, in-flight requests and per-page state. Dispose on HTMX replacement and plugin generation changes. JavaScript module code cannot be evicted from a live browser reliably; disable removes behavior and DOM contributions, while a full reload clears retained module state.

Render plugin and locale revisions into page metadata. Compare these during navigation, invalidate preloads and history snapshots on changes, and refresh stale open pages on their next navigation or explicit lightweight invalidation notification. Cached UI can never authorize a mutation. Disabled routes and credentials are refused immediately at the server.

## 9 Actions extraction and runtime behavior

### Extraction map

| Current location | Destination or change |
| --- | --- |
| `rbit-core::actions` and Actions config | `rbit-actions-core`; temporary configuration compatibility adapter |
| `rbit-storage::sqlite::actions_*` | `rbit-actions-storage` with host-managed trusted migration access |
| `rbit-services::actions` | `rbit-actions`; injected host interfaces instead of the full shared context over time |
| `Services::actions` | Plugin service registration and generation-aware lookup |
| Actions imports in statuses, releases, comments, Git and LFS authorization | Generic workload identity, provenance and mandatory grant policy |
| Actions variants in `DomainEvent` | Versioned namespaced event payloads, preserving legacy webhook shapes |
| Actions web routes, views and templates | `rbit-actions-web` with shared UI contracts |
| `has_actions`, Actions tab URLs and settings cards | Registered visibility-aware UI contributions |
| Actions OpenAPI fragments and route tables | Active plugin API contributions |
| Global `actions-log.js` import | Page-scoped trusted enhancement asset |

Keep the existing runner protocol and workflow semantics intact. Avoid changing workflow execution behavior while moving it. Translate existing `[actions] enabled` configuration into initial desired plugin state, retain repository policy settings, and provide an explicit configuration precedence rule: deployment deny overrides runtime administration; runtime desired state overrides the imported startup default. Warn on contradictory settings.

### Disable policy

Default disable stops triggers, dispatches and claims immediately, cancels waiting jobs, signals cancellation for running jobs and writes terminal status updates. Ordinary task tokens are revoked at admission close. Existing authenticated runner control channels may receive cancellation through a narrow teardown channel, without gaining repository read/write or secret access. Recheck the teardown deadline on every request.

Do not require an unresponsive runner to acknowledge cancellation before disabling the server plugin. After the grace period, mark remaining tasks cancelled or lost with a reason, revoke teardown channels and unload. The server cannot guarantee that a disconnected runner has stopped its operating-system process; runner execution must also stop on expired leases or loss of valid task heartbeats. Update and test that runner behavior as part of runtime disable support.

Offer a separate maintenance drain operation that stops new claims and waits for jobs before disable. The UI distinguishes draining from disabled. Force-disable is immediate authority revocation with bounded cleanup, not deletion of data. Any cancelled check still blocks a protected merge unless its required policy is deliberately changed.

Historical run details require the Actions plugin to be active in v1. Disabling retains history but removes its routes. A later history-only mode is a separate explicit feature, not a hidden process running after disable.

### Migration compatibility

Preserve the existing shared migration history, including Actions migrations, byte for byte. Keep compatibility migrations embedded so databases opened by older versions still validate. Fresh minimal databases may retain legacy Actions tables during this transition; record this limited schema overhead honestly.

Record an adoption marker establishing which existing tables are owned by Actions. Future Actions-only migrations use the plugin ledger and run only when the installed plugin is activated or explicitly migrated. Add append-only core migrations for generic service attribution and any existing `source IN ('user', 'actions')` constraints that need generalization. Preserve historical source strings and bot identities in APIs and webhooks.

Initially use the same SQLite file for trusted first-party Actions to preserve foreign keys and transactions. The managed Actions executable must call a host storage service; it must not open the forge database directly. Batch domain operations such as claims, heartbeat updates and run completion so process extraction does not become one IPC call per SQL statement.

## 10 i18n plugin and language packs

### Shared message contract

Introduce typed `MessageRef` values containing namespace, key and named typed arguments. Shared UI components and pages request messages through `Messages`; an English provider is always available. Keep protocol values, API field names, Git refs, stored identifiers and machine-readable error codes stable. Translate human-facing presentation, not persisted enum spellings or workflow context names.

The i18n plugin supplies locale negotiation, formatting and catalog providers. Disabling it atomically selects English, removes locale settings UI, releases catalogs and invalidates localized caches. User locale preferences remain stored for re-enablement. Actions and other functional plugins use English fallback when i18n is absent rather than declaring it as a hard dependency.

Start with an optional embedded i18n adapter for efficient in-process rendering. Deliver the same provider as an installable managed executable later. For the executable path, collect messages needed by the page and its shared components, resolve them in bounded batches, and render the resulting typed view model with Askama. No synchronous IPC per label and no catalog file reads during a request. Benchmark this path before making it the default distribution. If batching exceeds the latency budget, retain the embedded adapter for installations prioritizing rendering speed; expose that deployment choice honestly.

Do not allow a third-party provider to override arbitrary core login, security or administrator text. Core catalog replacement requires a separate trusted language-publisher permission. Untrusted plugins can supply messages only for their own namespaces. Translation is a content trust decision even when the file contains no executable code.

### Catalog format and packaging

Evaluate Fluent through a prototype covering plurals, grammatical variation, interpolation, bidirectional text and the required date/number formatter behavior. Pin the chosen Rust implementation, verify formatter coverage and licensing, and cap parsing and resolution work. The final format decision is a phase gate, not a claim that every Fluent implementation ships every formatter needed by Rbit.

Use a versioned data archive containing `pack.toml`, catalog files, license and publisher metadata. Metadata includes package ID, locale, direction, version, host message-schema range, covered feature namespaces and checksums. The package manager owns installation and integrity; the active i18n plugin owns catalog validation and activation. With i18n disabled, packs may be installed but remain inactive.

Reject executable entries, symlinks, hard links, absolute paths, traversal, duplicate paths, oversized files, excessive expansion, unbounded references and invalid locale tags. Validate message arguments and restrict resource counts, output lengths and reference depth. Catalogs produce text, not trusted HTML. Host code creates links and other markup with escaped arguments.

Resolve messages deterministically: exact locale in the feature namespace, declared parent locale, feature English fallback. The selected provider and approved pack version are explicit; installation order must not silently override core messages. Incomplete language packs can fall back, while the administrator UI reports coverage and compatibility. Plugin removal does not delete separately installed translations for that namespace.

### Locale resolution and formatting

Use explicit account preference, anonymous locale cookie, bounded browser negotiation and instance default, in that order. Store BCP 47 locale preferences independently of plugin configuration. Unsupported locale preferences fall back safely and remain recorded. Set document `lang`, `dir` and response `Content-Language` consistently.

Give each request one immutable locale/catalog snapshot, including plugin pages and fragments. Choose the recipient locale when sending email; background jobs cannot depend on a browser request. Convert visible dates, relative times, counts, validation errors, accessibility labels and browser enhancements. Test zero/one/many forms and long text without concatenating English sentence fragments.

### Cache correctness

Localized HTML identity includes effective locale, relevant catalog revision and UI contribution revision. Partition application caches accordingly. Set `Vary` for the actual request inputs used for negotiation and isolate account/cookie preferences; an internal locale key alone does not tell a browser cache when its cookie changed. Locale changes force a full navigation and clear or version preload, HTMX history and service-worker caches.

Current commit-pinned HTML can only remain immutable when its URL or representation key includes the locale and catalog revision, or when the fragment contains no localized presentation. Otherwise remove immutable caching. Raw Git objects and locale-independent highlighting remain keyed by content. Never multiply large blob caches by locale unnecessarily.

Pack updates stage and validate a new immutable catalog snapshot, then publish it atomically. Retired snapshots remain only for bounded in-flight renders and are subsequently reclaimed. Disabling or uninstalling an active pack immediately changes new render resolution and increments the relevant cache revision.

### Conversion order

1. Shared shell, navigation, forms, accessibility labels and core error pages.
2. Repository browsing, issues, pull requests and settings.
3. Administrator, account and security flows.
4. Actions and other feature namespaces.
5. Email subjects, plain text, HTML and browser-generated text.

Ship pseudolocales for expansion and RTL first, then a reviewed non-English pack to prove end-to-end installation. Linguistic review and translation writing have separate owners and estimates from infrastructure work.

## 11 Package management and administrator experience

Use immutable package versions in `<data_dir>/plugins/packages/<id>/<version>/<digest>` and separate writable data in `<data_dir>/plugins/data/<id>`. Stage uploads outside active paths, validate all archive entries and digest metadata, then rename atomically. Resolve all runtime paths relative to verified package roots. Installation never runs arbitrary package scripts.

Manifest fields cover package kind, ID, version, host API range, platform target, entry point, required/provided services, requested capabilities, declared contributions, resource budgets and schema compatibility. The host revalidates runtime registrations against that manifest. Content hashes ensure integrity; publisher signatures and pinned keys establish a separate trust decision. Support offline installation and a local allowlist first. An online marketplace is outside the initial scope.

Proposed CLI examples:

```sh
rbit plugin inspect ./rbit-actions.rbit-plugin
rbit plugin install ./rbit-actions.rbit-plugin
rbit plugin enable rbit.actions
rbit plugin disable rbit.actions
rbit plugin reload rbit.actions
rbit plugin update rbit.actions --package ./rbit-actions-next.rbit-plugin
rbit plugin uninstall rbit.actions
rbit language install ./fr-FR.rbit-language
```

Commands against a running instance use an authenticated administrator control API. An offline install command may stage files, but may not write live desired state behind the running server's lifecycle coordinator. Keep purge outside ordinary uninstall.

The administrator UI shows dependencies, affected features, requested permissions, current/desired versions and operation progress before applying changes. Ordinary authorized enable/disable is a direct action; data purge and newly requested powerful permissions receive explicit review. All changes are audited. Deployment policy can forbid executable installation or individual plugins regardless of administrator desired state.

## 12 Persistence and backups

Keep essential plugin installation metadata, desired state, grants, schema history and compatible package digests in backups. Back up plugin durable data through host-owned declarations even when the plugin is disabled or its package is absent. Never execute removed plugin code merely to discover backup files.

Classify data as authoritative, derived or intentionally excluded. Preserve Actions' documented treatment of logs, artifacts and caches until a deliberate backup policy change. Keep required secret keys under existing explicit backup rules; metadata containing encrypted values is not sufficient without its key.

Freeze lifecycle publication during the brief backup metadata capture, snapshot SQLite, and capture referenced immutable package/data objects consistently. Define a barrier for separately stored plugin data and refuse to label an incomplete backup complete. Restore validates package availability and compatibility before activation; missing plugins remain installed-but-unavailable with their data preserved. Restore never downloads or executes replacement packages silently.

## 13 Performance and operational gates

Establish measurements before extraction using existing `scripts/perf` fixtures, a release build, fixed hardware and the same dataset. Record warm/cold runs, sample sizes and normal variance. The following are initial acceptance budgets to validate in the prototype, not measured promises.

| Property | Initial gate |
| --- | --- |
| Core pages with plugins disabled | No new plugin subprocess, database query or filesystem read per request; p95 latency regression no more than 2 percent beyond measured noise |
| Dynamic dispatch | Indexed lookup independent of unrelated plugin count; benchmark 0, 10 and 100 installed plugins |
| Enabled UI contributions | No provider IPC or database query for each navigation item; permissions resolved in batches |
| i18n rendering | No per-message IPC or disk I/O; target at most 1 ms p95 added warm server render latency on reference hardware |
| Disabled plugin resources | Zero owned tasks, timers, streams, subscriptions and worker processes after completed disposal |
| Minimal package | Workflow parser and Wasm engine absent from server dependency tree; record compressed binary size and idle RSS |
| Lifecycle churn | 1,000 enable/disable cycles without a positive retained-resource trend; measure allocator steady state separately from live allocations |
| Unload deadline | 30 second default grace, configurable per host policy; explicit failure if an embedded operation cannot stop |

Streaming, upload and CPU-intensive operations have separate budgets. Limit installed/active plugins, concurrent worker calls, event backlog, persistent storage, descriptor counts and UI payload sizes. Admission and queue caps belong to the host. Plugins cannot self-approve larger budgets.

Host-call and storage quotas apply across adapters. Hard memory and CPU limits require the Wasm runtime or supported operating-system worker controls; the embedded trusted adapter cannot enforce those limits against arbitrary native code. Report the guarantees of the selected adapter in plugin diagnostics.

Expose low-cardinality metrics for lifecycle state, activation/disposal duration, owned resources, call latency, timeouts, worker restarts, event lag, quota rejection and catalog fallback. Do not use repository IDs, message arguments or request IDs as metric labels. Bound log volume and redact secrets.

## 14 Delivery phases and acceptance criteria

Effort is engineer-weeks for an experienced Rust contributor including review and validation. Estimates overlap only where interfaces are stable; they are not calendar commitments.

| Phase | Scope | Estimate | Exit criteria |
| --- | --- | --- | --- |
| 0 | Inventory, baseline and design spikes | 1–2 | Coupling map; reproducible performance baseline; dispatch, batching and disposal prototypes; schema adoption strategy |
| 1 | Extract Actions as an optional module with the minimum kernel | 3–5 | Core-only build excludes workflow dependencies; embedded Actions preserves existing behavior; dependency direction checked |
| 2 | Runtime activation and disposal | 3–5 | Actions can enable, disable and reload without server restart; job/token policy proven; tracked resources return to zero |
| 3 | Dynamic UI and durable events | 3–5 | Atomic routes/UI/OpenAPI; page-scoped assets; replay and crash-boundary tests; no dropped-trigger claim without reconciliation evidence |
| 4 | i18n plugin and installable packs | 4–7 | Hot pack install/update/remove; full English fallback; core and Actions conversion; reviewed second locale; RTL and cache correctness |
| 5 | Independently installable native plugins | 4–7 | Core-only server installs Actions and i18n executables live; versioned protocol; batched host storage; worker crash recovery |
| 6 | Sandboxed plugins and SDK | 4–7 | Optional Wasm host; quotas and termination; declarative UI; independent sample plugin installs and unloads live |
| 7 | Release and upgrade hardening | 2–3 | Compatibility matrix, full backup/restore coverage, upgrade rehearsal, security review and all performance gates |

Total planning range: **24–41 engineer-weeks**, plus translation production. The first useful Actions extraction is 3–5 weeks after the baseline work; the complete runtime ecosystem is materially larger than optional Cargo modules. Re-estimate after phases 0, 2 and 5, when the hardest compatibility and process costs are measurable.

### Suggested pull request sequence

| PR | Concrete change | Required validation |
| --- | --- | --- |
| 1 | Baseline fixtures and coupling map | Repeatable release measurements; document variance |
| 2 | Generic workload identity and provenance | Existing task restrictions, statuses, releases, comments, Git and LFS security scenarios |
| 3 | Actions domain/storage extraction and schema adoption | Existing migration checksums and old database upgrades |
| 4 | Actions service/web extraction and shared UI | Full feature parity, runner protocol, default-off behavior, dependency checks |
| 5 | Scope tracker and generation admission | Partial activation failure, recursive cleanup, stale-handle rejection and concurrency races |
| 6 | Runtime controller and Actions shutdown policy | Live disable/re-enable under claims, uploads, polls, disconnected runners and restart |
| 7 | Dynamic routes, UI, assets and API documentation | Collision rules, middleware profiles, HTMX changes and streaming lease release |
| 8 | Durable triggers and push reconciliation | Duplicate delivery, consumer crash, disabled intervals and Git/SQLite crash points |
| 9 | Typed messages and English fallback | Render parity; code/message ID extraction checks |
| 10 | i18n provider, locale preference and pack installer | Catalog limits, pack updates/removal, fallback and worker batching prototype |
| 11 | Template, browser and email conversion | Pseudolocales, RTL, layout, accessibility and locale cache transitions |
| 12 | Package manager and managed worker protocol | Archive fuzzing, integrity, capability review, offline install and version negotiation |
| 13 | Actions and i18n executable adapters | Install on a core-only live server; parity and throughput; process-tree termination |
| 14 | Wasm host and declarative UI SDK | Malicious guest, denied imports, fuel/deadlines, bounded host calls and unload |
| 15 | Administrator experience, distribution and upgrade guide | Backup/restore rehearsal, release matrix and performance gate report |

The i18n batching prototype belongs in phase 0 even though full i18n implementation follows Actions. This prevents late discovery that the planned process boundary cannot meet rendering requirements.

## 15 Verification strategy and CI changes

Retain existing workspace formatting, clippy, Rust, frontend, license and advisory gates. Add focused module profiles: core only, embedded Actions, embedded i18n, both embedded, and a core server with separately installed packages. Run the Wasm worker in its own job so optional engine dependencies do not contaminate minimal-build verification.

Use architectural dependency checks to prevent imports from core/shared crates back into feature implementations. `cargo tree` assertions must inspect actual selected feature graphs, not only manifest text. Validate plugin protocol schemas, generated SDK artifacts, namespaced OpenAPI and message catalogs for drift.

Exercise lifecycle with a controllable test plugin that intentionally fails after each registration, retains handles, blocks work, opens streams and loses dependencies. Test disable against old snapshots, concurrent mutations and queued host calls. Property-test the dependency graph and lifecycle state transitions. Fuzz archive parsing, manifests, wire envelopes, declarative UI and catalog resolution.

Actions tests cover trusted/untrusted fork runs, workflow mutation refusals, secret withholding, revoked task tokens, identity attribution, protected branches, duplicate trigger delivery, cancellation and runner disconnects. Preserve the current disabled feature tests and extend them to assert zero live resources after runtime disposal.

UI checks include no-JavaScript operation, keyboard navigation, focus after reload, 375 px layouts, expansion pseudolocales, RTL, HTMX history/preload and offline cache transitions. Untrusted UI tests attempt script injection, arbitrary URLs, form action substitution, cross-frame spoofing and oversized contribution trees.

Publish protocol compatibility across the current and previous supported host/plugin versions, plus Actions runner compatibility. Build trusted executables for supported Linux, macOS and Windows targets, sign/digest packages, and include their SDK/protocol compatibility metadata in releases. Capability increases on upgrade require new approval; an update cannot inherit undeclared authority.

## 16 Risks and decisions before implementation

| Risk or decision | Planned resolution |
| --- | --- |
| Process boundaries make Actions storage too chatty | Prototype claim, heartbeat, completion and artifact streaming as coarse operations before process extraction |
| i18n IPC adds latency | Page-level message batches and immutable caches; retain an embedded deployment option if the process adapter misses the gate |
| Embedded code cannot forcibly stop | Cooperative lifecycle contract; explicit restart-required state; process adapter for hard termination |
| UI extension privileges are misunderstood | Separate declarative UI, isolated frames and trusted same-origin enhancement permissions |
| Migration adoption breaks SQLx validation | Keep historical files unchanged; append-only transitions and plugin migration ledger |
| Disabled dependency silently changes protection | Stable workload grants, provider generation checks and fail-closed mandatory policy |
| Git event durability is overstated | Journal and reconciliation with fault-injection evidence; no exactly-once promise |
| Package signatures are confused with sandboxing | Separate integrity, publisher trust and runtime capability controls |
| Dynamic updates conflict with immutable caches | Version URLs and snapshots; invalidate locale/plugin history and preload state |

Phase 0 must settle the supported host/plugin version window, reference benchmark hardware, package signature format, catalog implementation, worker transport framing, native worker platform support and deployment defaults. Use length-delimited typed control messages over inherited private pipes or equivalent authenticated local transport; never expose an unauthenticated plugin listener. Streaming data uses bounded channels with backpressure, cancellation and separate size limits.

Default distribution proposal: ship embedded first-party modules during the transition, preserving Actions off-by-default. Also publish a core-only server. Once managed adapters pass parity and performance gates, offer installable first-party packages and keep embedded builds as an explicit deployment profile. New installations should not require the Wasm host unless a sandboxed plugin is enabled.

The final completion demonstration is a core-only Rbit process that keeps serving repository reads and Git traffic while an administrator installs and enables Actions, runs and cancels a workflow, unloads Actions, installs i18n and a language pack, switches locale, unloads i18n back to English, then installs and removes an independently authored sandboxed UI/functionality plugin. Resource counters, authority revocation and performance results must support every transition.

## 17 Source references

Repository references describe the inspected baseline; crate names and interfaces proposed above do not exist yet.

- [Services and Actions wiring](../../crates/rbit-services/src/lib.rs)
- [Current event bus](../../crates/rbit-services-base/src/events.rs)
- [Actions trigger handling](../../crates/rbit-actions/src/actions/triggers.rs)
- [Task grants and workflow push restrictions](../../crates/rbit-actions/src/actions/tokens.rs)
- [Status service coupling](../../crates/rbit-services-base/src/statuses.rs)
- [Web router composition](../../crates/rbit-web/src/routes/mod.rs)
- [Shared page state](../../crates/rbit-web/src/views/layout.rs)
- [HTML cache behavior](../../crates/rbit-web/src/middleware/cache.rs)
- [Global browser enhancements](../../crates/rbit-web/assets/app.js)
- [Storage migration runner](../../crates/rbit-storage/src/sqlite/mod.rs)
- [Backup implementation](../../crates/rbit-server/src/backup.rs)
- [Contribution and security conventions](../../CONTRIBUTING.md)
- [Cordis upstream](https://github.com/cordiverse/cordis) and its [linked lifecycle and context primer](https://deepseek-harness.github.io/deepseek-harness/reference/cordis-primer): conceptual basis for scoped effects and service dependency lifetimes; the proposed Rust API is independent.
- [Cargo features](https://doc.rust-lang.org/cargo/reference/features.html): conditional compilation and optional dependencies for the transitional embedded distribution.
- [ArcSwap documentation](https://docs.rs/arc-swap/latest/arc_swap/): candidate mechanism for immutable runtime snapshots.
- [Wasmtime security](https://docs.wasmtime.dev/security.html) and [execution interruption](https://docs.wasmtime.dev/examples-interrupting-wasm.html): sandbox and resource-control building blocks; host APIs and browser privileges still require separate enforcement.
- [Project Fluent](https://projectfluent.org/): candidate catalog format and localization model.
