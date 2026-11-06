# Collaboration retirement gaps and next active slice

The reference application source is now complete through suggestions, rendering
resolution, cross-reference links, commit effects and merged-pull closures. This is
prepared source, not passing Rust acceptance evidence. The shared thread and catalog
ports have separate validation histories; the broad ledger groups remain
baseline-retained until every behavior and consumer is owned and accepted.

## Suggestion parity and acceptance

ReferenceSuggestions owns the caller's repository-read gate, first 100 Unicode scalar
values, account-vs-thread branch, canonical IssueNumber precedence, whitespace terms,
search limit 10/offset 0 and exact Issue/PR labels. The static production port keeps
account display-name selection, SQL order and indexed numeric lookup. Denied or
unpolled requests do not construct stores. Account suggestions construct no IssueStore.
The thread adapter now borrows only Database. Numeric lookup obtains its IssueStore
when polled; the search branch constructs only IssueSearchStore, removing the original
unused IssueStore and its pool-handle clone. SearchFilter initialization remains
infallible empty-field setup, and the existing word Vec moves into it without copying. No new SQL/count/metadata
read, validation pass, task or dynamic dispatch is introduced.

Prepared policy fixtures observe construction, character cap (not UTF-8 byte cap),
noncanonical numeric fallback, exact labels/order and limit/offset. Real-store fixtures
pin accounts, numeric lookup and empty search to one acquisition/statement, and denied
requests to zero. Source/Node graph and AST bounds pass; Rust tests, HTTP autocomplete,
full reference/privacy and strict Clippy still await the shared lane. SuggestionThreads
has an explicit qualified legacy-component gate entry. No latency claim is made.

## Markdown content boundary assessment

The remaining core bridge is the original pulldown Markdown event projection/options.
It preserves code/link/image exclusions, emphasis joining and raw-HTML handling and
runs the feature-owned Scan. Both core and web already directly depend on pulldown;
collaboration's isolated models currently do not. There is no implemented presentation
crate in this checkout; the plan assigns safe content rendering to rbit-presentation.

Moving this bridge under collaboration's applications feature would require core to
enable application capabilities for its existing public API. Moving it into default
models would widen the model graph with a Markdown engine. Making core depend on a
future Askama/presentation implementation would reverse the intended layering and
risk feature-unified UI dependencies in model consumers. Those are not accepted
ways to remove a 50-line compatibility bridge.

A future substantial shared content boundary can expose a statically dispatched
Markdown text visitor/options contract without importing core or collaboration.
The existing core API can then delegate while web safe rendering uses the same content
policy. It must keep the existing engine/version and no HTTP/Askama dependency on the
core-facing edge; pure collaboration models must remain engine-free. Adopt that split
alongside actual shared safe-content ownership, measured compile/binary impact and
Markdown/security parity tests, rather than adding a crate solely for this wrapper.
Until then the bridge remains explicit and the reference/timeline ledger is not retired.

## Whole-group gaps

| Group | Current remaining authority | Acceptance before retirement |
|---|---|---|
| Issue reads/search | issues.rs search_filter/search_results/list/dashboard, metadata needs and batch hydration; issue_query parser/filter models | Existing indexed SQL, selected-page authors/metadata, clamped HTML vs unclamped API paging, current-viewer/private filtering and concurrent batches |
| Issue deletion | issues.rs delete authorization, audit and Deleted snapshot | Role/archive/missing/race ordering, audit behavior, conditional SQL winner and exact event; remove only proven redundant copies |
| Mentions/background | IssueService listen/handle_event, mention indexing/backfill and outer Git orchestration | Task/drop/drain ownership, historical previous-body indexing, workflow ghost exclusion, bounded backfill and no extra Git/OID resolution |
| Pull lifecycle | pulls.rs and pulls/* cross-fork models/access, compare/count cache, create/base edits and merge | Original full-OID and cached-count paths, stale refs, ref CAS/rollback, fork visibility and merge/squash/rebase parity |
| Reviews/requests/threads | legacy pull review models, storage and application authority | Approval invalidation, draft/private filtering, thread resolution, notification/event ordering and source-stable diffs |
| Status/protection | legacy statuses/check APIs and access/protection adapters | Required-context/approval/head gates across every ingress; no hot-path dependency cycle or extra reads |
| HTTP consumers | routes/templates still consume compatibility aliases | Wire/HTML/HTMX/accessibility/localization/cache behavior; explicit feature API migration and final alias removal |

Prioritize a coherent filtered-thread reader: repository search + API list + dashboard
through shared page hydration, with feature-owned query/filter models and borrowed
static ports. This removes the largest remaining repeated IssueService read authority
while preserving counts, paging distinctions, MetadataNeeds and existing try_join!
work. The dashboard must retain its active-user reload and one readable-repository
batch after selected-page lookup; no filter or identity grant is inferred from stale
contexts. Independent query-policy plus real private-data/query-plan/work-count
fixtures are required before moving consumers.

A bounded complementary improvement is issue deletion: the existing implementation
clones an entire Issue solely to borrow its event identity before boxing the original
Deleted snapshot. A feature-owned deletion workflow can publish copied identity fields
and move the single snapshot without copying title/body, while retaining the one
conditional delete, audit and transient event ordering. That is a concrete allocation
candidate, not a measured speedup or permission/durability change. Keep it separate
from the search port so each atomic behavior and budget is reviewable.

## Prepared filtered-reader follow-up

The prioritized filtered-thread reader is now activated in prepared source through
IssueListing, IssueMetadataReader and borrowed IssueReader. Exact query/filter/result
aliases, original statements, distinct HTML/API paths, active-user reload and selected
page hydration are covered by policy and real-store fixtures. See
[filtered thread read ownership](collaboration-filtered-thread-reads.md). Rust/HTTP
acceptance is pending; this does not retire the broader ledger groups or background
controller. The separate deletion improvement remains next.
