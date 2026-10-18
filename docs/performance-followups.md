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

No valid paired application latency or scrolling measurements have been recorded by this
follow-up harness yet. The machine was memory-pressured while the harness was prepared;
release builds and a quiet measurement window are required. HTTP results, once collected,
measure server work and transfer only. Browser responsiveness requires visible, styled,
fixed-viewport results with `valid: true`, and must distinguish scripted scrolling from
physical wheel input. Do not extrapolate either result to the other.

## Transfer efficiency

The existing build minifies Tailwind CSS and vendors minified htmx. First-party
JavaScript is served with its source formatting, and HTML only uses selective Askama
whitespace trimming. HTML, static assets and buffered API routes negotiate HTTP
compression. This work does not add a blanket response minifier: whitespace is part
of repository source, diffs, Markdown code blocks and form values, and raw downloads
must retain their bytes. Build-time JavaScript minification remains a separate
opportunity; measure compressed transfer bytes and preserve content-hashed module
imports before adopting it.

API JSON, the generated OpenAPI document and the web manifest are already compact.
The enabled compression codecs are Brotli, gzip, deflate and Zstandard. Static assets
are compressed on request; precompressed variants could avoid repeated compression
CPU work and should be evaluated separately from source minification.
