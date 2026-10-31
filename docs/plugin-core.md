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
cargo test --locked -p rbit-services --test integration actions_plugin
cargo test --locked -p rbit-services --no-default-features --test integration actions_unavailable
cargo check --locked -p rbit-server --no-default-features
cargo clippy --locked -p rbit-server -p rbit-services -p rbit-web --no-default-features --all-targets -- -D warnings
node scripts/check-core-dependencies.mjs
```

The broader workspace suite preserves workflow and runner behavior. Lifecycle tests exercise
real minted task credentials, admitted body drains, stale handles, aborted waiters, restart
precedence and interrupted cancellation/check reporting. Web tests cover the same bound
router before and after enable/disable, administrator authorization, CSRF, API documentation,
credential padding, paused stream cancellation and conditional enhancement cleanup.
