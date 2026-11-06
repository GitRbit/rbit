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
The original approximately 500 microsecond footer observation remains a useful target
to investigate, not a verified result for this rewrite.

No optimization replacement is accepted solely because it passes compatibility tests.
Investigate any regression against the plan's performance gates, including the cost of
new abstractions, background work, allocation, request-time parsing and localization.
Unmeasured alternatives remain experiments until the relevant gates pass.
