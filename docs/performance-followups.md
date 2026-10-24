# Performance follow-up evidence

This work separates reproducible work-count reductions from elapsed-time claims.
The earlier [frontend baseline](../scripts/perf/BASELINE.md) used a debug build under
heavy competing load; its browser sweep was explicitly invalid because CSS failed to
load and the pane was hidden. It is not a before/after performance baseline.

## Reproduction protocol

Use the [paired harness instructions](../scripts/perf/README.md#paired-http-measurements).
Build both revisions with identical release flags and assets, then stop build activity.
Use the same deterministic fixture refs and isolated server data directories. Preserve
build logs, executable hashes, metadata and every raw sample. Run at least two warmup
rounds and six measured rounds, alternating before/after order. Report medians and spread,
not a minimum selected from noisy samples. Repeat on an otherwise idle machine before
making latency claims.

The baseline is integrated source revision `327354d`, archived at
`target/perf-followups-baseline-source` in the primary checkout. The candidate is the
follow-up change, measured from an isolated worktree; record the exact candidate revision
and dirty-source fingerprint at measurement time. Existing frontend worker changes are
outside this comparison.

## Evidence available

- Harness tests exercise the real HTTP client against local synthetic responses:
  identity byte counts, redirects and malformed-response rejection, bounded response
  reading, alternating order, warmup exclusion, missing timing handling, raw sample
  capture and refusal to mix declared debug/release profiles.
- Browser-probe guards reject hidden or unstyled documents before scrolling, track
  visibility and viewport changes, and clean up observers and temporary styles on
  failure. The corrected header selector matches the current page template.
- The preloader's deterministic fake DOM/timer/fetch harness, using real navigation
  listeners, measured eight distinct queued intent requests followed by navigation to
  the first while two requests were active: baseline `327354d` started eight fetches;
  the candidate started two, discarding six obsolete queued requests. Unrelated active
  request cancellations changed from zero to one. Save-Data mouse intent changed from
  one fetch start to zero; scrolling after a 60 ms touch dwell now cancels the started
  request. These are scripted work/cancellation counts, not browser latency or byte
  measurements. Reproduce candidate behavior with
  `node --test scripts/tests/preload.test.mjs`; compare the baseline asset obtained with
  `git show 327354d:crates/rbit-web/assets/preload.js` in the same harness. All thirteen
  preload regressions pass, including overlapping navigation success, failure and `204`
  responses: speculative requests stay paused until the last navigation settles.
  A detached initiating link still releases failed navigation through native XHR
  terminal events, and duplicate terminal events do not resume speculation early.
- Highlighting regressions prove that concurrent requests compute once, cache hits
  share the same allocation, and file/blame views borrow those cached strings. A
  cancelled waiter retains its actual CPU job's admission slot; same-key requests can
  still join, while unrelated excess jobs receive `429`. Panic cleanup permits retry,
  and oversized results bypass retention.
- ETag regressions compare incremental slice hashing with the previous contiguous
  digest, including binary input and malformed markers. A two-MiB-body check verifies
  that the hash input borrows the response instead of copying it. Conditional requests
  still return `304`, and the response retains its render-time footer.
- A private-page integration regression verifies that session revocation overrides an
  old validator: a formerly authorized partial page returns `404`, not `304`, after
  access is revoked. Discussion preload tests cover the stricter audit policy for
  anonymous and signed-in visitors, without marking notifications read.

Run the harness checks with `node --test scripts/perf/harness.test.mjs`. These synthetic
test durations are test-runner output, not application benchmarks.

## Quality gates

The integrated candidate passes `cargo fmt --all --check`, workspace/all-target Clippy
with warnings denied, all **1,227 Rust tests**, `npm ci`, `npm run build`, all **26
JavaScript tests**, and `cargo deny check licenses advisories`. No tests are ignored.
The frontend rebuild leaves generated assets unchanged. The JavaScript count includes
13 preloader cases, eight measurement-harness cases and five service-worker cases.

## Measurement status

HTTP results measure server work and transfer only. Browser responsiveness requires
visible, styled, fixed-viewport results with `valid: true`, and must distinguish scripted
scrolling from physical wheel input. Do not extrapolate either result to the other.

### Integrated pass, 2026-10-23

Release builds (`lto = "fat"`) of `f6f9af7` (before) and of the integrated performance
branch (after), each from its own worktree and target directory, served the same seeded
fixture: one baseline seeding, its data directory copied for the other server. No builds
ran while sampling. Machine: i7-14700F, 28 logical CPUs, 32 GiB, Windows 11.

`measure.mjs`, anonymous, 2 warmups and 8 alternating rounds, `valid: true`. Median total
time in ms (interquartile range) and identity body bytes. The page marker was `<title`:
the inline stylesheet puts `id="main"` past the first 64 KiB of every full document, so the
default marker rejects them. "htmx request" sends `HX-Request` without a partial scope.

| Page | Before | After | Bytes before | Bytes after |
|---|---|---|---|---|
| Repository home | 6.5 (5.9–6.7) | 5.7 (4.9–6.2) | 168,398 | 168,641 |
| Directory | 5.1 (4.8–5.8) | 5.4 (4.7–5.8) | 165,128 | 165,371 |
| Directory, htmx request | 5.9 (5.1–6.0) | 4.1 (3.8–4.7) | 165,128 | 24,991 |
| Explore, htmx request | 2.6 (2.4–2.8) | 1.3 (1.2–1.5) | 152,072 | 11,935 |
| Issues | 4.3 (3.5–4.5) | 4.2 (3.5–4.7) | 170,559 | 170,802 |
| Pull requests | 4.8 (4.3–5.0) | 3.9 (3.8–4.6) | 171,684 | 171,927 |
| Pull request conversation | 8.9 (8.4–9.6) | 6.7 (5.8–7.3) | 163,775 | 164,018 |
| Commits | 4.7 (4.2–5.2) | 4.6 (4.4–5.5) | 162,744 | 162,987 |
| Blob, 20,000 lines | 31.6 (28.9–38.9) | 29.7 (26.9–31.3) | 4,015,175 | 4,015,418 |
| Blame | 35.0 (29.9–41.5) | 29.3 (28.1–36.1) | 4,199,264 | 4,199,507 |
| Compare, 152 files | 69.7 (62.7–74.6) | 66.8 (63.7–79.3) | 2,783,110 | 2,783,353 |
| Pull request files | 87.1 (77.4–97.5) | 76.5 (73.2–88.0) | 3,051,319 | 3,051,562 |

The anonymous sampler cannot see the signed-in Files tab, where line comment links were
replaced by one delegated handler. Measured separately with `curl` and a session on each
server, 8 alternating samples: 13,357,748 → 3,143,677 identity bytes, 628,977 → 307,224
Brotli bytes, 39,180 → 0 `hx-get` attributes, median 152 → 89 ms.

`sweep.mjs --rounds 5 --throttle 6`, the after build, the same URL with and without
`--no-backdrop`: switching off the sticky bars' backdrop blur did not help (blob 48.2 fps
with it, 45.0 without; pull request files 27.5 and 27.9; long frames 11 and 12, 56 and 54).
Headless Edge may raster differently from a visible window, so this does not rule out a
cost on a weak GPU. The signed-in Files tab at 27–28 fps under 6× throttling, with 54–56
frames over 33 ms per sweep, is the slowest scroll measured and the next candidate.

### SQLite pool contention, 2026-10-23

Question: should the one `SqlitePool` (CPU count clamped to 2–16 connections, here 16; 5 s
busy timeout) become a one-connection writer pool with `BEGIN IMMEDIATE` plus a
`query_only` reader pool?

**Instrumentation.** `sqlite::pool::Pool` labels the pool per store and exports
`rbit_sqlite_*` metrics (deployment guide): connection waits, `BEGIN IMMEDIATE` lock
waits, write transaction durations and busy errors by store, phase and extended code. A
release-build probe of a point query measured ~0.7 µs added per statement (15.2 → 15.9 µs).

**Bugs found.** Two deferred transactions read before writing:
`IssueStore::update_text` and `IssueMetaStore::set_milestone`. Once a transaction holds a
read, `SQLite` does not call the busy handler for the upgrade, so these failed at once with
`SQLITE_BUSY` (not only `SQLITE_BUSY_SNAPSHOT`) whenever another writer held the lock:
16 concurrent callers failed on the first round. Both now begin `IMMEDIATE`. The other
deferred write transactions write first, so they wait in the busy handler.

**Workload.** `scripts/perf/contention.mjs` (see the [harness README](../scripts/perf/README.md#sqlite-contention-workload)):
50% anonymous reads, 35% signed-in reads, 10% issue and 5% pull request comments that
notify nine participants, one push a second to an open pull request, and a webhook on
every event. Levels 4, 16 and 64 virtual users, 20 s each, on copies of one seeded data
directory. Release builds (`lto = "fat"`), port 3031, same machine as the integrated pass.
Variants: **single** (this branch), **split** (one writer connection for every
transaction and `execute`, writes with `RETURNING` routed to it, readers `query_only`;
branch `perf/sqlite-split`) and **gate** (the single pool, every write transaction
`IMMEDIATE` behind a one-permit async semaphore; branch `perf/sqlite-gate`). Three runs of
single and split and two of gate, interleaved (single, split, split, single, gate, gate,
single, split). Another agent's `rustc` was running at the end of single-2, single-3,
split-2, split-3 and during gate-1; runs waited for an idle machine before starting.
Medians of the runs; latencies are client-side ms.

| Users | Variant | Page reads/s | Anonymous p50 / p95 | Signed-in p50 / p95 | Comment p50 / p95 | Write tx p99 (server) |
|---|---|---|---|---|---|---|
| 4 | single | 428 | 3.8 / 9.1 | 3.7 / 18.6 | 1.0 / 60 | 293 |
| 4 | split | **560** | 5.4 / 12.1 | 5.3 / 14.9 | 1.5 / **7** | **1.6** |
| 4 | gate | 394 | 4.1 / 10.8 | 3.9 / 32 | 1.5 / 60 | 69 |
| 16 | single | **549** | 11.4 / 43 | 12.7 / 63 | 7.2 / 137 | 485 |
| 16 | split | 285 | 72 / 93 | 31 / 102 | 16.4 / **23** | **9.4** |
| 16 | gate | 410 | 13.6 / 54 | 14.2 / 107 | 23.6 / 131 | 131 |
| 64 | single | 328 | 159 / 228 | 199 / 309 | 138 / 168 | 68 |
| 64 | split | 313 | 177 / 245 | 211 / 320 | 126 / 154 | 6.4 |
| 64 | gate | 331 | 118 / 174 | 133 / 228 | 409 / 494 | 45 |

Failures and busy errors over all levels: single, one HTTP 500 (a pull request comment)
and two `SQLITE_BUSY` (`issues/transaction`, `notifications/begin`) in single-2 at 16
users, a run that overlapped a `rustc`; gate, one `SQLITE_BUSY` (`notifications/query`)
without a failed request; split, none. No acquisition timeouts in any run.

What the metrics show:

- Writers do not starve readers through the database: in WAL mode readers never wait for
  the write lock. They starve each other through the busy handler. With the single pool,
  comment p50 is 1 ms but p95 is 60 ms even at 4 users, and write transactions reach a p99
  of 0.3–0.5 s: a writer that misses the lock sleeps in `SQLite`'s backoff (1, 2, 5, 10 …
  100 ms) instead of being woken when the lock frees, while holding a pool connection.
  Rarely the backoff loses for the whole 5 s and the request fails.
- Serializing writers in-process removes that tail (split: p99 2–9 ms, no busy errors) and
  at 4 users lifts read throughput 31%, because connections no longer sleep. But the
  request path also issues many single-statement writes (notification read marks, webhook
  delivery bookkeeping, `INSERT … RETURNING`), and on one connection they queue: at 16
  users the writer is saturated at roughly 700 statements and transactions a second, write
  waits reach 5 ms per statement, issue pages (which mark notifications read) take 38 ms on
  the server, and read throughput halves. The gate, which serializes only transactions,
  keeps statement writes contending with them and gains nothing.
- At 64 users every variant is bound by the 16 connections: a request makes about 14
  acquisitions and each waits 17–23 ms on average. With the single pool, webhook deliveries fall
  from ~1,100 in 20 s at 16 users to ~170 there (split: 600–1,100), because the delivery worker waits for
  connections like everyone else.

**Recommendation: keep the single pool.** The split wins only at low concurrency and loses
half the read throughput at 16 users, so it is not kept. Its benefit, writers that queue
instead of polling, would win once request-path writes are rare enough that one
connection is not saturated: below a few hundred write statements a second and with no
writes on page views. Revisit when (a) `rbit_sqlite_busy_errors_total` grows in
production, or `rbit_sqlite_write_transaction_seconds` p99 stays above ~100 ms; and (b)
page views no longer write (batch notification read marks, move delivery bookkeeping to
transactions the worker owns). Before that, the levers with evidence are fewer
acquisitions per page and a pool size matched to them, which govern the 64-user case;
raising `busy_timeout` would only trade the rare 500 for a longer stall.

Raw reports: `target/perf/sqlite-locks/runs/*.json` and `.txt` in the primary checkout;
executables `single` 2ddd9b1a…, `split` 72108914…, `gate` dd62f529… (SHA-256). The single
binary was built before a test-only change to the instrumentation commit.

### Fewer statements per page, 2026-10-23

Following the recommendation above, branch `perf/fewer-queries` cuts the statements (each
one pool acquisition) the busiest pages take. These are work counts, not timings:
`rbit_storage::count_work` counts acquisitions, statements and insights cache (redb)
writes on the request's task, and `rbit-web/tests/work_counts.rs` pins them through the
real router, so a regression fails the test. Acquisitions equal statements on every page
below. The fixture: a public repository with an issue, two open pull requests in the same
repository and a second repository; the signed-in viewer is neither owner nor
collaborator, owns a repository, and has unread notifications in both.

| Page | Viewer | Before | After |
|---|---|---|---|
| `/` | anonymous | 3 | 3 |
| `/` | signed in | 6 | 5 |
| Repository home | anonymous | 7 | 2 |
| Repository home | signed in | 10 | 3 |
| Issue | anonymous | 12 | 4 |
| Issue, unread there | signed in | 17 | 8 |
| Issue, nothing unread there | signed in | 17 | 6 |
| Pull request conversation | anonymous | 17 | 9 |
| Pull request conversation | signed in | 24 | 11 |
| Pull request list (two rows) | anonymous | 17 | 10 |
| Pull request list (two rows) | signed in | 19 | 11 |
| Files changed (cache miss) | anonymous | 12, 1 cache write | 6, 1 cache write |
| Files changed (cache hit) | signed in | 14, 1 cache write | 7, no cache write |
| `/issues` (two rows, two repositories) | signed in | 14 | 9 |
| `/pulls` (no rows) | signed in | 4 | 4 |
| `/notifications` (two repositories) | signed in | 7 | 4 |

Per-row work is gone: a full `/issues` page of 25 rows from 25 repositories went from
4 + 75 statements to 5, and the inbox from one repository lookup and one grants query per
distinct repository to one statement. Where the counts went: the repository header folded
into its lookup (−5 anonymous, −6 signed in, on every repository page), the thread's
author (−1), one thread's metadata (−2), the read mark only for an unread thread (−2), the
same-repository head branch check (−2), the collaborator grant (−1), labels only on the
pull request list (−2) and both home lists' stars together (−1).

Not done: loading the list pages' filter menus (labels, milestones, assignable people) on
demand would save three more statements per list but lose the menus without JavaScript.

When the contention workload was re-run on this branch, it was about 30% *slower* than the
single-pool build before these cuts (16 users, anonymous reads: 413/s → 269/s, p95 38 →
73 ms), although serial page timings were the same. The cause was not the folding itself
but statement re-preparation, which the next section explains. With it fixed, the cuts are
a net win.

### SQLite statement re-preparation, 2026-10-24

Branch `perf/concurrency`. Server CPU per request on the repository's issue list rose from
2.5 ms at one client and 4.2 ms at four to 26 ms (single-pool build) and 37 ms (this
branch before the fix) at 16, and throughput at 16 clients fell below the 4-client rate.
Idle CPU was zero, so something spun or contended.

**Profiling.** Neither WPR nor samply can record without elevation here, so a small
user-mode sampler (scratchpad tool, not committed) suspended every thread of the server
each tick, walked its stack with dbghelp against a release build with line tables, and
weighted each stack by the cycles the thread used since the last tick
(`QueryThreadCycleTime`). The issue list at 16 clients, 8 s:

- 82% of the cycles were on `sqlx-sqlite-worker` threads, 18% on Tokio workers.
- 65% inclusive in `sqlite3_step` → `sqlite3Reprepare` → `sqlite3Prepare` → the parser,
  and most of that in `RtlEnterCriticalSection` under `sqlite3Malloc`: SQLite's global
  allocator mutex (`SQLITE_DEFAULT_MEMSTATUS` is on in the bundled build).
- Ruled out by the same profile: sqlx's channels (`flume` well under 1%), mmap, mimalloc
  (`_mi_*` under 0.1%), the blocking pool, the pool instrumentation and `count_work`
  (not visible), redb (not visible).

**Root cause.** libsqlite3-sys builds SQLite with `SQLITE_ENABLE_STAT4`. Once `PRAGMA
optimize` (added in this pass, at startup and in maintenance) has recorded `sqlite_stat4`
samples, the planner reads the value bound to any `column = ?` on a sampled index, and a
`LIKE ?` pattern even without samples. Such a statement is marked to expire whenever that
parameter is rebound, and sqlx rebinds every parameter on every execution, so each
execution re-parsed and re-planned the statement, and 16 connections doing so serialised
on the allocator mutex. The folded repository page statement, the largest, paid the most;
that is why only repository pages regressed when it replaced six small ones. Deleting the
`sqlite_stat4` rows of a loaded data directory took the issue list at 16 clients from
430/s to 980/s with the same binary, which confirmed it; the rest was the assignee menu's
`LIKE ?`.

**Fix.** `.cargo/config.toml` builds the bundled SQLite with `SQLITE_ENABLE_QPSG`
(`LIBSQLITE3_FLAGS`; the Dockerfile and CI builds read it): plans never depend on bound
values, so a rebind expires nothing. A storage test fails if the option is missing. The
planner can no longer derive a `LIKE` prefix range from a bound pattern, so mention
completion states its `NOCASE` name range. The pool size was not the problem and is
unchanged: after the fix CPU per request at 16 clients is 1.5–2× the 4-client figure, not
10×, and what remains is the file system work below.

Then the repository home, at 16 clients, spent 29% of its cycles in gix's abbreviated-id
lookup (`lookup_prefix` → `consolidate_with_disk_state`, `realpath`, `CreateFile`): pages
pass the commit they resolved once to every later read, and each full id went through the
prefix path, re-reading the object database's directory state. A full id of an existing
object now resolves directly, as `git rev-parse` does.

This branch's builds kept line tables for the sampler (`CARGO_PROFILE_RELEASE_DEBUG=
line-tables-only`, `CARGO_PROFILE_RELEASE_STRIP=none`; otherwise the release profile);
the single-pool build is the stripped one from the pool contention runs. No cargo or
rustc ran while measuring (three idle checks before each point).

`hammer.mjs`, 10 s per point, the contention fixture's data, alternating builds, two
rounds; requests/s and server CPU per request (user + kernel). *Tip* is the integration
tip, *SQLite* adds the QPSG build, *final* adds the commit id and query-plan commits
below (the plan restructuring did not move the issue pages, so the repository home's gain
in the last column is the commit id change).

| Page | Clients | Single pool | Tip | SQLite | Final |
|---|---|---|---|---|---|
| Repository home | 4 | 626/s, 6.6 ms | 634/s, 6.1 ms | 656/s, 6.0 ms | 937/s, 4.1 ms |
| Repository home | 16 | 489/s, 11.9 ms | 501/s, 11.5 ms | 527/s, 10.5 ms | 792/s, 7.3 ms |
| Issue list | 4 | 904/s, 5.6 ms | 874/s, 5.4 ms | 1,131/s, 3.7 ms | 1,134/s, 3.8 ms |
| Issue list | 16 | 643/s, 27.8 ms | 426/s, 39.5 ms | 1,070/s, 6.6 ms | 1,077/s, 6.8 ms |
| Issue | 4 | 1,034/s, 4.4 ms | 1,047/s, 3.7 ms | 1,209/s, 3.2 ms | 1,220/s, 3.2 ms |
| Issue | 16 | 1,029/s, 11.2 ms | 518/s, 29.7 ms | 1,123/s, 6.0 ms | 1,107/s, 6.0 ms |

The contention workload (`contention-ab.sh`: three alternating rounds, 16 and 64 users,
fresh copies of `target/perf/sqlite-locks/base`, port 3031), single-pool build against
this branch's final build; medians of the three runs, client-side ms:

| Users | Build | Anonymous reads/s | Anonymous p50 / p95 | Signed-in reads/s | Signed-in p50 / p95 | Comments p50 / p95 | Acquisitions |
|---|---|---|---|---|---|---|---|
| 16 | single pool | 404 | 9.4 / 36.4 | 283 | 10.2 / 55.6 | 5.1 / 77.5 | 250,000 |
| 16 | final | **525** | 7.3 / 32.6 | **365** | 7.1 / 37.8 | 2.8 / 75.9 | 169,000 |
| 64 | single pool | 199 | 143 / 204 | 145 | 176 / 280 | 125 / 157 | 121,000 |
| 64 | final | **274** | 113 / 170 | **198** | 127 / 206 | 112 / 146 | 80,000 |

No failures, busy errors or acquisition timeouts in any run. Write transaction p99 is
unchanged (~0.45 s at 16 users, the busy-handler backoff the pool contention section
describes); at 64 users connection waits still average 17–19 ms, so the pool remains the
limit there, now at about seven acquisitions per request instead of fourteen.

**Query plans.** The folded statements also planned scans when `sqlite_stat1` described
tiny tables: `REPOSITORY_PAGE` scanned `page`, the parent and the forks' repositories on the
post-load fixture (one-row `repositories`), and with one-row statistics the dashboard and
inbox check scanned `repositories` and `accounts`, the thread lookup `issues` and
`accounts`, the thread metadata labels, accounts and milestones. `PRAGMA optimize`
re-analyses a table only after it grows tenfold, and only when it runs. Join order and
`INDEXED BY` cannot force an inner rowid lookup, but a one-table query whose whole primary
or unique key is bound is planned by that key regardless of statistics (`whereShortCut`),
so these statements no longer join: each other row is a scalar subquery by key. Their plan
tests run with no statistics, one-row statistics and million-row statistics written into
`sqlite_stat1`, and fail on any scan of a table. Serially the page lookup went from ~9 µs
to ~13 µs, invisible in the page figures above; the statement counts are unchanged.

**Next.** At 16 clients the repository home and the issue list still spend roughly 40% of
their cycles in `CreateFile` under gix: `ThreadSafeRepository::open_from_paths` on every
request (the reader's pinned repository lives only as long as the request) and, for the
branch name, `consolidate_with_disk_state`. Keeping opened repositories across requests is
the next lever for repository pages.

Corrections to the sections above: the pool contention runs and the integrated pass were
measured with statements re-prepared on most executions (the startup `PRAGMA optimize`
had recorded `sqlite_stat4` samples), so their absolute throughput and connection wait
figures describe connections held longer than necessary. The comparison between pool
variants still stands, as every variant paid the same cost.

Raw reports: `target/concurrency/contention/*.json`, `.txt` in the primary checkout.

### Files tab scrolling, 2026-10-24

Release builds of `perf/integration` at `61de4a0` (before, sha256 `58c7b425…`) and of
`perf/files-scroll` at `686c443` (after, sha256 `8498f56c…`), served from copies of one
seeding. `sweep.mjs` and `trace.mjs`, 6× CPU throttling, 1280×1000, 5 rounds per page,
run one before/after pair at a time with the order alternating each round. Each pair ran
only after cargo and rustc had been absent for three checks in a row, and was discarded
and run again if either appeared during it. "Signed in" uses `--credentials`. The 27–28 fps
above was not reproduced on a quiet machine: the before build swept at about 51 fps, but
with 29–35 frames over 33 ms and a p95 of two frames.

Traces showed where the time went. On the Files tab Paint was 5.5 ms per frame against
0.34 ms on the 20,000-line file, while style, layout and `computeIntersections` were
alike. diff.js moves the file tree's current-file marker (`aria-current`) as the page
scrolls. The tree's file rows are links, and the base layer fades every link's colours
(`transition-colors`, 100 ms). Each move therefore started two fades, and every frame of
each fade restyled a row and repainted the whole sticky tree, which paints inside the
document. In one before sweep, 65 of 103 document repaints carried the tree, at
6–8 ms each. The rows now declare `transition-none` and fade only into `:hover`.
Separately, a window scroll listener in `contributions.js`, loaded on every page, ran
`document.querySelectorAll` on each scroll event. It cost 0.3 ms per frame on the
20,000-line file, 2.2 ms on the Files tab and 8 ms on blame (all at 6×). It now touches
only the tooltips it showed.

`sweep.mjs`, median [interquartile range]:

| Page | fps before | fps after | Frames > 33 ms before | after | p95 ms before | after |
|---|---|---|---|---|---|---|
| Pull request files | 50.8 [50.8–51.4] | 55.3 [54.8–55.5] | 31 [30–33] | 8 [7–9] | 35.5 | 18.5 |
| Pull request files, signed in | 51.8 [50.5–52.1] | 55.5 [55.4–55.5] | 28 [28–35] | 7 [5–8] | 35.2 | 18.5 |
| Compare, 152 files | 49.9 [49.6–50.4] | 55.1 [54.2–55.4] | 35 [35–36] | 11 [11–11] | 35.3 | 18.5 |
| File, 20,000 lines | 53.8 [53.8–53.9] | 54.0 [54.0–54.6] | 12 [12–13] | 11 [11–12] | 18.6 | 18.6 |
| Blame, same file | 52.7 [52.6–52.9] | 54.4 [54.4–54.5] | 12 [11–15] | 11 [11–12] | 18.5 | 18.5 |

Height changes, scroll corrections and placeholders more than 2 px off stay 0 on every
page.

`trace.mjs`, main-thread ms per frame, median, before → after:

| Page | RunTask | Style | Layout | PrePaint | Paint | Layerize | computeIntersections |
|---|---|---|---|---|---|---|---|
| Pull request files | 13.77 → 8.84 | 0.58 → 0.16 | 0.40 → 0.36 | 0.97 → 0.84 | 5.54 → 2.67 | 0.87 → 0.57 | 1.15 → 1.05 |
| Pull request files, signed in | 14.53 → 8.99 | 0.59 → 0.16 | 0.45 → 0.38 | 1.06 → 0.84 | 5.56 → 2.51 | 0.92 → 0.59 | 1.16 → 1.07 |
| Compare, 152 files | 14.48 → 9.17 | 0.56 → 0.16 | 0.41 → 0.36 | 1.05 → 0.88 | 5.71 → 2.81 | 0.83 → 0.57 | 1.17 → 1.13 |
| File, 20,000 lines | 6.14 → 6.49 | 0.49 → 0.56 | 0.66 → 0.66 | 0.54 → 0.56 | 0.34 → 0.40 | 0.19 → 0.21 | 1.01 → 1.18 |
| Blame, same file | 13.81 → 6.57 | 0.50 → 0.52 | 0.71 → 0.66 | 0.50 → 0.55 | 0.39 → 0.41 | 0.26 → 0.26 | 1.05 → 1.21 |

The 20,000-line file's per-frame differences lie within each other's interquartile
ranges (RunTask 6.07–6.83 before, 5.70–6.52 after), and its sweep is unchanged. Script
time per sweep (`FunctionCall`, which includes the sweep's own forced layouts) fell from
642 to 286 ms on the Files tab, from 148 to 42 on the file, and from 2,486 to 49 on blame.

Ruled out with injected stylesheets on the before build, where each change was within
noise or worse: signing in, the sticky file headers, row backgrounds, signs and numbers,
composited or opaque diff scrollers, `overflow: clip` on files, and replacing the diff
scroller with a clip. Splitting unblocked diffs into 128-row groups reduced the document's
own repaint time but raised layout. Two costs remain open. The tree still repaints when the
marker moves and when the tree scrolls to follow it, about 25 times per sweep. Repaints
that only follow the scroll position cost about 7 ms each on the Files tab against
0.7 ms on the file; this is not attributed further.

## Transfer efficiency

The existing build minifies Tailwind CSS and vendors minified htmx. esbuild bundles
and minifies the first-party page JavaScript into one `app.bundle.js` (`npm run
build:js`), the application script every page loads; only the small theme bootstrap
(`theme.js`) and the service worker (`sw.js`) keep their source formatting. HTML only
uses selective Askama whitespace trimming. HTML, static assets and buffered API routes
negotiate HTTP compression. This work does not add a blanket response minifier:
whitespace is part of repository source, diffs, Markdown code blocks and form values,
and raw downloads must retain their bytes.

API JSON, the generated OpenAPI document and the web manifest are already compact.
The enabled compression codecs are Brotli, gzip, deflate and Zstandard. Static assets
are compressed on request; precompressed variants could avoid repeated compression
CPU work and should be evaluated separately from source minification.
