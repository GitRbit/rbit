# Frontend performance harness

Reproducible measurements of large pages (a 20,000-line file, its blame, a 150-file
compare and pull request) while scrolling. Everything lives under `target/perf/`
(gitignored): the fixture repository, the server's data directory and the generated test
credentials.

| File | What it does |
|---|---|
| `fixture.mjs` | Builds the deterministic fixture repository (seeded PRNG, fixed commit dates, so commit ids never change). |
| `serve.mjs` | Runs the debug build from `target/perf-build` on http://localhost:3005 with data in `target/perf/data`. |
| `seed.mjs` | Signs up the test account `perf`, creates `perf/perf-fixture`, pushes both branches over HTTP and opens pull request #1. Rerunnable. |
| `scroll-probe.js` | Evaluated in the page: DOM size, navigation timing, a rAF-driven scroll sweep, scrollbar-drag jumps, blank-line sampling, long animation frames, layout shifts. |
| `BASELINE.md` | The recorded baseline. |

## Reproduce

```bash
CARGO_TARGET_DIR=target/perf-build cargo build -p rbit-server   # its own exe, never locked by a dev server
node scripts/perf/fixture.mjs                 # target/perf/fixture
node scripts/perf/serve.mjs                   # second terminal; Ctrl+C stops it
node scripts/perf/seed.mjs                    # prints the four pages to probe
```

Sign in at http://localhost:3005/login as `perf` with the password in
`target/perf/credentials.txt` (the repository is public, so signing in only matters for
the logged-in chrome). Then, on each page, in the devtools console:

```js
globalThis.PERF_PROBE_OPTIONS = { noBackdrop: false }   // true: blur suspect off
// paste scripts/perf/scroll-probe.js and wait for the result (~20 s), then:
copy(JSON.stringify(PERF_PROBE_RESULT, null, 2))
```

Pasting the file evaluates to a promise (the console awaits and prints it); the report is
also kept in `globalThis.PERF_PROBE_RESULT`. Browser automation can evaluate the file's
contents the same way. Run each page twice, with `noBackdrop` false and true, at the same
window size, and reload between runs so caches and `content-visibility` remembered sizes
start equal.

To start over: stop the server, delete `target/perf/data` (and `credentials.txt`), rerun
`seed.mjs`. Rerunning `fixture.mjs` replaces only a directory it created itself.

## Pitfalls seen while recording the baseline

- The pages send `Content-Security-Policy: script-src 'self'; style-src 'self'`: the probe
  cannot be `eval`ed from page script or fetched from another port, and an inline
  `<style>` is blocked (the probe uses a constructed stylesheet instead).
- Check the page is styled before measuring (`dom.headerBackdrop` should not be `none`
  on a desktop-width window). In the Claude desktop Browser pane, `app.css` and the theme
  stylesheet failed with `net::ERR_FAILED` while scripts loaded, so pages were unstyled.
- The pane only animates while it is visible; hidden, requestAnimationFrame stops.
- Programmatic scrolling (`scrollTop = …`) of a 356,000-element page cost 130-300 ms of
  main thread per call there. Compare it with a real wheel or scrollbar drag in the
  Performance panel before trusting the sweep's frame numbers.
- Cookies are per host, not per port: signing in on localhost:3005 replaces an rbit
  session cookie of another server on localhost in the same browser profile.
- Run on an otherwise idle machine; parallel `cargo` builds moved the blob page's
  server render from 2.5 s to 18 s.

## Reading the numbers

- Debug build: server timings are only comparable with each other, not with production.
- `sweep.over16_7ms` / `over33ms` count frames longer than one and two 60 Hz frames, from
  requestAnimationFrame timestamps; on a 120 Hz display the median is ~8.3 ms.
- `blank` tallies sample points on code lines whose content is skipped or has an empty box
  (`content-visibility: auto` not yet rendered); `jumps.blank.frame1` is what was painted in
  the first frame after a jump.
- `sweep.positionCorrections` counts frames where the page moved the scroll position by
  itself (scroll anchoring as estimated sizes resolve) — the "scroll jumps" suspect.
- `noBackdrop` removes `backdrop-filter` from `header.site-header` and `.site-subnav` (both
  sticky and blurred); pass `backdropSelector` to test one alone.

## Server-side caps the fixture meets

`src/huge.rs` stays under 512 KiB, the limit for highlighting (`MAX_HIGHLIGHT_BYTES`) and
blame (`BlameLimits::max_file_bytes`); above it the blob page is plain text and blame
refuses. The big change (~25,000 changed lines) exceeds `DiffLimits::max_total_lines`
(20,000), so the compare and pull request pages show the first files in full and the
rest truncated, as a real oversized change would.
