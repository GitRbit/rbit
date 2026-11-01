# Plugin core

Actions and localization are optional first-party embedded plugins. Administrators can enable
and disable them at **Admin → Plugins** without restarting Rbit. Localization accepts installable
data-only language packs; see [languages](i18n.md). The control plane lives in `rbit-actions`;
shared forge services live in `rbit-services-base`. The `rbit-services` facade composes them
and publishes the active service through an owned snapshot.

The plugin contracts and controller are reusable for other trusted embedded features.
Independently installed executable packages, sandboxed plugins, general third-party UI
contributions and executable package installation remain later stages of the
[runtime plugin plan](design/plugin-architecture.md).

## Build profiles

The ordinary server includes Actions and i18n support and starts both disabled by default. Build it
normally with `cargo build --release --package rbit-server`. Build a minimal server with:

```sh
cargo build --release --package rbit-server --no-default-features
node scripts/check-core-dependencies.mjs
```

The minimal server excludes `rbit-actions`, `rbit-workflow` and the workflow YAML parser
from its normal dependency graph. It also excludes `rbit-i18n` and the Fluent engine;
built-in English messages remain available. The Plugins page remains available and explains that
Actions is unavailable. Generic commit statuses, branch protection and external CI remain
usable. Existing Actions tables and compatibility value types stay in core storage so old
databases and migration checksums remain valid; they do not activate the control plane.

Build a particular server package when checking this boundary. Building or testing the whole
workspace includes the separately selected Actions and runner packages and unifies their
features. CI validates the minimal package graph separately from the full workspace.

## Runtime controls and persistence

The administrator-only Plugins page shows availability and observed lifecycle state. Its
forms use normal session authorization, Origin checks, security headers and auditing.
Non-administrators cannot change plugin state through the service API either.

`[actions] enabled` is the startup default until an administrator saves explicit intent.
Saved enable or disable intent survives restarts and overrides the startup default. Other
Actions limits, paths and policies remain deployment configuration and require restart.
A build without Actions rejects effective enabled intent, whether it came from the startup
default or persisted state. Explicit persisted disablement overrides an enabled startup
default. Disable Actions in a full build before moving an enabled instance to a core-only
build; saved state remains available when switching distributions.

Lifecycle operations are serialized with bounded admission. A second change receives a
conflict while one is running. Once admitted, the owned host operation continues if its
HTTP client disconnects; shutdown waits for it before stopping shared forge services.
Desired state is recorded and audited before lifecycle work, so a failed activation or
cleanup does not erase what the administrator requested.

## Disable and recovery

Disable removes the active snapshot and closes its generation before waiting. Navigation,
settings entries and OpenAPI reflect availability. Dedicated feature endpoints refuse new
work; task credentials cannot enter Git, LFS or shared API handlers after admission closes.
Old service snapshots also refuse credential authentication.

Admitted requests retain operation leases through their final side effects. Incoming and
outgoing streams observe cancellation. Outgoing streams have an independently driven,
bounded supervisor so a client that stops reading cannot indefinitely retain a plugin
lease. Git completion, archive producers and blocking plugin work retain their own leases
until they stop. Ordinary forge traffic does not enter the Actions scope.

After draining, disable cancels unfinished jobs, tasks and steps, revokes all task token
digests, reports terminal checks and releases the active instance. History, repository
policy, runner registrations, secrets and retained files remain stored. Re-enable creates
a new generation; credentials from cancelled tasks do not regain authority. Work published
while the plugin was absent does not start runs when it returns.

Cleanup deadlines are explicit failures. Surviving work remains tracked, the generation
stays closed, and a fresh activation cannot overlap it. Plugins shows incomplete cleanup
and offers a retry. This is cooperative native disposal, not forced interruption of an
arbitrary native thread or removal of machine code from the running server binary.

Two append-only migrations support recovery. `0060_plugin_settings.sql` stores desired
state and an incomplete-revocation marker. `0061_plugin_revocations.sql` journals affected
run IDs until cancellation and terminal status reporting both succeed. Thus a crash after
a run becomes Cancelled cannot cause its still-pending check report to be forgotten.
Recovery revokes old task authority before publishing a fresh activation. Normal server
shutdown preserves enabled intent and active-run recovery rather than cancelling every
job as an explicit disable would.

Online runners cancel when their task or endpoint is refused. Updated runners additionally
stop local execution after 120 seconds without an accepted state heartbeat, including when
the server is unreachable. Older offline runners may continue locally while disconnected;
server-side credential revocation still takes effect. A runner process that exits after a
disabled endpoint returns `404` may need restarting after re-enable.

## UI and caching

Actions pages and APIs remain registered in builds containing the plugin, under host-owned
admission guards. Disabled paths preserve ordinary unknown-route behavior. Navigation and
OpenAPI are generated from current availability rather than the immutable startup setting.
A feature revision invalidates preloads and restored navigation snapshots. HTML reads that
cross a revision transition refresh instead of caching an inconsistent shell.

Plugin UI contributions register with a scope and generation, update atomically and clear
synchronously on quiesce. Actions and Languages navigation/settings use those registrations.
Each page renders an immutable snapshot through compiled Askama components. The shared
browser lifecycle loads approved enhancements on matching roots and disposes their listeners,
timers, observers and pending work on navigation or observed provider changes. See
[UI extensions](plugin-ui.md) for slots, bounds and the trusted native boundary.

## Contracts and dependency boundaries

`rbit-ui` owns passive presentation primitives and the single task-local locale context
used by host and feature templates. Its `template::Localized<T>` wraps compiled Askama
models in an explicitly captured immutable locale, including independently owned mutation
work; it forwards directly to the host's bounded writer. Web compatibility exports keep
existing template calls on the same context. URL escaping/redirect validation, listing
controls, avatars/form errors and code-row layout are reusable without importing Web,
Services, a database or a translation engine. The all-feature/all-target normal dependency graph
is checked in CI. Page construction, assets, full chrome, authorization and security
middleware stay host-owned; this is the foundation for further Web extraction.

`rbit-ui::Icon::new(name, class)` selects a fixed private SVG shape and borrows only
its escaped class. Unknown names preserve the empty SVG fallback. The Web icon macro
is a direct compatibility delegate; its original leading and trailing line feeds
remain unchanged. Fixed catalog markup is trusted compiled presentation, never
caller-supplied HTML or package authority. Direct nested writes avoid an intermediate
icon String, but renderer calls and parent buffer sizing still require performance
measurement; this is not a latency or total-allocation parity claim.

`rbit-plugin-api` validates bounded dotted identifiers, manifests, requested capabilities
and service version requirements. A capability request is metadata, not authority.
`rbit-plugin-runtime` resolves required providers, rejects cycles and ambiguous versions,
and assigns non-reused generations within the process. Required consumers wait when a
provider is missing and dispose before it is retired.

`PluginScope` owns cooperative cancellation and task/operation leases. Closing and admitting
work use one short critical section, so retained routing or service handles cannot acquire
new authority after closure. Failed or cancelled lifecycle waits preserve cleanup state
for retry. The controller currently supports trusted embedded implementations, not an
untrusted native-code sandbox. Host authorization and the current generation remain
mandatory when exposing an implementation to HTTP.

## Package metadata contracts

`rbit-plugin-api::PackageManifest` introduces the metadata boundary for independently
installed packages. Schema version 1 wraps the existing plugin/service/capability
manifest and declares native code with a target and entrypoint, a Wasm component with
an entrypoint, or data without an entrypoint. A target is a bounded declaration;
the host must separately decide whether it supports that target and executable format.

Each regular payload file declares a portable relative path, byte length and canonical
lowercase SHA256 digest. Paths reject traversal, platform separators, Windows device
names and ambiguous spellings. Validation rejects duplicate paths, conflicting file
ancestors, inconsistent directory casing and absent or empty executable entrypoints.
There may be at most 1,024 files, 256 MiB per file and 1 GiB total. These limits bound
declarations; the installer must also enforce actual streamed and expanded sizes.

Readers must cap the complete encoded manifest at 1 MiB **before** deserialization.
The file-list deserializer additionally limits collection growth. Serde input and
programmatically constructed values both require validation; call `validate()` after
modifying public fields. Unknown fields and unsupported schema versions are rejected.

Requested memory, concurrency, call deadline, storage and event-queue ceilings do not
grant authority or enforce isolation. The future host intersects them with deployment
policy and approved capabilities. In particular, a native process does not become a
sandbox because its manifest requests a memory budget.

This adds no archive extraction, package installation, trust decision or execution.
The future installer must verify actual file contents, reject unsafe archive entries,
establish publisher trust separately, stage immutable versions and admit workers only
through the lifecycle coordinator. Worker framing, host-service ports, persistent
installation state and the optional Wasm host remain required work.

`rbit-plugin-packages::validate_archive` now checks an entire uncompressed package
against its descriptor without extracting files. It caps metadata before parsing,
streams payloads through a fixed 64 KiB heap buffer and verifies every declared size
and SHA256 digest. Unknown, duplicate and missing files fail validation, as do links,
extension records, nonzero padding, truncation and trailing bytes. Diagnostics do not
include arbitrary manifest keys or payload data. Synchronous input readers still need
a host-enforced deadline; a byte limit alone does not prevent a stalled read.

Packages use a deliberately narrow canonical USTAR profile, with `package.json` first,
regular files only and exactly two zero blocks followed by EOF. Generic tar archives,
including extra record padding and common alternate header encodings, may be rejected.
See the [exact format and limits](../crates/rbit-plugin-packages/README.md) before
implementing a packager. Validation establishes agreement with the descriptor, not
publisher identity, trust, installation or permission to execute.

## Custom middleware and other extensions

`ScopedExtensionRegistry<T>` generalizes the registration lifetime to trusted callback
adapters. Retained snapshots require fresh admission; replacement and unpublication revoke
old batches even while the parent plugin remains active. Resource limits include staged
registrations. Callback values retire outside registry and scope cleanup locks.

`rbit-plugin-web` supplies the first request-hook adapter. Trusted composition code accesses
`Services::request_hooks()`, stages validated hooks under a plugin scope and publishes after
readiness. Hooks receive a bounded method and matched-route pattern, can continue with safe
diagnostic headers or return a fixed denial, and run within per-hook and total deadlines, with a shared 128-pipeline concurrency cap.
The host retains authentication, Origin checks, request/response bodies, cache policy and
security headers. Exact recovery and operational route patterns bypass hooks so a broken
policy cannot block sign-in, plugin administration or health probes; their normal host
security checks still run. This is an initial middleware contract; raw Axum layers, private request
context and arbitrary response transformation require additional reviewed interfaces.
See [scoped extension contracts](design/plugin-extensions.md).

## Verification

The focused checks are:

```sh
cargo test --locked -p rbit-plugin-api -p rbit-plugin-runtime -p rbit-plugin-web -p rbit-actions-core
cargo test --locked -p rbit-plugin-packages
cargo test --locked -p rbit-services --test integration actions_plugin
cargo test --locked -p rbit-services --no-default-features --test integration actions_unavailable
cargo check --locked -p rbit-server --no-default-features
cargo clippy --locked -p rbit-server -p rbit-services -p rbit-web --no-default-features --all-targets -- -D warnings
node scripts/check-core-dependencies.mjs
node scripts/check-ui-dependencies.mjs
cargo test --locked -p rbit-ui
```

The broader workspace suite preserves workflow and runner behavior. Lifecycle tests exercise
real minted task credentials, admitted body drains, stale handles, aborted waiters, restart
precedence and interrupted cancellation/check reporting. Web tests cover the same bound
router before and after enable/disable, administrator authorization, CSRF, API documentation,
credential padding, paused stream cancellation and conditional enhancement cleanup.

Package archive parser validation also has a standalone, sanitizer-enabled
[fuzz workspace](../fuzz/README.md), with bounded campaigns and reproducible seeds.
It exercises validation only; it does not install or execute packages.

The host [package installation coordinator and journal](plugin-packages.md) retain
validated archive objects with durable version pins and backup coverage. These
records remain disabled and unreviewed. Administrator HTTP/CLI transport and explicit
Unix root provisioning are available; publisher review/grants, managed workers and
complete recovery remain subsequent stages. Windows HTTP uploads await supported
host root verification.
