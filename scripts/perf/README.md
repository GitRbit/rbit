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
| `serve.mjs` | Runs an already-built release binary by default, with configurable port/data/build directories, binary hash and raw server log. |
| `measure.mjs` | Alternating paired HTTP samples with warmups, raw JSON/JSONL evidence and distribution summaries. No browser automation. |
| `harness.test.mjs` | Tests configuration, ordering, HTTP validation, output capture and browser validity guards. |
| `seed.mjs` | Signs up the test account `perf`, creates `perf/perf-fixture`, pushes both branches over HTTP and opens pull request #1. Rerunnable. |
| `scroll-probe.js` | Evaluated in the page: DOM size, navigation timing, a rAF-driven scroll sweep, scrollbar-drag jumps, blank-line sampling, long animation frames, layout shifts. |
| `trace.mjs` | Headless Edge: a DevTools trace of a scroll sweep per page, main-thread time per phase (style, layout, pre-paint, paint, `content-visibility` intersection checks, …). |
| `sweep.mjs` | Headless Edge: scroll smoothness (fps, long frames), height changes and scroll corrections during a sweep, scrollbar-drag cost, and a check that every `content-visibility: auto` placeholder is exact. |
| `edge.mjs` | What both share: launching headless Edge with its own profile, the DevTools protocol, the sweep, killing the browser's whole process tree on exit. |
| `contention.mjs` | Concurrent mixed reads, comments, pushes and webhook deliveries at several concurrency levels, with the SQLite pool's lock metrics ([below](#sqlite-contention-workload)). |
| `BASELINE.md` | The recorded baseline. |

## Reproduce

```bash
npm run build                               # release embeds the generated assets
CARGO_TARGET_DIR=target/perf-build cargo build --release -p rbit-server
node scripts/perf/fixture.mjs                 # target/perf/fixture
node scripts/perf/serve.mjs                   # second terminal; Ctrl+C stops it
node scripts/perf/seed.mjs http://127.0.0.1:3005 # prints the four pages to probe
```

The launcher prints its base URL (default http://127.0.0.1:3005); pass that same origin
to `seed.mjs`, for example `node scripts/perf/seed.mjs http://127.0.0.1:3005`.
Sign in at that origin's `/login` as `perf` with the password in
`target/perf/credentials.txt` (the repository is public, so signing in only matters for
the logged-in chrome). Then, on each page, in the devtools console:

```js
globalThis.PERF_PROBE_OPTIONS = { noBackdrop: false }   // true: blur suspect off
// paste scripts/perf/scroll-probe.js and wait for the result (~20 s), then:
copy(JSON.stringify(PERF_PROBE_RESULT, null, 2))
```

The report is also kept in `globalThis.PERF_PROBE_RESULT`. During agent work use only the
provided CUA browser API to interact with a visible browser; do not start another CDP or
Playwright driver. The probe requires loaded stylesheets, applied page styling, a complete
document and a visible fixed viewport. Hidden tabs, resizing or a stalled animation frame
invalidate the run and clean up the probe. Only compare reports with `valid: true`.
Use at least six paired rounds, alternating A/B then B/A; reload between runs so remembered
`content-visibility` sizes start equal. Record screenshot evidence of correct styling as
well. A scripted scroll sweep is not a measurement of physical wheel input.

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
`PERF_BROWSER` to use another Chromium. `--credentials target/perf/credentials.txt` signs
the browser in first, as `seed.mjs`'s account, at every page's origin, so signed-in pages
(the pull request Files tab with its line comments) are measured as their viewers see
them; the session cookie goes from the server's answer straight into the browser and is
never printed. Cookies ignore the port, so give each signed-in server its own host
(`127.0.0.1:3021` and `localhost:3022`). `sweep.mjs` also takes `--no-backdrop a,b`: the
named pages are swept with `backdrop-filter` off on the sticky bars, so one URL under two
names (`blur=URL plain=URL --no-backdrop plain`) measures what the blur costs.

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

- The pages send `Content-Security-Policy` with `script-src 'self'` and `style-src 'self'`
  plus hashes authorizing only the embedded initial styles: the probe
  cannot be `eval`ed from page script or fetched from another port, and an inline
  `<style>` is blocked (the probe uses a constructed stylesheet instead).
- Check the page is styled before measuring (`valid` must be true). In the Claude
  desktop Browser pane, `app.css` and the theme
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

- Debug and release results are different experiments. `--profile debug` is available
  for development, but never compare it to release or label it production performance.
- `sweep.over16_7ms` / `over33ms` count frames longer than one and two 60 Hz frames, from
  requestAnimationFrame timestamps; on a 120 Hz display the median is ~8.3 ms.
- `blank` tallies sample points on code lines whose content is skipped or has an empty box
  (`content-visibility: auto` not yet rendered); `jumps.blank.frame1` is what was painted in
  the first frame after a jump.
- `sweep.positionCorrections` counts frames where the page moved the scroll position by
  itself (scroll anchoring as estimated sizes resolve) — the "scroll jumps" suspect.
- `noBackdrop` removes `backdrop-filter` from `body > header` and `.site-subnav` (both
  sticky and blurred); pass `backdropSelector` to test one alone.

## Paired HTTP measurements

Build each revision with identical toolchain, release flags and assets, **before** timing.
Keep build command/logs: launch-time Git metadata is not proof of which source produced
an executable. On PowerShell use `$env:CARGO_TARGET_DIR = 'target/perf-build'` followed
by `cargo build --release -p rbit-server`. Use a distinct target directory for each
revision, then launch each from its own worktree, with isolated copies of the same seeded
data and separate ports. Never open the same live SQLite/redb directory in two servers.

```text
node scripts/perf/serve.mjs --port 3005 --metadata target/perf/before-server.json
node scripts/perf/serve.mjs --port 3006 --metadata target/perf/after-server.json
```

The launcher accepts `--profile release|debug`, `--target-dir PATH` and `--data-dir PATH`.
For archived source exports use `--source-revision 327354d` (or the actual source revision).
Use `--source-dir PATH` to run a binary from its archived source directory without
copying the harness there; this controls the child's working directory and fingerprint.
Other relative CLI paths remain relative to the worktree containing the harness.
It records this as a declaration and refuses to attribute an ancestor repository's HEAD
to an export that is not itself a Git root.
It saves an executable SHA-256, source revision/diff fingerprint, start time and raw log;
existing metadata files are never overwritten. Record relevant server configuration in
the run notes, excluding credentials. Background work, dataset sizes and account state
must match. The HTTP sampler is deliberately anonymous and sends no cookies.

Save a configuration such as `target/perf/comparison.json` (all filesystem paths below
are relative to that configuration, URLs are origins of already-running servers):

```json
{
  "output": "comparison-01.json",
  "fixture": "fixture",
  "notes": "Same fixture refs/data/settings; no builds; record OS load and build commands here",
  "rounds": 6,
  "warmups": 2,
  "variants": [
    { "name": "before", "baseUrl": "http://127.0.0.1:3005", "metadata": "before-server.json" },
    { "name": "after", "baseUrl": "http://127.0.0.1:3006", "metadata": "after-server.json" }
  ],
  "pages": [
    { "name": "blob", "path": "/perf/perf-fixture/blob/main/src/huge.rs" },
    { "name": "blame", "path": "/perf/perf-fixture/blame/main/src/huge.rs" },
    { "name": "compare", "path": "/perf/perf-fixture/compare/main...big-change" },
    { "name": "pull-files", "path": "/perf/perf-fixture/pull/1/files" }
  ]
}
```

```text
node --test scripts/perf/harness.test.mjs
node scripts/perf/measure.mjs target/perf/comparison.json
```

Requests run serially, alternating variant order each round, with explicit warmups kept
in raw output but excluded from summaries. JSONL is appended after every request; JSON
includes all samples and median/interquartile range/p95. A failed request, redirect,
unexpected status, compressed response or missing HTML marker makes the run invalid and
sets a failing exit status. Output files are exclusive; use a new name for each run.

`headersMs` measures time to response headers; `totalMs` includes reading and hashing the
body; `renderMs` is the server's timing header and remains absent if unavailable.
`bodyBytes` counts identity HTTP entity bytes, excluding headers/framing. These are **warm
server HTTP** results with persistent connections; Node does not simulate browser caching,
HTML parsing, DOM layout, painting or navigation responsiveness. Restart-only runs still
have OS caches; do not call them cold disk measurements. Keep cold/startup experiments
separate and restart both variants equally for every paired sample.

Optional page `mode: "partial"` sends `HX-Request` and a required repository `scope`;
`mode: "preload"` additionally sends `HX-Preloaded`. Set `status: 204` only when testing
an intentional decline, and `contains` to override the default `id="main"` HTML marker.
These modes provide server response evidence only: request counts from actual preload
behavior come from the dedicated preloader tests or a visible browser network capture.
Never present a fast `204` as faster rendering of the corresponding `200` page.

## SQLite contention workload

`contention.mjs` drives concurrent mixed traffic at one server and reads the SQLite pool's
metrics (`rbit_sqlite_*`, see the deployment guide) before and after each concurrency
level. Each virtual user loops over a seeded random mix: 50% anonymous page reads, 35%
signed-in page reads (including the notification inbox and the `/issues` dashboard; issue
views mark notifications read), 10% issue comments and 5% pull request comments. Every
account participates in every thread, so each comment notifies all the others. One extra
lane pushes an empty commit to an open pull request's branch every second over HTTP.
A webhook subscribed to pushes, issues, comments and pull requests delivers to a listener
the script runs, so every write also queues deliveries.

```bash
# release build, seeded fixture (see "Reproduce"); then, with the server's environment:
RBIT_OPERATIONS__METRICS=true RBIT_SECURITY__RATE_LIMIT=false \
RBIT_WEBHOOKS__ALLOW_PRIVATE_ADDRESSES=true node scripts/perf/serve.mjs --port 3031 --data-dir target/perf/contention/data
node scripts/perf/seed.mjs http://127.0.0.1:3031
node scripts/perf/contention.mjs setup http://127.0.0.1:3031
node scripts/perf/contention.mjs run http://127.0.0.1:3031 --levels 4,16,64 --seconds 20 --output target/perf/contention/run-01.json
```

`setup` (once per data directory, rerunnable) signs up `load1`…`load8` with the seeded
account's password from `target/perf/credentials.txt` (never printed), opens issues #2–#5
and a `load-push` pull request, adds the webhook (listener port `--hook-port`, default
3032) and makes every account participate. Stop the server after it and copy its data
directory for each run, so every variant starts from the same rows. `--perf-dir` points
at another `target/perf` (credentials) and `--work` at the directory holding the push
clone and `setup.json`.

For each level the report gives operation counts, throughput, failures (status 0 or
≥ 400) and client-side p50/p95 latency per operation, the webhook deliveries received,
and from the server: connection acquisition waits (`query`, `transaction`), `BEGIN
IMMEDIATE` write lock waits and write transaction durations (mean and histogram
p50/p95/p99, interpolated within buckets), busy errors by `store/phase/code`, acquisition
timeouts and 5xx responses. Any failure sets a failing exit status. The client runs in
one Node process on the same machine as the server, so at high levels its latencies
include client scheduling; compare variants at the same levels, alternately, on an idle
machine.

## Server-side caps the fixture meets

`src/huge.rs` stays under 512 KiB, the limit for highlighting (`MAX_HIGHLIGHT_BYTES`) and
blame (`BlameLimits::max_file_bytes`); above it the blob page is plain text and blame
refuses. The big change (~25,000 changed lines) exceeds `DiffLimits::max_total_lines`
(20,000), so the compare and pull request pages show the first files in full and the
rest truncated, as a real oversized change would.
