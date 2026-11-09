# 0.2.0 release non-regression comparison

This November 9, 2026 comparison checks that the 0.2.0 candidate is not slower than
the released baseline on the existing benchmark scenarios. 0.2.0 ships without
retiring the old feature groups, so this is a non-regression check, not a speed-up
claim. Baseline: pinned GitHub main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`
(0.1.5 code). Candidate: `555e66e0c0206d2e3c99d1ff9446aca0f92a8d31`. Sanitized
samples, summaries, build identities and memory readings are in the
[raw evidence](release-020-nonregression.json).

## Method and provenance

Both executables were built from clean detached worktrees through the shared helper
(`workspace.mjs build --release -p rbit-server --locked`) with Rust 1.99.1, LLVM
23.1.1, MSVC, the unchanged release profile (fat LTO, one codegen unit, abort on
panic, stripped symbols), SQLite QPSG and the same compiler namespace. The baseline
took 30m43s and the candidate 1h17m; both source proofs were trusted. Each worktree
ran `npm ci && npm run build` first; its generated CSS/JS hashes are in the evidence
(they differ, as the candidate's interface changed). The helper could not build the
baseline until its policy accepted pinned main's one-line workspace member list.

One stopped seed served both: the baseline signed up the account, `seed.mjs` pushed the
scrolling fixture and opened its pull request, and `content.mjs seed` added
`perf/perf-content` (Markdown READMEs and guides, nesting deeper than the flattening
bound, 120 issues, 40 comments on #1, 24 pull requests and a 64-commit `cold` branch).
Every session copied that seed afresh for each server; the candidate migrated its copy at
start-up. Servers ran isolated (`serve.mjs --isolated true`: log mail, no SSH, Actions or
update checks, default theme), on ports 3305/3306, one persistent connection each.

`measure.mjs` interleaved the variants per request, A/B then B/A each round. Warm runs
used 5 warmups and 40 rounds; two sessions swapped the variant listing, so each warm
page has 80 pairs. Cold pages used per-round paths: every sample viewed a commit not
yet rendered (26 rounds after 4 warmups per session, 52 pairs). Signed-in pages used 30
rounds per session. Sign-in used two separate sessions with throttles off on both
servers (`--rate-limit false`), 30 rounds each. `paired.mjs` gives medians with 95%
bootstrap intervals (2,000 round resamples) and the paired change. A row is flagged
when the candidate's median is more than 5% higher and the intervals do not overlap.

Machine: Windows 11 build 26200, Intel Core i7-14700F (hybrid P/E cores, 28 logical
CPUs), 32 GiB RAM, Node 24.20.0. No Cargo, rustc or linker process ran during sampling;
processor load at run starts was 1–10%. Desktop applications (browser, chat and audio
software) stayed open and were not instrumented.

## Server time (render Server-Timing), milliseconds

Median [95% interval] and p95 per variant; change of the candidate's median.

| Scenario | Main | Main p95 | Candidate | Cand. p95 | Change [95%] |
| --- | ---: | ---: | ---: | ---: | ---: |
| Repository home with README | 4.16 [4.05–4.25] | 6.88 | 4.22 [4.13–4.30] | 6.42 | +1.4% [−1.8, +4.6] |
| Tree, 160 entries | 3.75 [3.67–3.82] | 5.98 | 3.82 [3.74–3.91] | 7.92 | +2.0% [−0.6, +5.2] |
| Blob, 20,000 highlighted lines | 20.6 [20.4–20.8] | 23.1 | 20.8 [20.6–20.9] | 23.4 | +0.9% [−0.6, +2.2] |
| Markdown guide (120 KB) | 4.35 [4.30–4.43] | 6.71 | 4.40 [4.35–4.46] | 6.01 | +1.2% [−1.1, +3.2] |
| Markdown, deep nesting | 4.14 [4.08–4.19] | 7.28 | 4.06 [3.94–4.14] | 8.04 | −1.9% [−4.7, +0.2] |
| Commit over the diff budget | 25.6 [25.5–25.8] | 28.0 | 25.8 [25.7–25.9] | 29.9 | +0.7% [0.0, +1.3] |
| Compare over the diff budget | 59.3 [58.8–59.5] | 63.9 | 59.5 [59.1–60.0] | 63.4 | +0.4% [−0.3, +1.4] |
| Pull request files | 61.4 [61.1–61.8] | 65.8 | 61.9 [61.6–62.2] | 66.6 | +0.9% [+0.1, +1.5] |
| Issues list | 3.34 [3.29–3.38] | 4.67 | 3.39 [3.34–3.42] | 4.03 | +1.6% [−0.3, +3.1] |
| Pull requests list | 3.34 [3.29–3.46] | 5.78 | 3.44 [3.35–3.48] | 5.02 | +2.8% [−0.8, +4.7] |
| Issue with 40 Markdown comments | 10.4 [10.2–10.6] | 13.2 | 9.91 [9.79–10.0] | 12.7 | −4.6% [−6.7, −2.7] |
| Commit history | 3.65 [3.60–3.73] | 4.69 | 3.66 [3.64–3.77] | 4.97 | +0.4% [−1.9, +3.7] |
| Explore | 1.03 [1.02–1.05] | 1.52 | 1.08 [1.06–1.09] | 1.33 | +4.3% [+2.0, +6.5] |
| German: Explore | 1.00 [0.98–1.02] | 1.19 | 1.06 [1.04–1.07] | 1.22 | **+6.5% [+3.6, +8.8]** |
| German: issues list | 3.25 [3.21–3.30] | 3.74 | 3.34 [3.31–3.37] | 3.70 | +3.0% [+0.8, +4.9] |
| German: pull requests list | 3.21 [3.19–3.25] | 3.64 | 3.31 [3.25–3.38] | 3.72 | +3.3% [+1.4, +5.4] |
| German: the other ten pages | | | | | −3.1% to +2.4%, none flagged |
| Cold Markdown document | 6.67 [6.58–6.85] | 7.55 | 6.66 [6.46–6.76] | 7.22 | −0.2% [−4.3, +2.0] |
| Cold highlighting, 2,000 lines | 307 [304–309] | 321 | 307 [304–309] | 318 | −0.1% [−1.2, +0.9] |
| Cold tree | 6.99 [5.94–9.26] | 10.1 | 6.55 [5.91–9.15] | 9.98 | −6.3% [−36, +52] |
| Signed in: repository home | 4.32 [4.24–4.36] | 4.73 | 4.31 [4.25–4.48] | 5.95 | −0.1% [−2.1, +4.9] |
| Signed in: issues list | 3.34 [3.23–3.40] | 4.97 | 3.37 [3.29–3.47] | 3.80 | +0.8% [−1.2, +4.6] |
| Signed in: pull request files | 61.1 [60.8–61.5] | 66.3 | 61.6 [61.3–61.9] | 64.8 | +0.9% [0.0, +1.6] |
| Signed in: dashboard | 1.26 [1.21–1.28] | 1.46 | 1.29 [1.27–1.32] | 1.66 | +2.6% [−0.2, +7.4] |

German rows send `Accept-Language: de-DE,de;q=0.9`: the candidate renders German
(catalogs, translation tables, German numbers and times), main renders English.
The cold Markdown and highlighting pages exercise the candidate's flattening and
blocking render pool on every sample; the deep-nesting document renders 14% smaller.

Sign-in (whole request, password verification and a new session; 60 pairs): main
30.9 ms [19.8–31.5], p95 32.7; candidate 30.2 ms [18.9–31.0], p95 32.5; −2.2%. Both
are bimodal at about 14 and 31 ms in the same proportions, consistent with the hybrid
processor's two core types; the wide intervals come from that, not from either build.

## Flagged scenarios

- **Explore, about 50 µs (+4–7%).** English is +4.3% (below the threshold, separate
  intervals), German +6.5% (flagged); in absolute terms 1.03→1.08 ms and 1.00→1.06 ms.
  The SSR footer splits it: the template interval rose 20–30 µs (0.49→0.51 ms,
  0.46→0.49 ms), the rest falls after the footer. Every candidate page carries about
  8.6 KB more inline stylesheet in `<head>` (Explore: 158,076→166,715 bytes), which
  the finishing middleware buffers and hashes for the ETag; the template adds localized
  chrome and Explore labels. Pages that render longer show the same 30–100 µs as
  0–3%. This matches the earlier unexplained signed-in Explore difference. No
  symbol-enabled profile was taken (stripped release symbols), so the split is from
  the footer and byte counts, not a profiler. No contained fix was identified.
- **Issues list, whole-request time in English (+194%), not a server regression.**
  Server time is equal (3.34 vs 3.39 ms) and so is the time to headers; only reading
  the identity-encoded body differs. Body transfer on this loopback setup stalls by
  10–20 ms at some body sizes regardless of build: main's 298,657-byte deep-nesting
  page and 293,226-byte README page stall, while both builds' 244–254 KB trees do not.
  The candidate's English issues list (210,342 bytes) stalls; its German one (213,730
  bytes) and main's (200,960) do not. Requested with Brotli, as browsers do, the issues
  list takes 4.52 ms (main) and 4.70 ms (candidate). Whole-request identity medians are
  in the evidence for every page; this record uses server time for latency.

## Memory and size

| | Main | Candidate |
| --- | ---: | ---: |
| Executable (bytes) | 46,152,704 | 53,785,088 (+16.5%) |
| Working set after start (MiB) | 25.5–26.5 | 33.3–35.2 |
| Private bytes after start (MiB) | 40.6–43.1 | 47.7–51.5 |
| Peak working set after all page runs (MiB) | 155.7 / 159.7 | 159.0 / 181.5 |
| Private bytes after all page runs (MiB) | 233.7 / 243.9 | 262.6 / 272.9 |
| Working set after 30 sign-ins (MiB) | 53.6 / 52.7 | 61.3 / 60.0 |

Readings are Windows process counters sampled after each run (two sessions shown as
pairs). Allocation counts and CPU per request were not measured.

## Not measured

Browser rendering, scrolling and styled navigation; production Brotli responses beyond
the six-page diagnostic; concurrency, contention, overload and cancellation; true cold
starts and OS cache state; large real repositories; Linux or other hosts; Actions,
wikis, SSH and Git transport; RTL and pseudo locales; footer/304 revalidation (see the
[small-page comparison](footer-b923-small-pages.md)). The seeded content is synthetic.
