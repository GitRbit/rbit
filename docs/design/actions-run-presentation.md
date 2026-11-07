# Actions run presentation slice

This slice binds the existing prototype hierarchy to the production run document.
It uses the existing Rust/Axum/Askama/HTMX/Tailwind stack and existing theme tokens.
It introduces no query, storage schema, scheduler control or runtime dependency.

The heading and semantic facts list use existing `RunSummary` fields. Real job
names, dependency order, duration and environment links remain the source of the
job list. This does not infer a matrix graph, explain a failed assertion, invent
progress percentages or add a log-search API. Existing trust warnings, approval
and deployment review, cancel/rerun controls, raw sanitized summaries and artifact
ZIP/tar links retain their authorization and route contracts.

Run models and dependency ordering live in `views/actions/run.rs`; typed labels
live in `views/actions/run_text.rs` and `rbit-localization`'s Actions catalog.
The route's used presentation adapter is `routes/actions/runs/run_page.rs`.
Header, review and results templates have separate responsibilities. The initial
extraction's includes expand exactly to the old template source.

Metadata/control labels use the same compiled English/Spanish catalogs as Explore.
Saved account language and browser negotiation are active. Expanded/RTL pseudo
catalogs remain trusted opt-in layout diagnostics for selected feature routes. Request locale negotiation supplies document direction,
Content-Language and Vary dimensions. This is an incremental text migration:
existing status/event/relative-time labels, trust paragraphs and deployment prose
still use their existing English representations. Pseudo output is a diagnostic,
not a translated language. Dynamic title/name/ref values remain escaped Unicode
text, with explicit bidirectional isolation for metadata and commit identifiers.
The selected `#run` root carries its own language and direction, so a locale change
also takes effect when HTMX replaces only that subtree.

## Live refresh ownership

The run retains its existing `#run`, self URL, whole-section HTMX replacement and
three-second polling. No-JavaScript navigation and native POST forms still work.
The enhancement never cancels a poll or suppresses a returned permission update.

Immediately before a successful same-run GET poll swaps, it snapshots only the
approval checkbox and deployment-comment text, dirty state and focused selection.
Both the configured request path and final XHR response URL must match the current
run's exact origin and full deployed pathname. Relative paths are resolved against
the current document; query/fragment differences do not select a different run in
this route. Userinfo, other protocols, missing URLs, boosted navigation and HTMX
redirect/retarget/navigation headers exclude restoration. No URL is logged.
After replacement it restores dirty values only into a newly rendered form with
the same method, action, review kind and environment, and matching enabled fields.
It never recreates a missing form or writes hidden fields, CSRF state or actions.
POST responses, failed/unauthorized responses, navigation, changed environments
and disabled controls do not receive old values. It restores focus only if the
browser is on the body, preserving a user's focus elsewhere. Snapshots are keyed
to the request object in a WeakMap and are not persisted or transmitted.

## Verification and acceptance

DOM fixtures cover dirty text/checkbox/caret retention, removed authorization,
changed environment/action, disabled controls, POST/error/navigation exclusions,
cross-origin/run redirects, subpaths/query normalization, new server defaults and
unrelated focus. Real-router fixtures seed a matrix run,
fork approval and workflow error, assert native route/method/permission contracts,
escaping, locale cache dimensions and full/HTMX document behavior. Existing reader/
writer, environment, artifact and sanitized-summary integration tests remain gates.

The initial preparation was source-only. Compilation, selected integration and
localization tests and strict Clippy must be tracked at the actual integrated source
revision; this presentation contract is not a substitute for those gate results.
Fresh matching-revision CSS/JS builds, isolated Linux visual proof, 360px overflow,
light/dark/RTL keyboard review and human screenshot acceptance remain required.
No screenshot acceptance or production render-speed improvement is claimed here.


This wording was reconciled with public `fb9f21dd`. Machine route/query values and
status keys remain unchanged; translated display labels do not change persisted
protocol/event history. Successful action redirects remain protocol responses.
Generic HTML errors use the shared real-language shell/catalog rules, with legacy
English error text and no successful-poll restoration or locale-refresh claim.
Prototype captures and earlier performance samples remain historical evidence for
their recorded revisions. Current production browser and paired speed acceptance
are separate gates.
