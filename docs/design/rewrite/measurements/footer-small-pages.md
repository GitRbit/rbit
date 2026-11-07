# Small-page footer comparison

This November 6, 2026 experiment compares pinned GitHub main
`39f932a00b7aa35b324e06a9ff2343a57d4bed76` with rewrite checkpoint
`7326e5635d4aee0acfa3e6ca064751ed6c340710`. It addresses the footer timing
comparison; it does not complete the platform performance gates.

## Method and provenance

Both exact executables were built with Rust 1.99.1, LLVM 23.1.1, the original release
profile (fat LTO, one codegen unit, abort-on-panic, stripped symbols), and SQLite QPSG.
No release-profile environment overrides were present. The generated CSS hashes
match. Each source checkout was clean at launch, and the candidate checkout was
frozen while other rewrite work continued separately.

Machine: Windows 11 build 26200, Intel Core i7-14700F, 28 logical CPUs, approximately
32 GiB RAM. Each server used isolated data, default English/theme settings, disabled
SSH/Actions/update checks and log-only mail. HTTP used identity encoding and one
persistent connection per variant. These are server/HTTP observations, not browser
paint, compressed browser transfers or user-input latency.

The deterministic fixture has one synthetic account, repository and pull request,
two branches, a 20,000-line source file and a 152-file comparison. Only the small
pages below were measured in this experiment. The seed server was stopped through
its owned process-tree control before copying data. All 28 files matched by SHA-256
in both copies. Credentials, cookies, setup tokens, database files, launch paths and
local branch inventories remain private.

The [paired protocol](../../../../scripts/perf/FOOTER-COMPARISON.md) used five warmups
and 50 alternating paired rounds per case, in this order: anonymous run 1,
authenticated run 1, anonymous run 2, authenticated run 2. Every run was valid;
all 1,800 measured responses passed the status/content/timing checks. Anonymous runs
also retain the six initial cache-prime responses each. No Cargo/compiler process
was present at sampling; overall desktop background utilization was not instrumented.

The [sanitized raw evidence](footer-small-pages.json) includes each measured and
warmup sample, cache primes, source/executable hashes and separate timing fields.
It omits private configuration, request headers and raw launch records.

## Common SSR footer interval

Values are microseconds. Each cell shows run 1 / run 2. The common comparison is
`footerSsrMs`, parsed from each actual HTML footer on both revisions. Its original
human-readable precision is preserved; medians may fall between integer samples.

| Viewer/page | Main median | Rewrite median | Main p95 | Rewrite p95 |
| --- | ---: | ---: | ---: | ---: |
| Anonymous home | 176.5 / 145.0 | 183.5 / 144.5 | 245.2 / 191.7 | 255.6 / 204.7 |
| Anonymous Explore | 231.5 / 191.5 | 247.5 / 192.5 | 313.4 / 238.0 | 320.9 / 245.6 |
| Anonymous login | 44.5 / 37.0 | 42.0 / 38.0 | 90.1 / 44.6 | 85.2 / 43.6 |
| Signed-in home | 240.0 / 259.0 | 242.5 / 246.5 | 295.4 / 310.2 | 311.6 / 320.7 |
| Signed-in Explore | 255.0 / 260.5 | 255.0 / 266.5 | 330.2 / 340.1 | 311.4 / 325.5 |
| Signed-in account settings | 199.0 / 204.0 | 200.0 / 204.0 | 249.6 / 259.1 | 257.1 / 250.1 |

Both revisions have sub-500-microsecond medians throughout this small fixture. The
rewrite does not show the multi-millisecond footer values of the earlier debug
preview here. This does not establish a universal speed improvement: medians and
tails vary, some relative p95 differences exceed five percent, and only 50 observations
per case/run support each p95 estimate. Those differences remain an investigation
item under the delivery plan rather than a waived acceptance gate.

## Total response interval and revalidation

The separate `renderMs` header includes middleware work after the footer. For example,
anonymous home medians were 0.574 / 0.527 ms on main and 0.586 / 0.544 ms on the
rewrite; signed-in home medians were 0.635 / 0.646 ms and 0.649 / 0.656 ms respectively.
Comparing those totals to another build's earlier footer interval would exaggerate
the apparent difference. The raw evidence retains both metrics for every sample.

All measured conditional cases returned `304` with zero body bytes. Their current
`footerSsrMs` is null, because no new HTML was sent. The rewrite supplied a finite
current `view` metric on every such response; main has no `view` metric. Missing
main values remain null and are never counted as zero or substituted from total time.

The frontend navigation/HTMX tests and Rust 200/304/request-isolation tests validate
how the new metric refreshes cached footers. This HTTP experiment did not operate a
browser or verify its displayed text visually.

## Remaining acceptance

Repeat sustained and concurrent workloads, process-first observations, larger real
repositories, Git operations, long logs and the remaining pages. Investigate tail
differences with enough observations and controlled system load; these short runs
do not support p99 or capacity claims. Allocation, CPU/request, idle/peak RSS and
styled browser measurements remain unmeasured here.

This evidence belongs specifically to checkpoint `7326e563`. Subsequent repository,
collaboration, credential and log ports require their own validation and runtime
comparison against the retained baseline executable and fixture.
