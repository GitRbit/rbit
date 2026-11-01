# Isolated i18n presentation extraction: warm HTTP results

On 2026-11-01 the registered ten-cell p95 criterion was met for exactly baseline
`d3dc8980dd091c31ceffc61f916ca282b10ab506` (A) and isolated i18n presentation
candidate `b951eed02f7fb13994d013f71b9ce87206f59f41` (B). Every primary
family-corrected one-sided upper change bound is at most +2%; the maximum is
**+1.507087%**. The separately reported within-process sensitivity maximum is
**+1.910273%**. This is a scoped warm HTTP result for those revisions and this
workload, not certification of later Icon/Actions changes or the full modularization.

The earlier shared-UI comparison remains [inconclusive](../scripts/perf/UI-EXTRACTION.md).
These isolated i18n intervals do not repair or replace that experiment.

## Sources, workload and conditions

The [registered protocol](../scripts/perf/i18n-extraction/README.md) and analyzer
were frozen at tooling commit `274f598542b83c86d2b504ee5482a4d43d6c09d8`.
That later tooling head is not candidate B. Four independent immutable release
exports supplied A/B default and A/B core-only binaries: Rust/Cargo 1.95.0,
`aarch64-apple-darwin`, fat LTO, one codegen unit, panic abort and stripped symbols.
Default binaries were reused for the enabled-i18n profile. External dependency
records/toolchain/configuration matched; each source archive, lockfile, source
inventory, generated-asset inventory and executable was bound to its artifact.

Fresh isolated A/B Tailwind 4.3.3 builds produced identical 145,155-byte CSS
(SHA256 `eb9b42f76df3a8b9336ad313e680232735db901bea8459a5fbda4013c6662b67`),
the other 540 assets matched, and the two moved templates were byte-identical.
An older ignored CSS artifact was stale even against fresh A; it was not used as
the equality baseline.

| Profile | Build/runtime and role | Primary GETs | Frozen A/B process pairs |
| --- | --- | --- | ---: |
| default-disabled | Default features; Actions/i18n off; anonymous | `/`, repository, medium blob, compare | 160 |
| core-only | `--no-default-features`; both off; anonymous | Same four pages | 160 |
| i18n-enabled | Default features; Actions off, i18n on, default `es` | Guest language picker; password-authenticated admin pack inventory | 80 |

Repository paths were `/perf/perf-fixture`,
`/perf/perf-fixture/blob/main/src/medium.rs` (including line 3000), and
`/perf/perf-fixture/compare/main...big-change`. Enabled paths were
`/-/plugins/rbit.i18n/language` and `/-/plugins/rbit.i18n-admin/packs`, with the
pinned Spanish, `en-XA` and `ar-XB` packs. Roles, locale/direction, archive markers,
CSRF forms and expected document content were checked. Separate guest/admin cookie
jars were bound before sampling; no mutable installation endpoint was timed.

One real-signup/Git-push/pull-request/pack-install seed was cleanly closed and
checkpointed, then independently copied and content-verified before every process.
Seed refs were `main=9f169c29f4cfad32933d9fc7b9ec0e5923dde124` and
`big-change=f1cf015659853cc4c02e03389dbab98fd2729137`. No SQL privilege injection
or synthetic signed cookie was used. Separate functional malformed-upload checks
on disposable clones preserved every pack column/BLOB after shutdown; these are
functional evidence, not timed mutations.

Machine: Apple M4, ten logical CPUs, 16 GiB, Darwin 25.5.0, Node v22.23.4, AC power.
This was a shared desktop with Codex/WindowServer/Steam activity. Bounded source
reads, ignored draft/design writes, standalone rustfmt/apply-checks and compact CI
metadata activity occurred and were disclosed. There were no concurrent Cargo
builds/tests, asset generation, fuzzing, extra fixture servers or measured-source/
harness/manifest edits. Power/thermal/load evidence consists of snapshots, not
continuous idle or temperature attestation. OS caches were deliberately warm.

## Prospective sample size and inference

A same-binary pilot used 12 A/A plus 12 B/B fresh-process pairs per profile (72
pairs total), never included in A/B effects. Only that pilot selected N from
80/160/320 using the registered 1.25-percentage-point precision target. N was frozen
as 160/160/80 at 12:54:15 UTC, before confirmation began at 12:56:31 UTC; no profile
was labeled precision-limited. No A/B outcome-based extension or favorable stopping
was used.

Confirmation ran 12:56:31–13:54:56 UTC: 400 A/B and 90 control pairs (490 pairs,
980 clean unforced launcher exits). Each profile had ten A/A before, ten evenly
interleaved B/B and ten A/A after. Port/process assignment alternated; AB/BA request
order alternated by round. Every page/arm/pair used 20 warmups followed by exactly
100 measured rounds on serial persistent identity-encoded HTTP connections.
All **417,600** confirmation records are retained: **348,000 measured** and
**69,600 predefined warmups**. Warmups alone were excluded from inference; no slow
valid samples were deleted, no failed blocks replaced, and no retries or exclusions
were used to improve the result. Registered provenance/protocol/content/role/order
and clean-shutdown checks passed for all 490 pairs; postflight found the seed
unchanged, ports free and source still frozen.

For each page/profile, the estimand is
`100 × (Q95(all B measured requests) / Q95(all A measured requests) − 1)`.
It is a pooled-request quantile ratio, not an average of per-process p95 ratios.
The primary bootstrap resamples whole paired process clusters, preserving paired
arms and their rounds. It uses 20,000 draws, seed `20261101-i18n-frozen-v1`, linear
`(n−1)q` quantiles, Python 3.14.7/NumPy 2.5.4 and the registered SHA256-derived
Python Random seed. Ordinary intervals are two-sided 95%. The primary family is
exactly ten p95 cells: Bonferroni one-sided alpha `.05/10=.005`, hence a `.995`
upper percentile per cell. Every corrected upper must be ≤+2%.

Sensitivity additionally resamples paired circular blocks of ten contiguous rounds
inside each selected process cluster. Its upper bounds are shown at the same tail
level, descriptively; they are not a second joint-family certification. Bootstrap
coverage assumes adequate exchangeable process-pair sampling. Controls stay
separate and are never subtracted. `totalMs` includes client response reading,
semantic stream checks and hashing; it is not pure server-render time or browser
navigation latency.

## Primary p95 results

Times are milliseconds; changes and intervals are percentages, B relative to A.
Upper columns are one-sided corrected primary and descriptive sensitivity bounds.
Values are rounded here; the hashed analysis retains full precision.

| Profile / page | A p95 ms | B p95 ms | Change % | Ordinary 95% CI % | Corrected upper % | Sensitivity upper % |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| default-disabled / home | 0.628463 | 0.632417 | +0.629130 | [-0.092883, +1.300847] | +1.507087 | +1.910273 |
| default-disabled / repository | 1.077962 | 1.078502 | +0.050085 | [-0.378958, +0.502105] | +0.647603 | +0.838610 |
| default-disabled / medium | 2.162131 | 2.178860 | +0.773720 | [+0.513964, +1.122583] | +1.219943 | +1.456264 |
| default-disabled / compare | 27.688508 | 27.242423 | -1.611084 | [-1.719401, -1.483211] | -1.451195 | -1.398358 |
| core-only / home | 0.629000 | 0.629500 | +0.079491 | [-0.599434, +0.776795] | +0.974752 | +1.381391 |
| core-only / repository | 1.073299 | 1.076421 | +0.290860 | [-0.108390, +0.687965] | +0.805667 | +0.942930 |
| core-only / medium | 2.170500 | 2.175211 | +0.217049 | [-0.111271, +0.562777] | +0.659362 | +0.855809 |
| core-only / compare | 27.035361 | 27.060179 | +0.091800 | [-0.011164, +0.204680] | +0.239234 | +0.299311 |
| i18n-enabled / language-picker | 0.633004 | 0.628002 | -0.790216 | [-1.210939, -0.350099] | -0.223281 | +0.000502 |
| i18n-enabled / language-packs | 0.795590 | 0.791713 | -0.487324 | [-1.124616, +0.036902] | +0.241313 | +0.501417 |

The default-disabled medium page has a positive measured change with an ordinary
interval above zero; it still satisfies the declared +2% bound. A gate pass does
not mean every page improved or establish zero overhead.

## Same-binary controls, reported independently

Each entry is p95 change % [ordinary two-sided 95% CI %], ten process pairs per
phase/profile. These are all thirty page/profile/phase control cells. A/A before
and after compare independent A processes; B/B compares independent B processes.
They are not corrections to the A/B table and are not required to pass a hidden
zero-effect gate. Directional variability and intervals excluding zero show why
signed speedup and a zero-noise laboratory claim are unwarranted.

| Profile / page | A/A before | B/B interleaved | A/A after |
| --- | ---: | ---: | ---: |
| default-disabled / home | +1.193295 [-2.243777, +2.457056] | +0.036052 [-2.457883, +2.744283] | +2.019314 [-0.904549, +4.505174] |
| default-disabled / repository | +0.722090 [+0.121635, +2.761552] | +1.141788 [+0.038372, +2.799832] | -1.308327 [-2.329528, +0.272044] |
| default-disabled / medium | -1.282191 [-3.171355, +0.349971] | -0.295212 [-1.057974, +0.993820] | -0.496718 [-1.352918, +0.434453] |
| default-disabled / compare | -0.416967 [-0.640184, +0.089052] | +0.137360 [-0.171716, +0.418606] | -0.070441 [-0.611481, +0.503785] |
| core-only / home | -0.436035 [-2.808102, +1.673674] | -0.898966 [-3.426863, +0.905863] | +0.415465 [-2.475345, +3.221344] |
| core-only / repository | +0.893603 [-0.551365, +1.969953] | +0.427496 [-1.337015, +2.456002] | +0.875599 [-0.552168, +2.358262] |
| core-only / medium | +0.446348 [-0.981143, +1.813234] | +0.768611 [-0.689967, +2.180868] | +0.429186 [-0.690624, +1.287537] |
| core-only / compare | +0.065104 [-0.418274, +0.447471] | +0.218598 [-0.315898, +0.533550] | -0.117422 [-0.459373, +0.060021] |
| i18n-enabled / language-picker | -0.730231 [-2.306554, +0.979909] | -0.200618 [-0.799703, +0.426149] | -2.468356 [-3.500967, -0.223353] |
| i18n-enabled / language-packs | -1.069658 [-3.028311, +0.005433] | -1.505736 [-2.751215, -0.204760] | -0.045371 [-1.251194, +1.319242] |

For example, default home A/A after is positive with a wide interval, while enabled
picker A/A after and enabled packs B/B show negative directional intervals. These
observations remain visible; they are neither deleted nor used to center A/B.

## Descriptive p50

These pooled medians are descriptive, outside the primary ten-cell p95 family.

| Profile / page | A p50 ms | B p50 ms | Change % |
| --- | ---: | ---: | ---: |
| default-disabled / home | 0.542312 | 0.543792 | +0.272813 |
| default-disabled / repository | 0.982084 | 0.984083 | +0.203598 |
| default-disabled / medium | 1.979584 | 1.987916 | +0.420897 |
| default-disabled / compare | 27.000583 | 26.542917 | -1.695024 |
| core-only / home | 0.542166 | 0.541917 | -0.046019 |
| core-only / repository | 0.981292 | 0.980791 | -0.051004 |
| core-only / medium | 1.976875 | 1.978063 | +0.060070 |
| core-only / compare | 26.380125 | 26.380709 | +0.002214 |
| i18n-enabled / language-picker | 0.561937 | 0.563021 | +0.192815 |
| i18n-enabled / language-packs | 0.695271 | 0.694708 | -0.080976 |

## Evidence and reproduction

Operational evidence remains outside Git under
`target/i18n-web-extraction/experiment-v1/timing-registration-v1/`; no private
credentials, cookie jars or raw setup logs are published. The final acceptance
and independent audits retain artifact/input inventories, raw JSON/JSONL hashes,
chronology, role/content checks and process ownership/clean-exit records. Hashes
bind retained local evidence; they do not independently attest compiler execution,
continuous machine conditions or a new run on another machine.

| Record | SHA256 |
| --- | --- |
| `experiment.timing.json` | `901a8eb7d2bffe94e86efaa236fced265964bd6055f39a218dc59d00d568e52e` |
| `freeze.json` | `e8a86bce2a9ecce2fd25d90d6d2c0ef7c8328e909545dd20637320c4dd5c2d91` |
| `pilot-analysis.json` | `deec76b89144240661e7f9ec3f32713ef91db0f09bf5d169deb51c8aceb3b40f` |
| `confirmatory-analysis.json` | `c4bc213417dbbd5fd9bbbec64bb9d7906920aacb01e09abbb5a57f1f2ecb443d` |
| `root-final-acceptance.json` | `69981ddd611f00075b23b2b48388d9b20cb149371131d9f91a111b7dc00ab48c` |
| `confirmatory-plan-v1/protocol.json` | `2fd1ec3b28687cf4f64455f4a847d83e522b5a464e813d1e94d50832fc4de965` |
| `independent-pilot-audit-v1.json` | `00bbc5e459c57403e92887f380c611c97f4cf5be98dde22d418951d529ea6cbe` |
| `independent-confirmation-audit-v1.json` | `2a533e0b62518107e3307c612f55cd8b0cb8125c138a6be09c91004498a61a46` |
| `independent-confirmation-analysis-audit-v1.json` | `30abcc86c1f19d6ddfeab5c5e528bafac6cdcb07ec33275642d8d62fbea08484` |
| `confirmation-postflight.json` | `cf106d03997564905e38b44642634d8d45ab4ddec178444bbd47713a10460bae` |
| `during-confirmation-source-preparation.json` | `09c714fa4fbcd4a21dc6f94d6e35f1522de78951657d849250ce026664a9184e` |
| Analyzer source | `d7f11dd1a327de5dd8ac5e3936def45d4d52ee7b160a0d43df78ed92b4c96ed8` |
| Planner source | `3c32121dbc8d33b29a367048334e4387652a3b3ffce8fd928c3df6ce8274edb4` |
| Common seed inventory | `c53f1967c9c247a785d1bb813c970f02d5915475a22d0335694ae1fcd29e9879` |

| Artifact | Canonical source archive SHA256 | Executable SHA256 |
| --- | --- | --- |
| A-default | `824d57c91625174d6b194510ab794fcd7f6d4a393bf841e7a90b60ae99971c36` | `96d599cdc9908ea5d048dadcfc98d2eaffbf88276d4b6430b6089e0bfc2f44fe` |
| B-default | `65e0b9282228d5753f743e6e5f408bb85ba5e4cb96d0365f7d28347302bab3e0` | `58ed092c69e4ca984e740108e32c2f5d310d527c241c493b460479f36e857c32` |
| A-core | `824d57c91625174d6b194510ab794fcd7f6d4a393bf841e7a90b60ae99971c36` | `d99565a0689de6fc551fe10baba6eb87c5e152e560c7109e88ce864555639665` |
| B-core | `65e0b9282228d5753f743e6e5f408bb85ba5e4cb96d0365f7d28347302bab3e0` | `0e752a5b5aceabacfea89b34c085f5784cd528ecfe04f3b6dd8aa3760f88e566` |

Use the committed protocol's source-export/build/fixture/smoke/pilot/freeze steps
for a new experiment; preserve the new run separately rather than overwrite this
record. Frozen tooling templates require operator-supplied local paths, reviewed
owned setup and immutable evidence. To reproduce this analysis from the retained
registered inputs, use the recorded Python/NumPy environment and run the committed
analyzer after all servers stop, with a new output path:

```sh
"$PYTHON" scripts/perf/i18n-extraction/analysis.py confirmatory \
  "$RUN/confirmatory-plan-v1/commands.json" \
  --manifest "$RUN/experiment.timing.json" \
  --output "$RUN/reanalysis-new.json" --engine numpy
```

`RUN` identifies the retained registration directory and `PYTHON` the recorded
analysis environment; neither contains credentials in argv/environment. The analyzer
revalidates frozen hashes and evidence. Rebuilding from later HEAD, mutating
registered inputs or replacing the fixture is a new experiment, not reproduction
of these exact artifacts.

Hosted functional validation associated with the frozen tooling head reported all
11 checks green, including 2,523 Linux and 2,504 native Windows tests. Those are CI
functional counts, not latency samples or additional validation of candidate B's
performance. Independent pilot, confirmation-record and final-analysis audits
preceded root acceptance on 2026-11-01 at 14:05:44 UTC.

This report establishes only the registered isolated-i18n warm HTTP bound. It makes
no allocation-count, startup, disk-cold, browser responsiveness, throughput/load,
other-platform, other-locale/provider, later Icon/Actions/RouteBundle, native-worker
or sandbox claim. Full modularization and the earlier shared-UI gate remain open.
