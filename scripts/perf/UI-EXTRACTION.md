# Shared UI extraction HTTP experiment

Measured 2026-11-01 UTC. This compares only the shared presentation extraction:
`0075cec5e8cea71149f582ffdc31fceb0be68b76` against
`3b9f263d308690891e67480659eff9ed33be7602`. It does not certify subsequent package
changes or the complete runtime-plugin architecture.

The two-percent core-page p95 gate remains **inconclusive**. Repository-page
uncertainty exceeds the bound; no statistically established regression above two
percent was observed. Same-binary controls also show material process variation.

## Method

- Apple M4, 10 logical CPUs, 16 GiB RAM, Darwin 25.5.0, Node 22.23.4.
- Rust/Cargo 1.95.0; production release profile: fat LTO, one codegen unit, abort
  panics, stripped symbols. Separate exact source exports and build directories;
  generated assets match. Actions and i18n were disabled at runtime.
- The deterministic fixture was seeded once, then copied from the quiescent data
  directory into a fresh directory for every server. Anonymous serial loopback
  HTTP, identity encoding, persistent connections, warm OS caches.
- Forty independent comparison process pairs; ten baseline/baseline control pairs
  before and ten after. Port assignment and launch order alternate across pairs.
  Each pair has 20 warmup and 50 measured rounds per page and variant; A/B then B/A
  request order alternates by round. All 60 blocks and 120 process exits validated.
- Each comparison page has 2,000 measured responses per variant. Confidence
  intervals use 2,500 hierarchical paired bootstrap replicates (seed 2026110103):
  resample process blocks, then paired rounds within each selected block.
  Quantiles interpolate at `(n-1)*q`; two-sided percentile intervals have no
  multiplicity correction. Controls are reported independently, never subtracted.

## Warm total HTTP latency

| Page | Baseline p50/p95, ms | Candidate p50/p95, ms | p95 change | 95% interval |
| --- | ---: | ---: | ---: | ---: |
| home | 0.5557 / 0.6667 | 0.5578 / 0.6429 | -3.57% | [-5.82%, -1.09%] |
| repository | 0.9631 / 1.0802 | 0.9647 / 1.0867 | +0.60% | [-1.49%, +4.29%] |
| medium | 1.6992 / 1.9122 | 1.6990 / 1.8966 | -0.82% | [-2.38%, +1.03%] |
| compare | 26.2972 / 27.2505 | 26.3405 / 27.2218 | -0.11% | [-0.78%, +0.42%] |

The repository control p95 changes were −0.45% before (95% interval −2.83% to
+2.14%), +3.01% after (−0.68% to +9.40%), and +0.99% combined (−0.86% to +3.08%).
The comparison does not establish equivalence of every render/header metric.
No repeated request, invalid response or failed block was silently discarded.

## Other evidence and limitations

An earlier fixed-process experiment had sub-one-percent pooled changes, but did
not represent process-to-process variation. It remains separate evidence. The
initial attempt had an invalid prefix-only HTML-marker check; all 880 affected
samples were retained and excluded, and the sampler was fixed in `133aa4a`.

A separate restart-only experiment validated all 120 responses: 30 fresh starts
per variant for each of the repository and medium-file pages, with warm OS caches.
Baseline/candidate p95 was 5.654/5.530 ms for repository and 102.824/103.349 ms for
medium-file. Thirty samples per arm are exploratory and do not certify a p95
budget. These are not disk-cold or browser measurements.

The source audit found no new per-request SQL, filesystem or plugin subprocess
operations in this presentation extraction. This experiment measures one machine
and session, not loaded production traffic or future external workers.

## Reproduction and retained evidence

Use the [HTTP comparison harness](README.md#paired-http-measurements) with
the same exact source revisions, locked dependencies, release profile and
deterministic fixture. Build and generate assets separately; seed once and stop
the server before copying its data. Restart both variants for each independent
block, balance ports and launch/request order, exclude warmups and analyze process
blocks as the independent units. Do not run builds or fuzzing during sampling.

Ignored `target/perf/ui-extraction/` retains source archives, build records, hashes,
server logs and complete experiment drivers. `run03/protocol.json`, per-block
JSON/JSONL, `summary.py` and `summary.json` retain the complete schedule, provenance,
raw responses, analysis and controls; `run02/restart-cold.json` retains restart
samples. The analysis checks binary hashes, source/build metadata, unchanged
harness/fixture/machine identity, valid paired responses and clean server exits.
These are retained local records, not independently attested results; seed-copy
contents were not independently hashed by the analysis.

| Binary | SHA256 |
| --- | --- |
| baseline | `7724f5b23d00bcb9eb1a4d346d067beab3addad0d1bb201181ebb349b0a8bc5f` |
| candidate | `d3c8902f140fec6f3f5307ba93647f6bb362aca47414dbb3f66d09bb81eb0e83` |
