# Plugin core

The plugin core provides validated manifests, dependency resolution and an in-memory
lifecycle controller for trusted embedded Rust plugins. Actions configuration and workflow
permission policy now live in `rbit-actions-core`, with compatibility reexports from
`rbit-core`. Actions background tasks use the same generation scope primitives.

This is the first implementation slice of the [runtime plugin plan](design/plugin-architecture.md).
The server still constructs Actions at startup. Administrator runtime controls, dynamic
route/UI publication, persistent plugin state, executable package installation, i18n and
language packs remain subsequent work. Installing an embedded implementation in the
controller is distinct from installing an archive into the running forge.

## Contracts

`rbit-plugin-api` has no HTTP, database or async runtime dependencies. Plugin, service and
capability identifiers use bounded dotted lowercase names such as `rbit.actions`. Serde
deserialization validates identifiers and manifests, including duplicate declarations,
self-dependencies and bounded version metadata. Trusted code constructing a manifest
directly must validate it at registry ingress; `PluginManager::install` does this.

Manifests declare the required host API version, required services, provided services and
requested capabilities. Capability requests are metadata and do not grant authority.
Required service versions use SemVer matching. A missing provider makes an enabled plugin
wait; ambiguous providers, incompatible versions and cycles reject the proposed graph.

The lifecycle controller accepts one installed implementation per plugin ID, with a maximum
of 128 installed plugins. It snapshots the manifest on installation so later interior
mutation cannot change the admitted dependency graph. Installation inspects metadata and
does not call the plugin's `start` callback.

## Embedded lifecycle

Implement `rbit_plugin_runtime::Plugin` with a manifest and owned asynchronous `start`
callback. Optional `stop` cleanup must be safe to retry after a reported error. Register
background work through the `PluginScope` passed to `start`; use its supplied cancellation
token to stop cooperatively. Untracked native side effects are outside the runtime's
cleanup guarantees.

`PluginManager` serializes operations through mutable access. Enabling a provider activates
waiting consumers in deterministic dependency order. Disabling a provider closes admission
for it and its affected consumers before cleanup, then retires consumers before providers.
Consumers retain enabled intent and can restart when the provider returns. Explicitly
disabled plugins stay disabled. Reload creates fresh generations for a provider and its
dependent consumers. Uninstall verifies disposal before removing the implementation.

Startup and stop futures are retained in the controller while waiting. Cancelling the
administrator's operation future must not erase the scope or cleanup state. A failed or
timed-out cleanup retains the generation and prevents an overlapping activation. Call
disable again to retry cleanup; never report a surviving generation as unloaded.

The controller currently manages lifecycle ownership and declared dependency readiness.
It does not yet publish typed service objects, routes, assets, capability grants or durable
desired state. A host integrating those contributions must bind their visibility and
authority to the generation admission gate.

## Admission and resource ownership

`Admission::try_acquire` checks the requested generation and acquires an `OperationLease`.
Closing and acquiring use the same short critical section. Once closure returns, a stale
request cannot acquire a new lease even if it retained an old routing snapshot. Keep the
lease until the final side effect or streamed response body finishes or is dropped.

`PluginScope::spawn` retains a lease for the complete asynchronous task, including when its
join handle is dropped. Quiescing permanently closes admission and signals cancellation.
Disposal waits for all leases with a deadline. On timeout, surviving work remains tracked
and disposal can be retried. Cancelling the disposal future leaves admission closed.

These primitives do not revoke repository permissions, force-stop blocking native code or
unload machine code from an embedded binary. Scope cancellation is cooperative. In
particular, a task that captures its own scope can keep it alive: retain the provided
cancellation token instead. Independently terminated workers and capability enforcement
are separate implementation stages.

## Persisted intent and recovery

The host storage layer provides `PluginSettingsStore` and migration `0060` for explicit
plugin enablement intent and pending credential revocation. Recording an enable request
preserves an unfinished disable marker; the host must complete revocation and reconcile
unfinished work before clearing that marker. The embedded controller does not yet wire
these records into runtime administration or startup reconciliation.

## Actions integration

All subscriber, schedule, sweeper, secrets initialization and trigger-spawned background
tasks enter a private scope. `ActionsService::stop_background` closes admission, signals
the existing shutdown token and returns a drain error if work survives the grace period.
The existing server shutdown method logs that failure without changing its public return
type. A disposed instance cannot start background work again; a future activation needs a
new instance.

This shutdown operation stops background work only. It is not a public runtime disable
API: routes, task credentials and active runner jobs still need the broader lifecycle
transaction described in the design before administrator disablement can be exposed.

## Validation

Run the standalone core checks without frontend assets or a forge database:

```sh
cargo test --locked -p rbit-actions-core -p rbit-plugin-api -p rbit-plugin-runtime
cargo clippy --locked -p rbit-actions-core -p rbit-plugin-api -p rbit-plugin-runtime --all-targets -- -D warnings
```

The workspace test suite additionally covers existing Actions behavior and the new
shutdown-before-start and shutdown-after-start regressions. CI runs the standalone crates
in their own job and retains the existing full workspace gates.
