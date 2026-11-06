# Preserve main's performance optimizations

The rewrite starts from GitHub main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`.
Its established optimizations are requirements to preserve through feature migration.
A new crate boundary or renderer is not an improvement by itself. Keep the optimized
behavior or prove a replacement preserves correctness and improves the relevant work
and runtime measurements. Do not remove caching, weaken access checks or change data
durability to make a benchmark faster.

## Initial source audit

At rewrite checkpoint `558d2364`, Git blob comparisons against pinned main found the
following implementation files unchanged. This is source-retention evidence, not a
performance result. Later extractions must carry their behavior and existing tests
into the owning feature; this table does not require keeping legacy file locations.

| Existing optimization | Source retained from main | Migration obligation |
| --- | --- | --- |
| Embedded release assets, shared raw bytes, cached style/CSP hashes, one-time Brotli, content-addressed immutable URLs | `rbit-web/src/assets.rs` | Avoid disk reads, repeated hashing/compression and CSS normalization on warm release requests; preserve validators, encoding negotiation and CSP |
| Conditional HTML, preloaded response freshness, pinned public fragments, borrowed ETag input | `rbit-web/src/middleware/cache.rs` | Preserve private/no-store/cookie safeguards and all request variation; authorize before honoring validators |
| Markdown document cache | `rbit-web/src/render/document_cache.rs` | Preserve byte bounds, source/base/version keys, small-input bypass and sanitization |
| Shared highlighting and cached file widths | `rbit-web/src/render/highlight_cache.rs` | Preserve shared allocations, same-key work sharing, bounded real-worker admission and cancellation ownership |
| Bounded intent preloading and navigation response reuse | `rbit-web/assets/preload.js`, `repo-navigation-cache.js` | Preserve Save-Data, navigation epochs, stale-work cancellation, concurrency/queue limits and same cache keys as actual navigation |
| Diff rendering and efficient scroll handlers | `rbit-web/assets/diff.js`, `contributions.js`, `rbit-web/styles/app.css` | Preserve bounded/delegated work and existing block/placeholder geometry; measure styled scrolling separately from server time |
| SQLite pool, WAL, memory mapping and in-memory temporary work | `rbit-storage/src/sqlite/mod.rs`, `sqlite/pool.rs` | Preserve configuration, pool instrumentation and write ordering; measure contention before replacing the pool topology |
| Reduced SQL work per page and stable indexed plans | `rbit-storage/src/sqlite/repositories.rs`, `issues.rs`, `issue_meta.rs`, `notifications.rs` | Retain combined lookups, no per-row queries, read-mark behavior, immediate transactions where needed and plan/work-count regressions |
| Persistent insight caches and bounded background work | `rbit-storage/src/kv/insights.rs`, `rbit-services/src/insights.rs`, `insights_recent.rs`, `jobs.rs` | Preserve commit/permission dimensions, incomplete-result rules, bounds and coalescing |
| Cached blame | `rbit-services/src/blame.rs` | Preserve identity keys, cache validity, resource limits and access checks |
| Pinned lazy Git reader and direct full-object-ID resolution | `rbit-git/src/reader.rs` | Avoid repeated opens within a read and abbreviated-ID filesystem scans for already resolved IDs |
| Bounded directory metadata traversal | `rbit-git/src/last_commit.rs` | Preserve page-selected work, decoded-tree reuse, budgets and incomplete-result semantics |
| Allocator and release optimization settings | `rbit-server/src/main.rs`; unchanged `[profile.release]` settings | Retain the allocator, fat LTO, one codegen unit, abort-on-panic and stripped release symbols unless matched experiments justify a change |
| Query planner stability at build time | `.cargo/config.toml` | Keep `SQLITE_ENABLE_QPSG` in local, CI and container builds; verify the actual SQLite compile option |
| Real router statement/acquisition budgets | `rbit-web/tests/work_counts.rs` | Keep these tests active while migrating adapters; no silent budget increases |

The shared compiler helper also supplies `-DSQLITE_ENABLE_QPSG` explicitly. Shared
output directories must not lose this flag when building the older baseline checkout.
Main's profile settings and the helper's compiler namespace must match on both sides
of a performance comparison.

The completed [Linux test job at `f82aaf09`](https://github.com/GitRbit/rbit/actions/runs/42174879076/job/127750932586)
ran 2,285 tests successfully, with one skipped. Its log confirms the actual SQLite
QPSG compile-option test, indexed repository lookup/assignee-search tests, and both
real-router work-count/access-recheck tests passed. These cover selected inherited
contracts on the rewrite's integrated session implementation. The overall workflow
was superseded and cancelled before Windows tests and release completed; this job
does not establish a complete hosted checkpoint or a latency result.

## Complete the audit during each feature migration

The initial table covers important shared fast paths; it is not an exhaustive claim
that every feature has been audited. Use main's source and its performance notes as
the migration checklist:

- [Performance follow-ups](../../performance-followups.md): statement re-preparation,
  full-ID resolution, query plans/counts, cache work sharing, cancellation, transfer
  size, preloading and scrolling. Historical timing tables retain their stated limits.
- [Bounded listings](../../performance-listings.md): directory metadata keyed by
  repository/commit/directory/entries, selected-page work, worker/result bounds,
  100-row lists, ref pickers, graph limits and non-JavaScript navigation.
- [Activity and graphs](../../activity-performance.md): deferred calendar scans,
  tip-keyed bounded day totals, exact partial-day semantics, priority-queue graph
  ordering and per-row lane lookup.
- [Frontend design](../frontend-speed-themes.md): assess proposed alternatives against
  implemented code; a proposal is not evidence that an optimization is active.

For every migrated feature, identify inherited optimization owners, cache/admission
and invalidation rules, SQL/Git work counts, transfer/DOM bounds and relevant existing
tests before changing the implementation. Include that evidence in its acceptance
record. Retain all user-visible features, accessibility, localization, security and
no-JavaScript behavior. Do not adopt an experimental alternative that main rejected
without new representative evidence addressing the earlier tradeoff.

## Performance and footer acceptance

Use matching release builds of pinned main and the candidate, identical seeded data,
configuration, language/theme/auth state and an idle compiler lane. The
[paired footer protocol](../../../scripts/perf/FOOTER-COMPARISON.md) distinguishes raw
SSR footer time, the current footer-interval header and total response timing. Cached
HTML is not a fresh sample. A correct changing footer alone proves no speedup.

Measure small pages and large repository workloads, cold and warm states, anonymous
and authenticated access, serial and concurrent traffic, overload and cancellation.
Preserve actual query/prepare/acquisition counts, cache work counts, CPU, memory and
transfer/DOM evidence alongside latency distributions. Compare minimum to minimum
only as supplementary information; use distributions and repeated runs for acceptance.
The original approximately 500 microsecond footer observation remains a useful target.
The subsequent [matched release comparison](measurements/footer-small-pages.md)
records sub-500-microsecond medians for both pinned main and checkpoint `7326e563`
on six small anonymous/authenticated page cases. It also records tail differences
and the limits of the fixture; this does not establish platform-wide parity or
validate later ports.

No optimization replacement is accepted solely because it passes compatibility tests.
Investigate any regression against the plan's performance gates, including the cost of
new abstractions, background work, allocation, request-time parsing and localization.
Unmeasured alternatives remain experiments until the relevant gates pass.


## Follow-up source audit at `91bb328`

This review compared pinned main `39f932a` with `91bb328` and read the four historical
performance notes against actual call sites. No Cargo, browser, live application,
allocation profiler or elapsed-time experiment ran. No removed original fast path was
found in the reviewed areas; this is a scoped source conclusion, not runtime parity.

| Area | Retained implementation and call-path evidence |
| --- | --- |
| Directory metadata | All28 tracked files under rbit-git/src are identical, including pinned reader/full-ID path and last_commit tree-cache/work budgets. repo routes, insights and insights_recent are unchanged; selected-page names still form repository/commit/path/sorted-name keys, with32 slots,4 worker permits,512KiB estimated slot budget and30s reuse. Workers retain permits after waiter cancellation; incomplete results stay out of complete persistent cache. Services composes one Arc-backed recent cache, not a new cache per request. |
| Bounded lists/ref picker | routes/repo.rs, routes/history.rs, routes/ref_picker.rs, views/pagination.rs, views/ref_picker.rs and language-files/graph templates are unchanged. Lists remain100/page, picker50/page; search precedes slicing, ref detail/protection matching is page-selected, graph requests201 then truncates200 and limits visible branch labels to8. Ref enumeration/language classification costs are still present, as the original docs acknowledge. |
| Calendar/graphs | services/contributions.rs, views/branch_graph.rs, views/contributions.rs, contributions.js and activity/calendar templates are unchanged. Activity still defers scanning unless calendar is requested; commit history does not implicitly load it. Day totals retain32-tip/25,000-day bounds, exact partial-day fallback and requested-day ranges. Graph ordering still uses BinaryHeap and one destination HashMap per row, not repeated edge-wide lane searches. |
| Virtualization/navigation | app.js, blocks.js, diff.js, preload.js, repo-navigation-cache.js, repo-navigation-bar.js and app.css are unchanged. Source/blame/diff blocks retain128-row geometry, containment and shared highlighting; diff indexes/delegated review handlers and preload cancellation/cache rules remain. views/blame.rs changed only its test Page fixture, not rendering. No DOM-windowing replacement or experimental segmented layout is active in production; representative probes are cfg(test). |
| Compression/assets | routes/mod.rs and assets.rs are unchanged, including route-layer order: Brotli HTML quality5 without zstd, ETag before compression, cached release asset Brotli quality11, content hashes and style/CSP caches. Static/Git/streaming routes keep their distinct policies. Debug still rereads/hashes assets and skips cached asset compression; source equality does not make a debug run comparable to an optimized release. |
| Page/SQL work | listing.rs, repositories service, AppState and views/components remain shared optimized paths. AppState is still a one-Arc clone. The exact work_counts.rs budgets remain unchanged (e.g. home3/5 statements anonymous/signed-in, repository home2/3); no budget increase was hidden in extraction. SESSION_USER SQL text and find_session_user call are unchanged and supply both flags in the one account query. |

### Changed paths and concrete added work

- **Session resolution retains its original backend work.** AccountService now creates
  a stack SessionApplication holding store references and copied policy. Static
  repository adapters forward get_live/get and find_session_user directly. The live
  path still does one redb read and one SQL projection, renews only after a strict
  greater-than-one-day expiry movement or at-least300s last-seen age, and deletes on
  absolute-age/missing-user refusal. New wrappers contain no Box, request-time store
  clones or added SQL call. The same postcard decode/materialization still owns its
  strings; extraction alone is not an allocation benchmark. Tracking/ledger admission
  applies to password mutations/guards, not ordinary GET session resolution.
- **Explore introduced an avoidable header clone.** Its new `HeaderMap` parameter used
  the locked axum-core 0.5.6 extractor, which returns `parts.headers.clone()`. Only four
  response-shape checks require those headers. Commit `497a7e9a` removes this through
  a narrow borrowing extractor. Two Rust tests cover the old expression across 48
  combinations and ambiguous/invalid headers; three HTTP localization/privacy tests
  also pass. No latency percentage is inferred from the removed operation.
- **Explore has small new dynamic text materialization.** English static labels use
  borrowed Cow and catalogs are compiled. However subtitle and PageOf are eagerly
  made into owned text even for a results fragment/single-page result; Stars creates
  an owned string per positive-star row where the old macro wrote count+literal
  directly to the Askama writer. CountForQuery replaces an already-allocating old
  total_label, so its entire cost must not be counted as new. These are bounded but
  real source-level extra operations; allocation-free/lazy formatting requires exact
  English/pseudo/escaping equivalence tests, not a claim of measured slowdown.
- **Locale work is scoped.** RequestLocale borrows and bounds preference parsing and
  memoizes it once on Explore, adds no preference SQL and does not run on every page.
  Page::new elsewhere sets an English enum; extract.rs is unchanged. The new language/
  cookie/shape Vary dimensions intentionally partition Explore cache entries. They
  can affect cache reuse, but removing them to obtain a faster sample would lose
  privacy/response-shape correctness. English/RTL document attributes add bytes;
  there is no source-wide new translation parser or full-platform localization pass.
- **Visible footer timing changed meaning at this checkpoint.** `timing.rs` itself is
  unchanged, but the new initial-navigation script selects total render timing where
  old initial HTML displayed the earlier SSR footer interval. This can change the
  displayed number without proving lost backend optimization. The subsequent view-
  metric correction in `dbfa4bf` has eight passing Rust tests covering precision,
  200/304 responses and task isolation. Compare raw SSR with raw SSR and total with
  total, retain 304 semantics, and use the exact executable's source contract.

### Historical notes are leads, not current regression evidence

rendering-review.md describes older branches with unbounded directories/refs; those
statements are stale for pinned main39f and91bb328, which contain the paged code above.
Its remaining explicit block-observer unobserve/cleanup concern and theme-picker
one-rAF-per-scroll path are still visible in unchanged blocks.js/theme-picker.js.
They are inherited opportunities, not optimizations removed by this rewrite; source
alone does not establish a browser leak or scrolling regression. Keep the existing
no-JavaScript paths and correctness tests when investigating them later.

The lock comparison also found pulldown-cmark0.13.5 ->0.13.6 despite unchanged
Markdown render/cache source; keep that dependency difference in README/rendering
comparisons rather than attributing every delta to crate extraction. Additional
thiserror/Windows version families were added; their actual target/feature activation
needs resolved-graph evidence, not an assumption from package presence alone.

Runtime effects still depend on actual release flags/QPSG, cache configuration/state,
permissions, seed size, auth/theme/language, background work and contention. The first
acceptance evidence remains unchanged work-count tests plus paired idle release runs;
this audit neither re-ran those tests nor turns identical blobs into a speed claim.


## Timing-header formatting follow-up

A source comparison of main `39f932a` and rewrite `128ecfcf` found an additional owned
formatting buffer in the new `view` Server-Timing metric: `view_metric` built a String,
then HeaderValue copied its bytes. Main only emitted `render`, whose formatter had
the same intermediate-buffer pattern. The actual timing middleware now formats both
metrics into a bounded 64-byte stack writer before creating the final validated owned
HeaderValue. The old view formatter is test-only reference code. The footer's existing
String formatting, metric precision, field ordering, HTML caching and borrowed ETag
inputs remain unchanged. Exact formatter oracles cover microsecond/millisecond
boundaries and Duration::MAX, alongside the existing real-router 200/304/no-footer
and interleaved-request timing tests. This removes specific intermediate source work;
no allocator-count or latency improvement has yet been measured.

The same review followed the extracted SessionApplication resume path and public
RepositoryDiscovery adapter against main: the former retains one live redb lookup
plus the combined SQL SessionUser projection and conditional renewal, and the latter
retains count-before-clamp-before-search ordering. The migrated issue creation and
label adapters preserve their existing store calls and ordered effects. No additional
query or removed cache fast path was identified in these reviewed paths; this is a
bounded source review, not an exhaustive or runtime performance claim.
