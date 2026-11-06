# Small-page footer comparison at b923e3da

This November 6, 2026 checkpoint compares pinned GitHub main
`39f932a00b7aa35b324e06a9ff2343a57d4bed76` with the optimized rewrite
`b923e3dafe958f02500db501215d64ddd0837bb1`. The same rewrite executable now serves
the managed local preview. Platform performance acceptance remains open.

## Method and provenance

Both exact executables use Rust 1.99.1/LLVM 23.1.1/MSVC, the original release profile
(fat LTO, one codegen unit, abort on panic, stripped symbols), SQLite QPSG and no
profile overrides. The candidate build completed successfully in 29m35s with clean,
frozen source and before/after source-tree, CSS and JavaScript checks. Executable
SHA-256 values are retained in the [sanitized raw evidence](footer-b923-small-pages.json).

The baseline and candidate assets differ with later UI changes; their individual CSS
and JavaScript hashes are recorded. This differs from the earlier equal-CSS
[7326e563 comparison](footer-small-pages.md). Assets were not fetched by this identity-
encoded HTTP sampler. Their inline output and middleware costs remain part of each
actual server; this experiment does not isolate one source change.

Each server starts from its own fresh copy of the same retained stopped synthetic
seed: one account/public repository/pull request, two branches, a 20,000-line file
and a 152-file comparison. Native stop proof was verified. All 28 source files were
held exclusively throughout copying, and both copies matched every SHA-256. The
candidate's migration 0060 runs before sampling. Original key material, credentials,
cookies, database files, local paths and raw launch metadata remain private.

The machine is Windows 11 build 26200, Intel Core i7-14700F, 28 logical CPUs and about
32 GiB RAM. Both isolated servers disable SSH, Actions and update checks, use log mail,
English and the default theme. They use independently copied exact release binaries,
one persistent HTTP connection per variant and alternating A/B then B/A round order.
Source checkouts were clean at launch. No Cargo/rustc/link process was observed at
sampling boundaries, and the team compiler lane was reserved; continuous desktop CPU
and process activity were not instrumented.

Seven complete valid runs retain 3,882 raw responses: 3,600 measured, 270 warmups and
12 cache primes. The initial anonymous/signed/anonymous/signed runs use 50 paired
rounds per case and five warmups. A repeated signed-in Explore difference prompted
three further 100-round signed runs; odd follow-ups reverse page order. These runs
are retained alongside the initial observations. Every response passed its status,
content and timing checks. No browser rendering, CPU/request, allocation or RSS
measurement is established by these samples.

## Footer interval

Values below are microseconds from actual HTML footers. Anonymous cells show the two
run medians. Signed-in rows pool 400 measured samples per page and variant across all
five runs; pooled values complement the per-run raw summaries.

| Viewer/page | Main median | Rewrite median | Main p95 | Rewrite p95 |
| --- | ---: | ---: | ---: | ---: |
| Anonymous home, two runs | 145 / 140 | 146.5 / 142.5 | 200.35 / 191.65 | 187.1 / 178.75 |
| Anonymous Explore, two runs | 186 / 182 | 189 / 182 | 243.75 / 227.1 | 245.9 / 227.85 |
| Anonymous login, two runs | 36 / 36 | 36.5 / 37 | 45.55 / 41.65 | 68.55 / 44 |
| Signed-in home, pooled | 233 | 234 | 303.05 | 304.05 |
| Signed-in Explore, pooled | 238 | 248.5 | 325.1 | 326.2 |
| Signed-in account, pooled | 190.5 | 194 | 259.15 | 251.15 |

All small-page medians remain below the user's approximately 500-microsecond target.
Signed-in Explore remains slower in every run: main/rewrite medians are
231/247, 232/253.5, 243.5/252, 239/247 and 235.5/248 microseconds. That is about
3.3–9.3 percent per run and 4.4 percent for the pooled medians. The first signed
account run also differs more than later runs. Follow-up samples refine the observed
size; they do not waive the earlier differences or prove their cause.

Tails vary: the final Explore run has p95 289.5/357.2 microseconds, whereas some other
runs favor the rewrite. Fifty or 100 samples per run do not support p99 or capacity
claims. Investigate the consistent Explore median difference and unstable tails
against the plan's noise/significance gates before declaring performance parity.
Localization, session orchestration, rendered buffer behavior, compiler layout and
asset differences are investigation candidates, not established explanations.

The total response interval is recorded separately as renderMs. The footer ends
before later middleware work, so these quantities must not be substituted. All
conditional cases return 304 with empty bodies and a null current HTML-footer value;
the rewrite supplies finite fresh view timing. Missing baseline view timing remains
null. The actual browser footer update still requires browser acceptance.

## Remaining acceptance

Profile and repeat signed-in Explore with controlled host load; preserve queries,
cache/privacy behavior and all features while investigating. Expand to large real
repositories, long logs, cold starts, sustained/concurrent traffic, cancellation,
CPU/allocations/RSS and styled browser interaction. This evidence belongs to b923e3da;
subsequent source-only reference, credential and listing ports require their own
Rust, release and runtime acceptance.
