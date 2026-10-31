# Scoped plugin extension contracts

This document specifies the common ownership rules for runtime extensions and the
request-hook surface. It complements [the plugin architecture](plugin-architecture.md).
Declarative UI registration, the generic scoped extension registry, metadata-only
request hooks, namespaced compiled native GET pages and typed POST mutations are
implemented. Arbitrary route mounts, richer hook contexts and unrestricted
middleware remain later stages. The [native page guide](../plugin-routes.md)
documents rendering and host security profiles; [native plugin forms](../plugin-mutations.md)
covers session-bound proofs, bounded schemas and owned commit lifetime.

## Ownership and publication

Every effect belongs to a host-assigned plugin ID, activation generation and
`PluginScope`. A service lookup, event consumer, UI contribution, request hook or
route registration must not outlive that scope's admission gate.

The host validates an entire registration before changing shared state. Preparing
an extension reserves its identity but does not make it discoverable. The host
publishes it only after dependency resolution, configuration validation and plugin
startup succeed. IDs must be unique within their registry and replacements must
match the current registration token and generation. An old handle cannot remove,
replace or republish a newer registration.

Closing admission is synchronous: remove discovery entries and cancel owned work
before awaiting audit writes, callback cleanup or draining. Host cleanup effects
are bounded, synchronous registry operations. They must not execute arbitrary
plugin code, acquire a plugin lifecycle mutex or perform I/O. Expensive plugin
cleanup runs separately under the manager's timeout.

Disabling removes visibility immediately while previously admitted operations
drain. A drain timeout keeps the generation closed and reports incomplete cleanup;
it must not reopen authority or report successful unload. Re-enabling creates a
new generation. Desired enablement remains durable and independent of whether
activation succeeded.

The declarative UI registry follows these rules with staged registration,
explicit publication, atomic replacement and synchronous scope cleanup. Its
immutable `Arc` snapshots contain presentation data, not callbacks or operation
leases. Holding a page snapshot during an administrator's disable request cannot
make that request wait for itself. An older response may finish rendering from
its captured data; a fresh snapshot omits the closed generation.

First-party service composition publishes the service before its UI. This permits
a short interval with an available service and no discovery link. UI publication
failure withdraws the service and closes its scope. There is no interval where a
newly published link grants authority to an unavailable service. Route permission
checks remain mandatory regardless of rendered navigation.

The host's UI slots render through compiled Askama templates and fixed Rbit
components. Rendering a contribution does not compile a template, read a plugin
file or query SQL. Existing trusted native plugin pages can also use compiled
Askama views; declarative contributions do not require replacing those pages
with an interpreted template system. Compare measured latency and allocation
costs before claiming parity with existing pages.

`ScopedExtensionRegistry<T>` supplies reusable staged publication, atomic batch
replacement, bounded capacity and generation admission for trusted host adapters.
An `ExtensionEntry` exposes its value only through an admitted `ExtensionLease`;
retained discovery snapshots cannot invoke a revoked batch. Each adapter still
validates its own typed descriptors before staging. Registry validation is not
delegated to arbitrary plugin callbacks.

Callback snapshots hold weak references, while the live batch and admitted leases
own strong callback references. Retaining stale discovery metadata therefore
cannot keep disposed native callbacks or their captured service graphs alive.
Metadata supplied by a host adapter must remain passive data. Compiled native
machine code remains in the binary after resources are released.

Retired callback values can have native destructors. Removal detaches those values
under the registry lock and drops them after releasing it. Scope cleanup similarly
releases its serialization lock before dropping detached values. Destructors that
reenter discovery or quiescing therefore do not deadlock either mutex. Native
destructors remain trusted code and cannot be forcibly preempted by this runtime.

## Host-owned extension points

Each extension family needs an explicit descriptor, validation budget, registry,
typed handle and ownership contract. Do not provide a generic map of arbitrary
callbacks with unrestricted access to the server.

| Extension | Discovery data | Operation lifetime |
| --- | --- | --- |
| Declarative UI | Validated component records, message references and internal targets | Pure data snapshot; no authority lease |
| Service provider | Typed service identity and supported version | Acquire a current-generation lease per call |
| Event consumer | Event kinds, permissions and bounded delivery policy | Scope-owned consumer task and acknowledged delivery |
| Request hook | Declared host stage and metadata requirements | One bounded lease per hook invocation |
| Plugin route | Validated method/path and approved security profile | Lease through handler and streamed response completion |
| Browser enhancement | Approved asset identity, owner and generation | Mount/dispose with host-owned browser resources |

Declarative descriptors accept text and bounded structured values. They do not
accept HTML, scripts, styles, arbitrary classes, expressions or executable module
URLs. The host resolves internal targets using trusted route context and escapes
all text. UI visibility is a server-evaluated predicate, not an authorization grant.

## Request-hook foundation

`rbit-plugin-web` contains typed adapters over the generic registry, including the
bounded Askama native-page bridge. It has no Axum, HTTP body, forge service,
database or Fluent dependency. Shared locale messages do not pull in the optional
catalog engine. `Services` shares
one registry through its trusted `request_hooks()` accessor; the actual HTTP
adapter remains in `rbit-web`.

The implemented invocation context contains only a closed HTTP method enum and
the router's matched route pattern. Raw paths can carry credentials, including
the one-click unsubscribe token, so the adapter never exposes the raw URI, query
string, captures, headers, cookies, session or body. Unmatched requests use a
fixed sentinel. The route pattern is capped at 2,048 bytes.

At most 16 staged and active hooks fit in the registry. Each declares a stable
ID, ordering and a deadline from 1 to 100 ms; the complete pipeline has a 500 ms
budget. A shared admission pool caps concurrently executing pipelines at 128 across
registry clones and snapshots. Overload returns 503 immediately, without queuing or
constructing a callback; empty pipelines and reserved recovery routes do not use the pool.
Ordering is deterministic by order, plugin identity, hook identity and
generation. Invocation requires exact-generation admission, and both scope
cancellation and deadline expiration drop the callback future before releasing
its operation lease. Cancelling the request also drops the invocation future.
Native callbacks must keep work inside that future and remain cooperative;
this interface cannot forcibly stop blocking code or discover untracked tasks.

The first result type is `Continue` with validated diagnostic annotations or
`Reject` with a fixed host-owned reason. Rejection statuses are limited to 403,
429 and 503, use fixed host text and `no-store`, and do not accept plugin-supplied
HTML or authentication challenges. Deadline, cancellation, stale admission and
aggregate-budget failures produce 503.

Annotations must use lowercase `x-rbit-plugin-` names, at most 64 bytes each, with
at most 256 printable ASCII bytes per value. Each hook contributes at most four
annotations and the response at most 16. Conflicting annotation names fail the
pipeline. Existing host response headers win. Plugins cannot replace bodies,
status codes, redirects, security headers, cookies, cache policy or framing.
Completed annotations are immutable request-result data. The host may apply them
after the originating scope closes; no callback or plugin authority remains in
that data. A metadata hook's lease ends with its invocation rather than being
retained through the host handler or response body.

### Native hook example

The plugin's startup code receives its host-assigned scope. A typed callback can
annotate an approved route without receiving its captures or request contents:

```rust
use std::{sync::Arc, time::Duration};
use rbit_plugin_runtime::PluginScope;
use rbit_services::{Services, request_hooks::{
    HookDecision, HookFuture, HookRegistration, HookRegistrationError,
    RequestHook, RequestHookRegistration, RequestMetadata, ResponseAnnotation,
}};

#[derive(Debug)]
struct ExampleHook;

impl RequestHook for ExampleHook {
    fn call<'a>(&'a self, request: &'a RequestMetadata) -> HookFuture<'a> {
        Box::pin(async move {
            let annotations = if request.route() == "/explore" {
                vec![ResponseAnnotation::new("x-rbit-plugin-example", "active")
                    .expect("static annotation is valid")]
            } else {
                Vec::new()
            };
            HookDecision::Continue(annotations)
        })
    }
}

fn stage_hook(
    services: &Services,
    scope: &PluginScope,
) -> Result<RequestHookRegistration, HookRegistrationError> {
    services.request_hooks().stage(
        "example.audit".parse().expect("static plugin ID is valid"),
        scope,
        scope.generation(),
        vec![HookRegistration::new(
            "example-audit", 0, Duration::from_millis(10), Arc::new(ExampleHook),
        )?],
    )
}
```

The host retains the returned registration and calls `publish()` only after the
plugin is ready. `replace()` validates and publishes a complete replacement batch;
old snapshot handles immediately refuse new admission. `unpublish()` removes this
registration, and closing the owning scope also removes it synchronously. The
host drains already admitted work through the plugin manager before reporting
successful unload. Dropping the registration handle alone does not unload its
scope-owned effect.

The host reserves plugin administration, administrator restart/status and monitoring,
sign-in/MFA/OAuth/password recovery, the exact account routes needed for forced
password changes, embedded assets and operational health/readiness/metrics routes
from hooks. This uses compiled matched-route patterns,
not raw prefix matching or a blanket administrator exemption. A deny-all or timing-out hook
cannot trap administrators outside plugin recovery or trigger a health-probe restart loop.
Reserved routes still run their ordinary administrator, authentication, token, Origin and
body/security guards. Other administrator pages remain eligible for hooks.

The upstream monitoring and restart controls retain their existing English prose
and browser status labels. Their host chrome still uses the selected locale, and
preexisting translated configuration, form and recovery text is preserved. This
page-registry phase does not migrate that inherited English presentation into new
catalog entries or change bundled language packs.

The following subsections specify how this foundation may expand. Host-resolved
principals, repository permission context, capability grants, localized rejection
messages and dynamic route descriptors are not exposed by the initial hook API.

The first middleware surface should be a typed host hook, rather than an arbitrary
Axum layer. Native code is trusted process code; its registry contract reduces
accidental authority and cleanup errors but does not sandbox hostile native code.
Third-party execution still requires the isolated runtime and capability design
in the main architecture.

### Registration and future metadata expansion

A descriptor declares a stable hook ID, supported host API version, an approved
stage, deterministic ordering and its metadata requirements. The host verifies
that its manifest grants the required capability. Registry registration is staged,
published and removed through the same generation ownership rules as UI.

Stages should name the information already established by the host. For example,
an authenticated-request stage can expose the principal and normalized route
classification after session resolution. A repository-aware stage must run only
after the host resolves the repository and viewer permissions. Do not infer
permissions from raw URL strings or let a plugin choose an earlier privileged
position in the middleware stack.

The initial metadata structure should contain only the HTTP method, approved
route identity, bounded request ID and optional host-resolved principal/context
identifiers. Add each field deliberately. Raw cookies, authorization headers,
session tokens, query strings, request bodies, filesystem paths and database
handles are excluded. Hooks that need private metadata require an additional
reviewed capability; subscription is not consent to expose the whole request.

### Decisions and response effects

An initial hook can return `Continue` or a host-rendered rejection. Rejections
carry a message reference and one of a narrow set of approved statuses, such as
403, 429 or 503. They cannot introduce HTML, redirects, authentication challenges
or response bodies interpreted as executable content. Hook failures and timeouts
use an explicit fail-closed policy for protected requests; optional telemetry
hooks need a separate declared best-effort policy.

Response annotations, if included, should use an explicit bounded allowlist of
namespaced diagnostic headers. Plugins cannot change `Set-Cookie`, authorization,
Origin/CORS, CSP, other security headers, redirects, cache policy, content type,
content length or transfer framing. The host validates header names, values,
count and total bytes before applying the complete annotation set. CR/LF and
conflicting duplicates are rejected.

The host still owns CSRF, authentication, authorization, rate limits, body caps,
security headers and response cache policy. Hook effects cannot remove or bypass
those checks. Expanding into route rewriting, body transformation, streaming
filters or arbitrary response mutation requires a separate contract and threat
review.

### Invocation and cancellation

Capture an ordered hook list without holding the registry lock while executing
callbacks. Immediately before invocation, acquire the exact generation's
operation lease. A retained handle for a closed generation must reject invocation.
The hook receives a child cancellation token, a bounded immutable context and a
host-enforced deadline. Run hooks sequentially initially so rejection precedence
is deterministic; any later parallel model needs explicit combination semantics.

Cancellation or a timeout must not detach plugin work. If a callback owns work
after its caller disappears, that work must retain its lease and remain visible to
draining until it actually stops. A timeout may end the request while cleanup
continues, but successful unload cannot be reported while that task is alive.
Background work must use the scope's tracked task facility. Callbacks must not
create untracked tasks or retain request authority for later use.

If a future extension wraps a response body, its lease must survive until the body
finishes, errors or is dropped. A lease covering only handler construction does
not protect a live stream. Metadata-only hooks that finish before handler dispatch
release their lease when the hook invocation completes.

### Budgets and performance

Specify limits for installed owners, registrations per owner, metadata bytes,
header annotations, timeout and concurrent hook work. Use immutable cached
discovery snapshots and avoid per-request registry allocation, filesystem reads,
catalog parsing or SQL queries. Replacing a registration validates the complete
candidate before publishing one new snapshot.

Benchmark requests with no installed hooks, installed but disabled hooks, one
metadata hook and the maximum supported active set. Measure latency percentiles,
allocation counts and throughput; do not describe an average as p95. A single
hook's deadline is not enough: cap the complete request-hook budget as well.

## Validation requirements

Registration tests cover malformed descriptors, unsupported capabilities,
duplicate IDs, capacity limits, invalid replacement rollback and stable ordering.
Lifecycle tests cover invisible preparation, failed startup, publication,
synchronous discovery removal before drain, retained stale handles, old-handle
replacement attacks, repeated activation and unload timeout/retry.

Request-hook tests additionally cover principal/context redaction, permission-stage
ordering, protected header rejection, malicious message escaping, rejection
precedence, per-hook and total deadlines, caller abort, hook cancellation and
cleanup with work still in flight. Streamed route tests exercise completion,
error and dropped bodies. Test disabled or absent providers in core-only,
Actions-only, i18n-only and combined distributions.

Browser checks cover removing obsolete DOM contributions and disposing listeners,
observers, timers and requests on generation changes and HTMX replacement. Browser
history and preload keys include a host-boot-unique plugin revision. Cached
presentation data never authorizes server operations.
