# Native plugin pages with compiled Askama

Runtime page registration lets a trusted embedded plugin publish and remove
compiled Askama pages without rebuilding the application router. It extends the
same generation ownership used by [UI and request hooks](design/plugin-extensions.md).
It does not load templates from uploaded files or provide a sandbox for native
code. Existing Actions and i18n route tables remain compiled host routes in this
phase; they are not silently migrated into the new registry. There is no dynamic
installation of executable plugin code.

## Route ownership

Global pages use `/-/plugins/{plugin}/{path}`; repository pages use
`/{owner}/{repo}/-/plugins/{plugin}/{path}`. The leading `-` cannot be an account
name, so the global mount cannot shadow an existing account or repository. The
repository mount is also separate from core settings, issues, files and Actions
paths. Plugin IDs and page identifiers are validated before registration. Paths
are exact literal suffixes, not regular expressions or route parameters: at most
96 bytes, eight segments and 32 bytes per segment, using lowercase ASCII letters,
digits, hyphens and underscores. This phase supports read-only GET pages and
HEAD; plugin forms, mutation handlers and arbitrary dynamic mounts are outside
the contract.

One host-owned registry is shared across `Services` clones. Trusted composition
code stages descriptors with the plugin ID, the manager-assigned scope and its
generation. Staging is invisible. The host publishes only after startup succeeds.
Replacement validates the complete candidate before changing discovery; stale
handles cannot replace or remove a newer registration.

The route registry revision contributes to the host's plugin revision. Publishing,
replacing or removing routes therefore retires browser shell, history and preload
identities on the same terms as UI registration. A cached link grants no authority.

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

The bounded rendered-page result is an explicit native privilege. Declarative UI
and installable language packs still cannot supply HTML, scripts, CSS or template
source. Only approved native handlers can produce compiled page output. Security
headers and approved asset rules still apply to the resulting HTML.

The registry allows 128 total staged and published pages, at most 32 in one
generation's batch, and 64 concurrent admitted page operations. A descriptor's
deadline is between 1 ms and 1 second. The page title is limited to 160 bytes and
the compiled fragment to 256 KiB. The bridge renders directly into a bounded
writer and ignores Askama's allocation size hint. It also checks cancellation and
the elapsed deadline after synchronous rendering before publishing output.

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
    source = "<section><h1>Plugin status</h1><p>Locale: {{ locale }}</p></section>",
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

Lookup snapshots retain immutable descriptors and weak callback references. Closing
the scope releases registry-owned callback captures once already
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

One reference run on 2026-10-31 used registry source `84857d0`, an Apple M3 Pro,
Rust 1.99.1, release optimization with LTO disabled and 16 code-generation units:

| Measured path | p50 | p95 |
| --- | ---: | ---: |
| Direct compiled Askama fragment | 0.458 μs | 0.500 μs |
| Snapshot, last-of-16 lookup, admission and bounded compiled rendering | 1.875 μs | 1.958 μs |
| Empty snapshot and missing-route lookup | 0.125 μs | 0.166 μs |

These are local samples, not a deployment latency guarantee. Keep the measured
scope and compiler settings with any comparison; the HTTP pipeline and final-body
supervisor are deliberately outside this microbenchmark.
