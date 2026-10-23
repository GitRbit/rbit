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
