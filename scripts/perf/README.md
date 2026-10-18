# Frontend performance harness

## Real repositories

With the development server running (`npm run dev`), run:

```bash
npm run test:realworld -- http://localhost:3000
```

This creates public `realworld/rbit` and `realworld/git` repositories on the local
server using a generated test account. rbit comes from the workspace's configured
`origin` remote (override with `RBIT_TEST_SOURCE`); the scale workload is Git itself from
`https://github.com/git/git.git`. Both include complete history, all branches and tags.
The initial download and HTTP push/clone can take several minutes and require substantial
disk space. Sources, clones, generated credentials and `report.json` live in
`target/realworld/`, outside version control. Repeated runs fetch upstream updates;
pushes use normal fast-forward checks and preserve existing test repositories.

The runner verifies advertised refs against upstream, clones anonymously over HTTP,
compares HEAD, all branch/tag object IDs and full-history commit counts, runs
`git fsck --full --strict`, and pushes, fetches and deletes a temporary probe branch.
Four concurrent reads exercise browsing under overlapping requests. It checks tree listings, deferred entry
commits, history, branches, the branch graph, tags, source rendering, blame, commit diffs,
compare and language-file browsing. Raw README/source bytes must exactly match upstream.
The JSON report records revisions, repository sizes, push/clone durations and each page's
status, response bytes and elapsed time. Any failed assertion exits nonzero.

Use a development server with registration enabled. The runner only accepts loopback
origins and keeps its generated password out of URLs, command arguments and reports.
Run one instance at a time; don't clone into the source cache while a run is active.
These are debug-server measurements and functional scale checks, not production load
capacity numbers. Upstream revisions are recorded because these repositories change.

## Synthetic scrolling fixture

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
| `trace.mjs` | Headless Edge: a DevTools trace of a scroll sweep per page, main-thread time per phase (style, layout, pre-paint, paint, `content-visibility` intersection checks, …). |
| `sweep.mjs` | Headless Edge: scroll smoothness (fps, long frames), height changes and scroll corrections during a sweep, scrollbar-drag cost, and a check that every `content-visibility: auto` placeholder is exact. |
| `edge.mjs` | What both share: launching headless Edge with its own profile, the DevTools protocol, the sweep, killing the browser's whole process tree on exit. |
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

## Compare builds in headless Edge

`trace.mjs` and `sweep.mjs` take named URLs and visit them in turn, round after round, so
drift on the machine (a build, another browser) hits every page alike; they print the
median and interquartile range per page. Run the builds to compare side by side, each on
its own port with its own copy of the data, e.g. the debug build of an older commit from a
worktree of it (debug builds read `assets/` from the directory they were built in, so build
its `app.css` there too):

```bash
node scripts/perf/trace.mjs --rounds 5 --throttle 6 \
  old=http://localhost:3032/perf/perf-fixture/compare/main...big-change \
  new=http://localhost:3031/perf/perf-fixture/compare/main...big-change
node scripts/perf/sweep.mjs --rounds 5 --throttle 6 old=… new=…
```

Options (both): `--rounds 5`, `--ms 6000` (sweep length cap; it also stops at the bottom),
`--vps 5` (viewports per second), `--throttle 6` (CPU slowdown; 1 for none, where a
healthy page holds 60 fps), `--width 1280 --height 1000`, `--json file` (every run). Set
`PERF_BROWSER` to use another Chromium.

- `trace.mjs` sums complete trace events on the renderer's main thread. `RunTask` is the
  total; the phases nest inside it and partly in each other, so compare a phase with the
  same phase. `computeIntersections` is mostly `content-visibility: auto` deciding which
  elements are near the viewport: it grows with the number of such elements and with the
  scroll containers between them and the viewport. The sweep reads `scrollHeight` every
  frame, so `FunctionCall` includes forced layouts; they are the same for every page.
- `sweep.mjs`: `over33` counts frames longer than two 60 Hz frames per sweep;
  `heightChanges` and `corrections` must be 0 on a page whose placeholders are exact
  (a change is the scrollbar thumb jumping, a correction the content jumping);
  `placeholdersOff` lists `content-visibility: auto` elements whose placeholder (the
  `contain-intrinsic-block-size` length plus padding and borders) is more than 2 px off
  their rendered height, measured after the sweep with every element forced visible.
- Fine-grained timing in a visible browser (the desktop Browser pane, a devtools
  session) is not comparable: renderers there are throttled or paused when hidden and
  share the machine with the UI. Use these scripts for numbers, a visible browser for
  looks.
- The browser is killed with its whole process tree when the script ends, even on an
  error or Ctrl+C; on Windows it is found by its profile directory, because the process
  Node starts hands over to another and exits.

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
