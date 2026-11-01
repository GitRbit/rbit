# Native plugin pages with compiled Askama

Runtime page registration lets a trusted embedded plugin publish and remove
compiled Askama pages without rebuilding the application router. It extends the
same generation ownership used by [UI and request hooks](design/plugin-extensions.md).
It does not load templates from uploaded files or provide a sandbox for native
code. Languages uses these registries for its picker and package administration;
see [Languages](i18n.md) for active versus installed scope ownership. Actions HTML pages and forms also use scoped native registrations; badges, raw
logs, archive downloads, runner protocol and REST endpoints retain their typed
host transport adapters. There is no dynamic installation
of executable plugin code.

## Route ownership

Global pages use `/-/plugins/{plugin}/{path}`; repository pages use
`/{owner}/{repo}/-/plugins/{plugin}/{path}`. The leading `-` cannot be an account
name, so the global mount cannot shadow an existing account or repository. The
repository mount is also separate from core settings, issues, files and Actions
paths. Plugin IDs and page identifiers are validated before registration. Paths
are exact literal suffixes, not regular expressions or route parameters: at most
96 bytes, eight segments and 32 bytes per segment, using lowercase ASCII letters,
digits, hyphens and underscores. GET pages and HEAD use the read-only renderer
contract. Typed POST handlers and explicit GET form bindings are covered by
[native plugin forms](plugin-mutations.md);
arbitrary dynamic mounts remain outside the contract.

One host-owned registry is shared across `Services` clones. Trusted composition
code stages descriptors with the plugin ID, the manager-assigned scope and its
generation. Staging is invisible. The host publishes only after startup succeeds.
Replacement validates the complete candidate before changing discovery; stale
handles cannot replace or remove a newer registration.

The route registry revision contributes to the host's plugin revision. Publishing,
replacing or removing routes therefore retires browser shell, history and preload
identities on the same terms as UI registration. A cached link grants no authority.

## Built-in startup composition

The official server calls `rbit_web::prepare_plugin_routes(&state).await?` before
serving HTTP. Embedded Web hosts must do the same after constructing `AppState`.
Services supplies shared registries and accepts Web's compiled blueprints through
`bind_i18n_routes(active, management)` and `bind_actions_routes(bundle)` without
depending on Web. Binding is serialized
with the provider controller and attaches to an already-active generation. Repeated
preparation of a clone of the same host is idempotent; a different host cannot silently
replace callbacks already bound to Services.

Languages publishes `rbit.i18n` pages with its active scope. A separate
`rbit.i18n-admin` installed scope retains administrator package repair while the
provider is disabled or failed. Their different owners preserve the registry's
single-batch-per-owner rule. Both scopes close terminally at host shutdown. Actions uses its single active
`rbit.actions` generation for all native pages and mutations; disabling Actions
withdraws its entire route bundle while core lifecycle controls remain available.
Blueprints capture weak host references; they must not create a Services-to-callback
ownership cycle. Existing GET bookmarks redirect to canonical plugin routes where
supported; removed legacy POST handlers do not bypass registry admission or proofs.

## Declared query input and exact resource routes

Use `PageDescriptor::with_query(&["run", "job"])` and `compiled_request_page` when
an exact page needs resource selectors or filters. The factory receives
`PageContext`, authorized `FormActions`, and a validated `PageQuery`; use
`query.get("run")` and perform domain-specific parsing before loading the resource.
The host never forwards a raw URI. Undeclared or duplicate names, malformed
encoding, control characters and invalid UTF-8 are refused. There are at most
16 fields, 2 KiB per value and 8 KiB of decoded keys and values. Resource IDs in
queries are selectors, not permission grants: host repository access and domain
ownership checks still apply on every request.

Actions deliberately uses exact page paths rather than dynamically registering
one route per run: repository suffixes include `runs`, `workflow?workflow=ci.yml`,
`run?run=12`, `job?run=12&job=34`, and `logs?run=12&job=34&step=0`. Workflow names,
filters and resource selectors are encoded as query values. Status views continue
to use bounded HTMX polling; this does not introduce a server-sent event endpoint.
Old HTML GET bookmarks redirect to the canonical namespace. Old POST URLs do not
execute mutations or redirect a submitted body to a new handler.

## Host security remains in charge

Both mounts run through the existing browser-page middleware: session resolution,
forced password-change restrictions, locale selection, error pages, Origin/CSRF
checks, unsafe-request rate limiting, body limits, deadlines, security headers,
partial responses and cache policy. A plugin cannot turn those guards off.

The dispatcher rejects percent-encoded aliases and malformed paths before invoking
a factory. Unsupported methods retain the host's method refusal; HEAD shares the
declared read-only renderer contract and sends no response body. Raw query strings
are not passed to the factory.

Speculative preloads return an empty 204 without invoking plugin code. Page responses
use `Cache-Control: no-store`, and both shells set `hx-history="false"` so HTMX does
not persist native page content in its history cache. The repository tab response
cache also excludes these route namespaces. Back navigation therefore resolves the
current route and permissions rather than restoring an old plugin page snapshot.

The host enforces each route's declared permission profile before calling plugin
code. Repository policy uses the resolved repository and viewer permissions;
the dispatcher never derives permission from a string-shaped URL alone. Missing,
disabled and inaccessible repository resources retain the host's privacy behavior.
Recovery, sign-in, administration, operational and core forge paths remain owned
by the host.

Global audiences are Public, SignedIn or SiteAdmin. Repository audiences are
RepositoryReader or RepositoryAdmin; incompatible audience/scope pairs are
rejected at construction. A repository context is supplied only after the host
successfully resolves read access, including anonymous access to a public
repository. Its administrator flag comes from the same authorized lookup.

Callbacks receive only their approved typed context. Session credentials,
passwords, authorization headers and arbitrary request extensions are not part of
that contract. A more privileged context or route family needs an explicit API
addition. Native plugin code already runs inside the server process and must be
trusted; these interfaces are lifecycle and validation boundaries, not isolation
against hostile Rust code.

## Compiled rendering

Page templates are Rust types derived with Askama. Use an HTML template extension
such as `ext = "html"` so dynamic values receive normal HTML escaping. Template
syntax is checked and compiled with the plugin. Review any `safe` filters,
disabled escaping or manual `Template` implementations: the native trait bound
cannot prove escaping or constrain arbitrary Rust code. Publication does not
compile a template, read a template file or
perform a database query. A request renders through the host's bounded writer;
oversized or failed output never becomes a partially published response.

By default the compiled result is a fragment inside a host-owned shell.
`PageDescriptor::with_document()` opts a trusted compiled page into
`PagePresentation::Document`: its Askama type renders the complete document,
including existing repository, organization, account or administrator chrome.
The host avoids wrapping it in a second shell. This does not grant response header,
cache, authentication or transport control: admission, output bounds, no-store,
security headers and final-body supervision still apply. A document template must
keep proofs and sensitive content out of HTMX history and use native form
navigation where required. Actions templates use `hx-history="false"` and
`hx-boost="false"` around their native page content.

Capture the request's immutable locale in a template wrapper when rendering inside
an independently owned mutation task. Request task-local state is not inherited
by spawned workers; the compiled template must render under the captured locale
rather than accidentally falling back to English.

The bounded rendered-page result is an explicit native privilege. Declarative UI
and installable language packs still cannot supply HTML, scripts, CSS or template
source. Only approved native handlers can produce compiled page output. Security
headers and approved asset rules still apply to the resulting HTML.

The registry allows 128 total staged and published pages, at most 32 in one
generation's batch, and 64 concurrent admitted page operations. A descriptor's
deadline is between 1 ms and 1 second. The page title is limited to 160 bytes and
the compiled output, fragment or document, to 256 KiB by default. The bridge renders directly into a bounded
writer and ignores Askama's allocation size hint. It also checks cancellation and
the elapsed deadline after synchronous rendering before publishing output.

`PageDescriptor::with_body_limit(bytes)` explicitly chooses a positive output
limit up to 2 MiB. A limit above the default 256 KiB also acquires one of eight
large-document permits in the shared GET registry, in addition to an ordinary
page permit. Both remain held through callback rendering, host transforms and
final response consumption, drop or generation cancellation. An unread response
cannot bypass that aggregate capacity bound. Prefer bounded previews and links to
streaming transport for large logs; raising the document limit is an explicit
reviewed resource policy, not a replacement for pagination or truncation.

Output limits constrain response memory; they do not preempt arbitrary native
CPU work. Callbacks must remain cooperative, avoid blocking request executors and
keep asynchronous work tracked. Benchmark the actual compiled rendering path and
complete HTTP pipeline before claiming latency parity with existing pages.

## Host composition example

The public facade is `Services::page_registry()`, with page types reexported from
`rbit_services::plugin_pages`. Pass that trusted registration capability to a
native plugin's startup context; do not expose it as an HTTP administrator API.
The scope comes from the plugin manager, not from a request.

```rust
use std::time::Duration;
use askama::Template;
use rbit_plugin_runtime::PluginScope;
use rbit_services::{Services, plugin_pages::{
    CompiledPage, PageAudience, PageDescriptor, PageError, PageRouteRegistration,
    PageScope, compiled_page,
}};

#[derive(Debug, Template)]
#[template(
    source = "<section><h2>Runtime details</h2><p>Locale: {{ locale }}</p></section>",
    ext = "html"
)]
struct StatusPage {
    locale: String,
}

fn stage_pages(
    services: &Services,
    scope: &PluginScope,
) -> Result<PageRouteRegistration, PageError> {
    let descriptor = PageDescriptor::new(
        "status", PageScope::Global, "status", PageAudience::Public,
        Duration::from_millis(250),
    )?;
    let page = compiled_page(descriptor, |context| async move {
        CompiledPage::new("Plugin status", StatusPage {
            locale: context.locale.lang().to_owned(),
        })
    });
    services.page_registry().stage(
        "example.status".parse().expect("static plugin ID is valid"),
        scope,
        scope.generation(),
        vec![page],
    )
}
```

After successful activation, publishing that registration makes
`/-/plugins/example.status/status` available on the existing router. The factory
must return `Result<CompiledPage<T>, PageError>` for an Askama `Template`; a
factory returning a raw `String` does not satisfy the API. The bridge alone
constructs `PageBody`, whose title and fragment the host renders inside its page
chrome. The callback cannot set response headers or replace the host response.

For a repository page, use `PageScope::Repository` and
`PageAudience::RepositoryReader` or `RepositoryAdmin`. Its factory receives a
host-authorized `PageRepository` containing the repository ID, owner, name and
administrator permission. `PageActor` contains only account ID and administrator
status. Neither is a database handle or session credential.

Use `PageDescriptor::target(&plugin_id)` when linking from a declarative UI
contribution; it produces the approved global path or repository target with
`{owner}` and `{repo}` placeholders. The host fills those placeholders from its
authorized page context. A descriptor never accepts an arbitrary full mount path.

## Disable and in-flight requests

Dispatch first acquires the exact published generation's operation lease. Closing
the scope synchronously removes discovery and prevents new admission before the
host awaits audit or draining. Retaining an earlier route snapshot cannot bypass
that gate.

Lookup snapshots retain immutable descriptors and weak callback references. A shared
owner index is built with each published entry array. Page and mutation lookup selects
the requested plugin's range before comparing scope and path, examining at most its
32 registrations. It does not rebuild an index, allocate a lookup key or admit unrelated
callbacks on the request path. Entries and their index share one immutable `Arc`,
so snapshot clones still require one reference-count increment. Empty and
single-owner registries skip hashing. Taking a fresh registry snapshot still checks
owners for closed scopes; this cleanup cost is separate from indexed route selection.

The native SDK exposes `ExtensionSnapshot::entries()` as a read-only slice and
`owner_entries(&PluginId)` as indexed entries from that same snapshot. The former public
`entries` field is private to keep the entry array and index consistent. Callback leases,
generation checks and revocation are unchanged.

Closing the scope releases registry-owned callback captures once already
admitted leases finish, even if an old route handle is retained. Passive metadata
can remain available without preserving executable callback resource graphs.
This releases runtime resources; compiled native machine code stays in the server
binary.

An admitted request remains tracked until its owned work and response output are
finished or dropped. Cancellation must release its source resources before its
lease. A slow or unpolled client cannot retain plugin authority indefinitely;
the host supervises cancellation independently of client polling. A drain timeout
leaves the generation closed and reports incomplete cleanup instead of reopening
the route or claiming successful unload.

The operation guard crosses host partial-page and compression transforms before
one outer supervisor owns the final response body. A completed but unread response
still consumes one of the 64 page permits. Consuming or dropping the body releases
that permit; generation cancellation drains it without waiting for the peer to
poll. When a request also owns Actions admission, the same supervisor retains both
guards and reacts to cancellation of either generation.

UI navigation can be staged alongside a page route using the same plugin scope.
Publish the handler before its discovery link. On any UI publication failure,
withdraw the handler and close the scope. A brief available-handler/no-link
interval is safe; advertising an unavailable handler is not.

## Host shutdown

The executable calls `Services::prepare_shutdown()` when shutdown or restart is
requested, before waiting for HTTP connections to drain. This synchronously closes
the page and request-hook registries across every clone, cancels admitted callbacks
and response bodies, and withdraws built-in provider entry points. Core services
and stores remain available during HTTP drain. `Services::shutdown()` then finishes
provider and worker disposal before the stores close. Saved enablement intent and
retained plugin data survive an ordinary restart.

Registry `close()` is terminal for the entire host instance. It also fences staged
registrations, so a late startup cannot republish after shutdown starts. A plugin's
normal disable path uses its registration or scope; it must not close the shared
host registry. Closing a registry revokes its callbacks, while the plugin manager
still owns disposal of independent tasks in custom scopes. Connection drain follows
the host's transport policy.

## Validation

The contract suite must cover route IDs, duplicate mounts, invalid replacement
rollback, descriptor budgets, typed permission profiles and bounded Askama output.
HTTP tests use one already-created router while registering, publishing,
replacing, closing and re-enabling a plugin. They verify current-generation
dispatch, no stale authority, preserved core routes, HTML escaping, locale and
security metadata, partial navigation and failure behavior.

Repository tests exercise anonymous visitors, signed-in readers and administrators,
including private repository refusals. Lifecycle tests retain in-flight output
and old handles across disable and ensure cleanup does not depend on browser
polling. Core-only CI runs the runtime registration test so page extensibility
does not accidentally depend on Actions or the optional catalog engine.

## Rendering measurements

A local optimized benchmark compares identical compiled fragment HTML across
1,000 warmed samples. The plugin path includes a fresh registry snapshot, lookup
of the last of 16 pages, admission, factory invocation, deadline setup and bounded
Askama rendering. The empty case measures snapshot plus unsuccessful lookup.

Run the manual benchmark after changes to discovery, admission or rendering:

```sh
cargo test --release --locked -p rbit-plugin-web --test routes_benchmark -- --ignored --nocapture
```

Record the exact revision, release profile, LTO/code-generation settings and
hardware with p50 and p95 values. The test verifies identical HTML rather than
enforcing a hardware-dependent timing threshold. These local warmed measurements
exclude HTTP middleware, authentication, database work, body supervision and
network transfer. They do not establish end-to-end performance parity with
existing pages.

One reference run on 2026-10-31 used registry source `706094d`, an Apple M3 Pro,
Rust 1.99.1, release optimization with LTO disabled and 16 code-generation units:

| Measured path | p50 | p95 |
| --- | ---: | ---: |
| Direct compiled Askama fragment | 0.417 μs | 0.500 μs |
| Snapshot, last-of-16 lookup, admission and bounded compiled rendering | 1.708 μs | 1.792 μs |
| Empty snapshot and missing-route lookup | 0.125 μs | 0.125 μs |

These are local samples, not a deployment latency guarantee. Keep the measured
scope and compiler settings with any comparison; the HTTP pipeline and final-body
supervisor are deliberately outside this microbenchmark.

The separate lookup benchmark measures pages and mutations with 0, 10 and 100
unrelated plugin owners (one route each, plus a lexically last target owner). It
reports hits and missing paths with both fresh and retained snapshots, including
handle destruction but excluding admission and rendering. Each case warms 32
batches, then records 256 batches of 1,000 operations; p50/p95 describe batch-average
nanoseconds per operation. Run both revisions on idle hardware with the same compiler
and release settings; fresh snapshots include the existing closed-scope cleanup scan.

```sh
CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  cargo test --release --locked -p rbit-plugin-web --test lookup_benchmark -- --ignored --nocapture
```

An idle Apple M4 run on 2026-10-31 compared baseline `9fd65b9` with this owner-index
implementation using Rust 1.95.0, release optimization, LTO disabled and 16 code-generation
units. Baseline and candidate binaries ran sequentially in baseline/candidate/candidate/
baseline order. The first pair measured retained-snapshot successful lookups as follows
(nanoseconds per operation, p50 / p95 of batch averages):

| Registry | Unrelated owners | Baseline | Owner index |
| --- | ---: | ---: | ---: |
| Pages | 0 | 18.29 / 18.38 | 23.21 / 24.17 |
| Pages | 10 | 21.54 / 22.96 | 28.21 / 28.38 |
| Pages | 100 | 44.42 / 49.25 | 28.12 / 31.96 |
| Mutations | 0 | 18.33 / 19.67 | 18.83 / 19.17 |
| Mutations | 10 | 22.58 / 22.88 | 28.96 / 29.42 |
| Mutations | 100 | 44.83 / 47.58 | 29.25 / 29.46 |

The repeated pair retained the same tradeoff: indexing adds several nanoseconds at
small owner counts, while 100-owner hit lookup fell from roughly 45 ns to 28–29 ns.
Missing paths at 100 owners fell from roughly 40–41 ns to 23 ns. Fresh snapshots still
cost roughly 0.6–0.7 microseconds at 100 owners because scope cleanup remains linear.
This establishes bounded route selection, not constant-cost snapshot acquisition or
an end-to-end latency result; it does not certify the separate two-percent core-page
performance gate. No production deadline, queue, capacity or retry policy changed.

Full document factories can read `FormActions::appearance()` for the host-validated
theme slug and unread-notification indicator. The host projects these from its
already-extracted page state on both GET and owned mutation responses. No cookie
jar, session credential, notification content or raw header is exposed. Built-in
Actions documents reconstruct their chrome from this snapshot, including pages
with no forms and successful/invalid mutation replies.
