# Synthetic Askama runtime screening

[Raw samples and build identity](render-runtime-count8.json) record a first runtime
screen of the development-only rendering probe at source `e8e5dba0`. Each of six
rounds ran all three variants in a balanced order, using the same release executable,
fixture values and reused output buffer. Each sample rendered 800,000 documents after
1,000 warmup rounds. All 18 samples produced the same byte count.

| Composition | Median ns/document | Minimum | Maximum |
| --- | ---: | ---: | ---: |
| Inherited document | 200.10 | 198.34 | 204.41 |
| Shared shell segments | 185.86 | 185.25 | 187.25 |
| Inherited content block with shared shell | 186.95 | 185.29 | 188.01 |

These values describe a small synthetic render loop on a shared Windows desktop,
not HTTP latency or the real platform. The executable contains eight distinct
contexts for each variant. All three modes were compiled together; the run does not
isolate their feature-specific compiler costs. No compiler-memory, allocation-count,
production speed or statistical tail-latency conclusion follows from these samples.

The result supports continuing the experiment: shared-shell composition did not show
a synthetic runtime regression in this setup. The next gate remains isolated,
repeated cold-package compilation at 1/8/32/96 contexts, including measured compiler
memory and shell-edit rebuilds, followed by representative real-page correctness and
runtime comparisons. The [adoption gates](../../rendering-compilation.md) still apply.

## Reproduction

Build through the shared lane:

```text
node scripts/workspace.mjs build --package rbit-render-probe --release --no-default-features --features inheritance,segments,blocks,count-8 --locked
```

Use the exact copied executable returned by that invocation through the managed
runtime launcher. For each mode, pass:

```text
--mode MODE --count 8 --iterations 100000 --warmup 1000 --label "count-8 release synthetic screen"
```

Run orders A/B/C, C/B/A, B/A/C, C/A/B, A/C/B and B/C/A, where A is `inheritance`, B is
`segments` and C is `blocks`. Complete compilation before sampling. Save every JSON
record and its round, source/lockfile/executable hashes, toolchain and machine
identity. The recorded elapsed time excludes process startup, warmup and JSON output;
it includes rendering, buffer clearing, byte counting and black-box observations.
