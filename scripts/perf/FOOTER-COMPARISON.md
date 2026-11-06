# Paired release footer comparison

Prepared protocol, not measured results. Baseline is pinned GitHub main
39f932a00b7aa35b324e06a9ff2343a57d4bed76. The candidate revision/build record must be
recorded when its release build completes. The user's earlier ~500µs observation has
unknown build/data/cache conditions and is not this baseline's measured result.

## Separate the timing observations

- `footerSsrMs` is parsed from the raw HTML footer while Askama renders it. It includes
  preceding handler/session/template work, but not later middleware finishing work.
  Its source has rounded human-readable precision (integerµs / one decimalms /
  two decimalsseconds), not an exact profiler timestamp.
- `renderMs` is Server-Timing render duration for the whole timed response pipeline,
  including work after the template/footer was rendered. Compare this to the same
  header metric, not to another revision's visible SSR footer.
- `viewMs` is the candidate's new Server-Timing view duration, captured at the same
  footer interval and truncated to whole microseconds. Main39f has no view metric:
  its viewMs remains null/count0, never substituted from render. Raw SSR remains the
  common comparison metric; candidate view provides fresh precision on revalidation.
- The visible browser footer is a presentation contract. Main39f initial navigation
  retains SSR text. The intermediate render-only navigation fix displayed total time;
  ef3fee3 restores the footer interval by preferring finite view on navigation and
  HTMX, retaining render only as a legacy/non-footer fallback. Confirm the exact
  executable embeds that server change, not just rebuilt browser assets. A304 has
  no current HTML footer at all: cached SSR text is old.

The HTTP sampler now records raw footer, view and total metrics independently, rejects required missing/
malformed values, keeps304 footer null, and records every cache prime separately. It
reads a bounded tail so the real footer can follow more than64KiB of page content.
This does not measure browser paint, JS/CSS execution, cached transfer bytes or scrolling.

## Source/assets/build preparation

Use existing nested reference checkout and a current harness checkout. Do not copy
modified source into the reference or build the active PR. Reference npmci with the
pinned lock and npmrunbuild completed; gitstatus remained clean. Generated ignored
CSS exists, and tracked bundle/vendor bytes were unchanged. Before each release build,
retain asset hashes and gitrevision/diff status locally. Build both with the SAME Rust
version/host target, release profile, Cargo features, flags and Node dependency lock;
identify the exact resulting executable hashes. Do not infer build identity from a
launch-time HEAD or executable directory name.

The reference predates the shared build helper, but the CURRENT helper can be invoked
by absolute path with the reference as cwd: its context follows cwd, not script location.
After root grants the compiler lane, run its build --release --package rbit-server
--locked there. Do not pass --manifest-path or override target/build/output dirs;
those bypass helper safety. The helper uses the existing shared compiler namespace and
copies generation-specific executables before releasing its lease. Preserve candidate
and baseline runnable generations. Never compile or run npm watchers during timing.
Reference Node dependencies occupy ~214MB logical bytes; no Rust output was created.
No per-worktree target directory or independent compiler/intermediate namespace is
needed. Current D free space was ~1.66TB at preparation, not an approved cache budget;
shared helper retention/budget policy still governs build storage.

## Isolated identical seed

Use a NEW private run directory under the common repository .worktree-state/perf/
footer-comparison-<unique-id>, not the user app on3100, its data, cookies or browser
profile. Suggested dedicated HTTP ports3305/3306 must first be proven free; never stop
an unrelated listener. All fixture, server data, credentials and raw logs live in this
run directory, outside source and public reports. Do not create target/perf in each
worktree merely because old harness defaults point there.

1. Use existing fixture.mjs with an explicit new RUN/fixture path; record deterministic
   refs and object IDs. This fixture is synthetic and does not fetch moving upstreams.
2. Generate private setup-token and login-credentials files in RUN (0600 where supported;
   preserve private directory ACLs on Windows). Supply the setup token to the benchmark
   server through RBIT_SECURITY__SETUP_TOKEN in its environment, never argv/report/log.
   Set identical common configuration: updates.check_for_updates=false, actions.enabled=
   false, ssh.enabled=false, mail.transport=log, same default theme/DNA palette, same
   authentication and rate-limit settings. Use the same release asset embedding policy.
   Use serve.mjs --isolated true: it strips inherited RBIT settings, refuses source-dir
   private config files, fixes common benchmark switches and keeps the private setup
   token only in child environment. It does not dump that environment into metadata.
3. Launch ONLY the baseline seed server using the CURRENT serve.mjs --executable exact-
   path or --build-record record, --source-dir reference, --profile release --isolated true, --data-dir
   RUN/seed-data, --port3305 and --metadata RUN/seed-server.json. Launch cwd must contain
   no user rbit.toml; current reference is clean. Launcher does not compile.
4. Run existing seed.mjs with baseURL plus explicit environment RBIT_PERF_FIXTURE=
   RUN/fixture, RBIT_PERF_CREDENTIALS_FILE=RUN/credentials.txt and
   RBIT_PERF_SETUP_TOKEN_FILE=RUN/setup-token.txt. The first signup now supplies the
   bootstrap token; credentials still travel only in POST body/Git environment, not
   argv/remote URLs/reports. The seeder accepts only HTTP loopback origins.
5. Stop the OWNED seed server through existing runtime/process-tree control and verify
   its close/drain proof. Only then copy seed-data twice to RUN/main-data and
   RUN/candidate-data. Never copy live SQLite/redb files or open a live directory from
   two processes. Retain the original seed unchanged for restart/cold rounds.
6. Launch exact baseline/candidate release executables against their separate copies,
   ports3305/3306 and matching public URLs. Use the CURRENT serve harness with explicit
   source-dir for each and exclusive metadata/log paths. Candidate migrations are part
   of its startup, not timed requests; record schema/repository/user counts locally.
   Confirm both show the same public fixture, English, theme and anonymous/auth chrome.
   No pending reset/MFA/setup states, notifications or Actions jobs should differ.

Credentials, setup tokens, cookies, SQL/redb files and raw server logs are PRIVATE.
The launch metadata also contains local paths/refs: keep raw evidence local and publish
only a reviewed sanitized summary. Do not dump configuration/environment to reports.

## Warm paired HTTP

Copy footer-comparison.example.json into RUN and resolve its relative fixture/metadata
paths there. It uses the existing measure.mjs:50 alternating A/B then B/A paired rounds,
5 warmups/case, identity content encoding, Accept HTML/en, one persistent agent/variant,
serial requests and no browser cache. Warmups and cache-prime responses remain raw
but are excluded from p50/IQR/p95 summaries. Same host/case/order/header policy applies
to both; exclude all compiler/npm/browser-load overlap and record observed idle state.
The sampler validates every sample; failed redirects/content/status/timing invalidate
rather than being silently filtered into faster medians. It records bytes/hash, status,
header duration, full wire duration, raw SSR duration, view duration and selected
non-secret headers. The example requires view only on the candidate. Its executable
MUST include ef3fee3 (or equivalent integrated server code); an older release missing
view is an invalid candidate for that contract, not a0ms observation.

Run anonymous cases /, /explore, /login as fresh200 GETs and separate explicit304
revalidations. Each conditional case first primes a200+ETag on its own variant and
path; no current SSR metric is synthesized from a304. If a route genuinely sends
no-store/no validator on either revision, record that protocol difference before
excluding its conditional case from both; keep its normal200 comparison.

For authenticated comparison, use a separate output file and add credentialsFile to
BOTH variants, referencing the private seeder file. Login is performed once per origin;
session cookies remain separate in memory and are redacted from serialized headers.
Do not paste cookies into configuration. Compare / and /explore plus /settings/account,
not /login (which intentionally redirects authenticated viewers). Require an auth-only
marker, e.g. action="/logout", on public pages. Validate displayed account/DNA settings
before timing. Per-origin HTTP agents do not share a cookie jar even though browsers
share cookies across ports. Keep credential-file contents and session-cookie values
out of config, argv, console and raw/summary reports; synthetic tests assert this.

The default run is a warm-server experiment. For process-first observations, restart
both OWNED servers from fresh seed copies in at least6 alternating paired rounds and
retain first warmup requests separately. Health readiness may precede the first HTML
request; label page order because the second page is already warmed by the first.
Do not call this OS/filesystem cold-cache testing: the build and seed warm disk caches,
and no privileged cache flush is authorized. Browser cold/warm/revalidated navigation
is a separate functional experiment, never mixed into warm HTTP percentile samples.

## Visible footer/cache validity gate

Use an isolated browser/profile and no user cookies; provided CUA is the supported
interactive control surface. Do not start another ad-hoc CDP/Playwright driver during
this task. Keep viewport/DNA/auth/page/English equal. On each variant verify stylesheet
and bundle load, navigation200 followed by a real conditional304 on reload, and an
HTMX page/update. Record the navigation/response render metric and footer text from
the same response, using documented rounding. In the candidate, the displayed footer
must equal the finite current view metric (rounded like the footer), not an earlier
cached SSR value or the larger render total. If the response genuinely has no view
(e.g. a fragment that renders no footer), test the documented render fallback separately. Main's old
behavior is recorded as reference, not silently patched for the benchmark. Identical
rounded text on two requests is possible; require current metric mapping rather than
asserting every request's formatted number changes.

Root's actual render-time.js navigation/revalidation/HTMX contract tests passed4 cases
at preparation. New HTTP harness tests cover streamed raw footer, distinct totals,
304 empty/null-footer behavior, per-origin auth and secret-free raw evidence. These are
functional Node tests, NOT real application timings or a completed browser review.
The pending release comparison must retain all raw samples, invalid cases, executable/
asset/build identities and sample counts. Report each page/auth/cache/profile separately;
use paired distributions and repeat on an idle host before claiming a regression or win.


## Minimal PowerShell run commands after both release builds are ready

These assume the private RUN directory, deterministic fixture and setup-token file have
been prepared as above, and BASE_EXE/CANDIDATE_EXE identify immutable helper outputs.
They do not compile. HARNESS is the current harness checkout; BASE_SOURCE is pinned39f;
CANDIDATE_SOURCE is the exact candidate checkout. Use quoted absolute paths for each
variable. Keep the same shell environment for both server commands.

```powershell
$env:RBIT_PERF_SETUP_TOKEN_FILE = Join-Path $perfRunRoot 'setup-token.txt'
$env:RBIT_PERF_FIXTURE = Join-Path $perfRunRoot 'fixture'
$env:RBIT_PERF_CREDENTIALS_FILE = Join-Path $perfRunRoot 'credentials.txt'
node "$perfHarnessRoot/scripts/perf/fixture.mjs" $env:RBIT_PERF_FIXTURE
# Terminal A: fresh baseline seed server. Never substitute the live user app.
node "$perfHarnessRoot/scripts/perf/serve.mjs" --executable $perfBaselineExecutable --source-dir $perfBaselineRoot --profile release --isolated true --port 3305 --data-dir "$perfRunRoot/seed-data" --metadata "$perfRunRoot/seed-server.json"
# Terminal B, same environment:
node "$perfHarnessRoot/scripts/perf/seed.mjs" http://127.0.0.1:3305
# Stop only Terminal A's owned seed server and verify drain before copying.
Copy-Item -LiteralPath "$perfRunRoot/seed-data" -Destination "$perfRunRoot/main-data" -Recurse
Copy-Item -LiteralPath "$perfRunRoot/seed-data" -Destination "$perfRunRoot/candidate-data" -Recurse
# Terminal A: baseline comparison process.
node "$perfHarnessRoot/scripts/perf/serve.mjs" --executable $perfBaselineExecutable --source-dir $perfBaselineRoot --profile release --isolated true --port 3305 --data-dir "$perfRunRoot/main-data" --metadata "$perfRunRoot/main-server.json"
# Terminal B: candidate comparison process.
node "$perfHarnessRoot/scripts/perf/serve.mjs" --executable $perfCandidateExecutable --source-dir $perfCandidateRoot --profile release --isolated true --port 3306 --data-dir "$perfRunRoot/candidate-data" --metadata "$perfRunRoot/candidate-server.json"
# Terminal C: only after both are ready and no compiler/build/browser load remains.
Copy-Item -LiteralPath "$perfHarnessRoot/scripts/perf/footer-comparison.example.json" -Destination "$perfRunRoot/anonymous.json"
node "$perfHarnessRoot/scripts/perf/measure.mjs" "$perfRunRoot/anonymous.json"
```

Both terminals need the private setup-file environment; use distinct new metadata paths
on every restart, and new destinations that do not exist. Resolve and verify all run
paths are under the dedicated RUN directory before copying. No recursive deletion or
movement is required by this protocol. The fixture command intentionally refuses to
replace an unmarked existing directory.

For the authenticated run, derive a second config from anonymous.json. Change output
file, put credentialsFile="credentials.txt" on both variants, and choose only document
pages /, /explore, /settings/account with requireTiming/requireFooter true and
contains='action="/logout"'. Keep candidate requireViewTiming=true and baseline absent.
Write UTF8 JSON without a BOM, then invoke the same measure.mjs with that config. Keep
cold-process restart samples and browser304 validity outside both warm summaries.
Compare baseline footerSsrMs to candidate footerSsrMs, baseline renderMs to candidate
renderMs, and candidate viewMs to its own raw/footer display interval. Missing baseline
view is not a speedup and total/view ratios are not a before/after performance ratio.
