# Synthetic Askama compiler screening

Source: `b4766b35385d00130bf67eab6f9e976566310a2a`. Workload: package-cold, dependency-warm `cargo check --lib`, dev profile, one job, controlled lint-only rustflags, incremental compilation disabled.

Twelve feature combinations were warmed, then six balanced serial rounds produced 72 samples. All 72 satisfy timing validity: exact selected features, rebuilt probe library, warm dependencies, stable provenance and no observed external compiler. Each mode occupies each position twice per context count. Package cleaning and observer startup are excluded from wall time.

| Contexts | Composition | n | Median ms | Min–max ms | Paired Δ vs inheritance |
| ---: | --- | ---: | ---: | --- | ---: |
| 1 | inheritance | 6 | 615.83 | 607.42–704.95 | 0.00% |
| 1 | segments | 6 | 622.20 | 612.15–691.77 | 2.02% |
| 1 | blocks | 6 | 614.13 | 606.84–634.74 | -0.39% |
| 8 | inheritance | 6 | 681.33 | 673.16–700.93 | 0.00% |
| 8 | segments | 6 | 656.83 | 644.35–695.29 | -3.49% |
| 8 | blocks | 6 | 659.02 | 647.44–687.26 | -2.61% |
| 32 | inheritance | 6 | 905.68 | 890.84–918.31 | 0.00% |
| 32 | segments | 6 | 810.01 | 790.43–914.96 | -11.07% |
| 32 | blocks | 6 | 818.56 | 800.25–847.51 | -9.20% |
| 96 | inheritance | 6 | 1488.71 | 1452.79–1615.42 | 0.00% |
| 96 | segments | 6 | 1209.10 | 1186.88–1267.94 | -18.73% |
| 96 | blocks | 6 | 1229.62 | 1209.15–1278.99 | -17.29% |

The paired difference is the median of six within-round timing ratios, rather than a ratio of aggregate medians. Six samples do not establish latency tails or statistical confidence. Measurements come from a shared development machine, not an isolated performance runner.

No memory sample is accepted for compiler comparisons from this run. The original tool marked 27/72 as memory-comparable and rejected 45, returning exit 1. Review of the raw data found that a rustup proxy can carry the same library crate arguments as the actual compiler. Crate-name/ancestry alone therefore cannot prove that a sample observed the real compiler. The original flags remain unchanged in the raw report as evidence of this measurement defect; they must not be treated as accepted memory evidence.

The observer samples Windows working set and private committed bytes every nominal 250 ms. The next measurement must independently identify the actual compiler executable, excluding its proxy. Even then, sampled values remain lower bounds: the census is non-atomic, can miss short-lived processes and does not cover linkers or true lifetime/concurrent-tree peaks. Process peaks must never be summed.

These are synthetic check/typechecking results. They do not establish release code generation, test compilation, real application compiler memory, shell-edit invalidation, HTTP rendering performance, binary size, or a production renderer winner. The next gate is representative real-page prototypes and isolated real-web build/runtime comparisons under the rendering contract.

[Raw observations](render-check-counts.jsonl) · SHA-256 `68423668d4cf6afc59b506f57365e36c80f7cf5dbe0cc96e939ced0b00340fe6`. The raw report retains toolchain, source/configuration hashes, machine metadata, exit and artifact identities, per-process observations, exclusions and validity reasons; no private absolute paths or environment values are published.
