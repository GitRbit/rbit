# Isolated i18n experiment, prospective ten-cell version 2

Prospective experiment tooling. No pilot, confirmatory measurement or performance
result is implied by this protocol or its unit tests. Unit tests use synthetic
samples, inert mock launchers and small mock HTTP servers.

The ignored ten-cell source predecessor and its hash manifest remain under
`target/i18n-web-extraction/perf-draft/`. Its `superseded-8-cell-v1/` archive
preserves the older unmeasured eight-cell draft. Operational build/fixture records,
failed asset preparation and analysis-environment evidence remain outside Git;
none is relabeled as a tracked-tooling execution. Before pilot data, commit the
reviewed tooling and run its own real-host smoke with the exact current hashes.

A: `d3dc8980dd091c31ceffc61f916ca282b10ab506`, archive SHA-256
`824d57c91625174d6b194510ab794fcd7f6d4a393bf841e7a90b60ae99971c36`.
B: `b951eed02f7fb13994d013f71b9ce87206f59f41`, archive SHA-256
`65e0b9282228d5753f743e6e5f408bb85ba5e4cb96d0365f7d28347302bab3e0`.
Generate each canonical tar from its exact Git revision with
`git archive --format=tar REVISION`, writing to a new file with exclusive creation,
and verify its registered SHA-256 before extraction. Keep generated assets, source
inventories and build records alongside the operational manifest outside Git.
Canonical tar files and source/lock/tree evidence are under
`target/i18n-web-extraction/`, including `perf-baseline.json`. Later Actions/docs
heads and their CI runs are not B. Do not build from a later working tree and label
it with B's revision. The 0075cec-to-3b9 shared-UI experiment remains inconclusive
and cannot be closed by adding this slice's intervals to its result.

## Registered workload

The v2 workload pins these roles/paths/settings, while deriving family size and
alpha from the frozen profiles/pages rather than an analysis constant:

| Profile | Compile features | Runtime | Primary pages |
| --- | --- | --- | --- |
| default-disabled | default | Actions/i18n disabled | `/`, repository, medium blob, compare |
| core-only | `--no-default-features` | both disabled | the same four pages |
| i18n-enabled | default | Actions disabled, i18n enabled, default locale `es` | public `/-/plugins/rbit.i18n/language`; admin `/-/plugins/rbit.i18n-admin/packs` |

The repository paths are `/perf/perf-fixture`,
`/perf/perf-fixture/blob/main/src/medium.rs`, and
`/perf/perf-fixture/compare/main...big-change`. Ten pooled-request p95 cells are
primary. v2 cannot silently omit the enabled UI, replace admin with anonymous,
remove its required Spanish/archive markers or shrink the correction. A different
workload requires a separately reviewed prospective protocol before data.

All requests use identity encoding and serial persistent connections, 20 warmup
rounds then exactly 100 measured rounds per page/arm/process pair. AB/BA request
order alternates by round; process launch/port assignment alternates by phase pair.
Profiles remain separate. Guest picker and administrator inventory use distinct
private cookie jars. Login and guest-binding bootstrap occur before sampling;
postflight GET checks occur after the measured matrix. No mutable install endpoint
is part of latency sampling. See `ENABLED-FIXTURE.md` for actual archive/auth/locale
prerequisites and functional-only installation/error checks.

## Reproducible source and release builds

Copy `experiment.template.json` and fill all null paths/hashes. Use absolute paths,
private directories and immutable source exports. Each arm/profile artifact follows
`artifact-record.template.json`; default-disabled and enabled-i18n may reuse the
same default-feature binary. A and B must have independent export/target directories.
Use production Rust/Cargo 1.95.0, fat LTO, one codegen unit, `panic=abort` and
`strip=symbols`. Rust 1.99 lint artifacts are not performance artifacts.

Build argv is `cargo build --release --locked -p rbit-server`, plus
`--no-default-features` only for core-only. Record `CARGO_TARGET_DIR` separately in
`outputEnvironment` and bind it to the artifact target directory. Record full
compiler/Cargo versions, target, compiler-affecting environment, Cargo config,
lock/archive/source/assets/binary hashes and build logs. Capture absence as well as
presence of flags/config; retained evidence does not independently prove what the
compiler consumed. The planner/analyzer check the pinned archive and lock identities,
all external Cargo.lock package records, actual source inventory and binary hashes,
and matching compiler/config/assets within each comparison.

File inventories are exactly `{"files":{"relative/path":"sha256",...}}`. Reject
symlinks, inventory every regular file, and store evidence outside its own inventory.
`plan.inventory(path)` supplies this read-only function. Source inventory describes
the immutable archive plus the documented deterministic generated-asset overlay,
not a build tree with mutable node_modules/target caches. Fresh isolated A/B
Tailwind 4.3.3 outputs matched: CSS 145,155 bytes, SHA-256
`eb9b42f76df3a8b9336ad313e680232735db901bea8459a5fbda4013c6662b67`;
other 540 assets unchanged and two moved templates byte-identical. Recheck actual
build inputs; the earlier stale ignored CSS is not the A baseline.

One closed/checkpointed deterministic dataset supplies every process clone. Pin its
complete content inventory in the manifest; preserve permission/ownership and Git
refs/object identity separately. Never copy a live SQLite/redb directory. Old run03
did not independently hash seed copies; this executor verifies every copy before
launch and saves `<metadata>.seed-before-start.json`. Persisted administrator desired
state must remain absent so runtime config can select disabled versus enabled
without changing seed bytes. No queued workers or unrelated package operations.

Frozen `semanticChecksRecord` and `machineAndIdleConditionsRecord` describe expected
content, machine/configuration and idle/thermal prerequisites plus initial reviewed
evidence. Their byte hashes are pinned in command plans and revalidated. Do not
append post-run observations to them; actual bootstrap/postflight evidence is bound
into each sampler report, and additional manual observations stay in separate
immutable files. Hashes/flags are manual evidence, not automatic idle or complete DOM
correctness proof. Check repository/ref/content/hunk identity, metadata and security
headers outside the timed client path. Never save raw cookie/password/CSRF tokens in
logs/results; the sampler emits hashes, redacted headers and semantic outcomes only.

No concurrent builds/fuzzing/maintenance during timing. Record power, memory,
thermal/idle conditions and interruptions. OS caches are deliberately warm: no disk
cache purge or disk-cold/startup claim. `totalMs` includes client body reading,
semantic stream checks and hashing, not pure server render or browser navigation.

## Planner, owned setup and real-host smoke

`plan.py` generates configs/argv only. `execute.py` runs a reviewed plan, never builds.
It probes both fixed ports before launch, creates explicit owner records, rechecks
launcher liveness after health success, preserves copied-seed evidence and settles
both owned groups in `finally`. Readiness is capped at 30 seconds, sampler at 120,
and shutdown at 30. A single interruption while waiting for shutdown force-kills/
reaps that owner, still attempts the other, and invalidates the attempt. No forced
kill/nonzero launcher can be hidden behind a clean child exit record.

This assumes a trusted idle host with exclusive local ports. Probe-close-launch and
PID liveness are not race-proof against hostile local processes/PID reuse. Functional
setup/smoke requires an explicitly owned live launcher record, matching metadata
path/port and no exit record; do not authenticate to an arbitrary existing listener.
`owned-serve.py` supplies that wrapper for initial fixture setup. Both CLI launchers
install SIGTERM cleanup before startup; SIGKILL cannot be unwound. Fixture finalization
requires an unforced zero launcher exit and a clean actual child exit record. Authentication
secrets never enter argv or process environment.

Fill `samplerPath` with the absolute path to this directory's `harness/measure.mjs`, register its
capability/test evidence, and choose a dedicated `realHostSmokeEvidence` output
path. Templates leave local paths null; copy and fill them in an external run
manifest rather than editing the tracked template. A `--phase smoke` plan launches only enabled A/B and runs `harness/smoke.mjs`,
not the measured loop. It needs the real prepared seed and release binaries but no
pilot/freeze. Evidence binds the exact enabled profile spec, seed identity, A/B
binaries and actual guest/admin Spanish/archive semantics. Anonymous/home-only smoke
cannot satisfy it. After it succeeds and both owned launchers shut down cleanly,
register the result SHA-256 and the finalized `protocol.json` path/hash as
`realHostSmokeProtocol`/`realHostSmokeProtocolSha256`. The planner also checks its
immutable commands, current smoke-validator/executor/planner hashes, source/binary/dataset identity
and clean child exits; a semantic success followed by failed or forced cleanup
cannot satisfy this prerequisite. Freeze these records before planning pilots.
A successful, finalized real-host smoke is required before pilot data.

```sh
python3 scripts/perf/i18n-extraction/plan.py /ABS/experiment.json --phase smoke --output /ABS/new-smoke-plan
python3 scripts/perf/i18n-extraction/execute.py /ABS/new-smoke-plan/commands.json --manifest /ABS/experiment.json --execute-reviewed-plan
```

## Pilot and immutable sample-size freeze

Each profile gets 12 A/A and 12 B/B independent fresh-process pairs, interleaved.
Pilot measurements never enter A/B effects. Same-binary controls compare two
independent processes of the same exact executable, with the profile's runtime state.

Choose the smallest N in {80,160,320} using only pilot variance: for each candidate
N, draw N whole pilot process-pair clusters, obtain pooled p95 ratios, and compute
max(median-to-alpha-tail distance) across both same-binary groups and every page in
that profile. Require estimated uncertainty <=1.25 percentage points. The 12-pair
empirical pilot is uncertain, not a precision guarantee. If no N qualifies, cap at
320 with `precisionLimited=true`, or improve conditions and start a new pilot before
confirmation. Never stop or extend because an A/B estimate looks favorable.

Set the final freeze path in the manifest before planning the pilot. After any smoke
hash updates, do not mutate the registered manifest. For each profile confirmation
is ten A/A before, N A/B, ten evenly interleaved B/B, ten A/A after. At the maximum:
72 pilot + 1,050 confirmation pairs = 1,122 process pairs/2,244 server processes.
Counts are bounded; this does not promise a wall-clock duration or a gate pass.

```sh
python3 scripts/perf/i18n-extraction/plan.py /ABS/experiment.json --phase pilot --output /ABS/new-pilot-plan
python3 scripts/perf/i18n-extraction/execute.py /ABS/new-pilot-plan/commands.json --manifest /ABS/experiment.json --execute-reviewed-plan
python3 scripts/perf/i18n-extraction/analysis.py pilot /ABS/new-pilot-plan/commands.json --manifest /ABS/experiment.json --output /ABS/pilot-analysis.json --freeze-output /ABS/freeze.json --engine python
python3 scripts/perf/i18n-extraction/plan.py /ABS/experiment.json --phase confirmatory --output /ABS/new-confirm-plan
python3 scripts/perf/i18n-extraction/execute.py /ABS/new-confirm-plan/commands.json --manifest /ABS/experiment.json --execute-reviewed-plan
python3 scripts/perf/i18n-extraction/analysis.py confirmatory /ABS/new-confirm-plan/commands.json --manifest /ABS/experiment.json --output /ABS/final-analysis.json --engine python
```

Analyze only after both servers stop. Offline bootstrapping can be expensive; restore
predeclared idle/thermal conditions before subsequent timing. Outputs use exclusive
creation and retain every attempted run. Any transport/protocol/semantic/provenance
failure invalidates the attempt; no deletion of slow valid samples or automatic
replacement blocks. Only the predefined warmups are excluded from statistics.

## Statistical definition and interpretation

For q=.50/.95 within one page/profile:

`ratio_q = Qq(union of all B process requests) / Qq(union of all A process requests)`

`effectPercent_q = 100 * (ratio_q - 1)`

This is a pooled-request quantile ratio. It is **not** a mean/median of per-process
relative quantiles. Whole paired processes are bootstrap units: resample intact
clusters, preserving both arms and round structure. Per-process p95 deltas are a
separate descriptive distribution. Use >=20,000 seeded draws, linear `(n-1)*q`
quantiles; report p50/p95 ratios, ordinary two-sided 95% intervals and per-process
spread. The within-pair sensitivity resamples circular contiguous blocks of ten
paired rounds inside selected process clusters. Report both; never choose the
more favorable interval. Controls remain separate, never subtracted as a correction.

Family size is derived from the registered 4+4+2 cells: one-sided alpha=.05/10=.005,
upper percentile .995 per primary p95 cell. A simultaneous scoped <=2% interpretation
requires every corrected upper ratio <=1.02, valid complete evidence, reviewed
sensitivity/controls and stable conditions. Bootstrap coverage assumes an adequate,
exchangeable process-pair sample; no universal/hardware or other-locale claim follows.
P50/sensitivity are descriptive, not an additional joint twenty-cell claim. An
upper bound above2% is inconclusive for the bound unless the interval supports a
regression. Nonsignificance against zero and point estimates alone are not passes.

Analyzer validation requires exact JSON/JSONL agreement, full paired matrices/order,
identity encoding, role/locale/cookie stability, immutable postflight checks, unique
data directories, correct build/harness/fixture/machine, clean launchers AND child
exit records, complete executor protocol and fixed phase order. UTC chronology binds
pilot shutdown/analysis <= freeze <= every confirm process start; sampler/request/
postflight times lie inside both live servers. Final provenance hashes the exact
freeze/pilot/protocol/manual prerequisites/source evidence. This is retained local
evidence, not independent compiler attestation or proof an operator never saw an
external A/B run. No claim currently exists because actual runs are pending.

## Synthetic validation

```sh
python3 scripts/perf/i18n-extraction/test.py
node --test scripts/perf/i18n-extraction/harness/contexts.test.mjs
```

The tracked-path suite passed all 38 Python tests in a private Python
3.14.7/NumPy 2.5.4 environment. A separate standard-library run passed 37 tests
and skipped the optional NumPy parity test. All eight Node cookie/protocol tests
passed. Local evidence is under `target/i18n-perf-promotion/`; the selected analysis
environment is recorded in `target/i18n-web-extraction/analysis-environment.json`.
These synthetic/mock checks do not execute Rbit or measure latency. CI runs the
standard-library suite without installing NumPy, so the optional parity test may
skip; run it in the selected analysis environment before freezing that environment.

Python 3.11+ is required. The standard-library weighted-quantile engine is tested;
NumPy optionally accelerates bounded 32-replicate CDF batches with identical Python
seeded draws. When NumPy is absent, its parity test explicitly skips. Validate that test in the
chosen analysis environment before using it/freeze; record backend and versions.
The standard-library CI entry installs no packages. Tests include 20,000 synthetic paired
draws, known proportional effects/shared process noise, process uncertainty despite
more requests, contiguous sensitivity, corrected-tail refusal, prospective family,
pilot-only N/cap, producer-schema files, immutable freeze/UTC/protocol/cookie evidence,
and forced shutdown on mock interruptions. Mock HTTP proves cookie isolation,
no redirect replay/leaks and semantic failures; it does not prove real Rbit login,
archive setup or runtime performance. The mandatory real-host smoke closes that
functional prerequisite only when actually run.
