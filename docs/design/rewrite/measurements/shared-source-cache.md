# Shared compiler-cache source validation

A 22-step Windows native fixture passed with helper revision
`67a2be069858eac25a6fc41766651ac4f45e4ff9`. It demonstrated correct executable
output across three worktrees sharing one existing compiler lane, including
backdated sources and a legacy writer that did not update the source-proof ledger.
Before and after the run, the helper revision and clean source state matched.
This is compiler-cache correctness evidence, not an application or performance result.

The integrated guard is commit `1263cc38`; its helper source is byte-identical to
the tested guard above. The Windows-default change is `b58fa8c6`, with helper source
byte-identical to the separately tested `fa275da9` revision. These changes were
replayed after the accessible-theme update without changing the validated helper.

The [sanitized observations](shared-source-cache.json) preserve all 22 steps using
worktree labels A, B and C. They omit machine paths, cache identifiers, process IDs,
configuration values, keys and raw logs.

## Method

Each worktree contained the same small two-package, standard-library-only workspace:
a library and an executable. Their source content differed. The library also had a
real build script reading a backdated input and generating Rust source consumed by
the library. The executable printed an exact synthetic tag/value pair. The alternate
feature changed that value, exercising a distinct compiled feature variant.

The run used the helper's owned Windows process-tree completion path and one shared
writer lease. Both Cargo output directories remained in the existing compiler lane;
no checkout-local compiler directories were created. It did not create a replacement
compiler cache, run a global clean, rotate the lane or delete registry/codegen/runtime
outputs. The guard never changes source timestamps to establish freshness. Fixture
sources were deliberately backdated; the legacy-writer control additionally advanced
only synthetic C source timestamps to force an actual overwrite, then restored the
backdated input. That control is not a freshness solution.

Before native compilation, a separate read-only inspection successfully scanned
1,324 local Cargo units from 25 local packages in the existing cache. That inspection
held the writer lease and did not compile or mutate cache contents. It establishes
that the bounded scanner handled that observed layout and size, not that every
existing unit was trusted or that every application target was subsequently rebuilt.

## Observed sequence

Counts below are the helper's source-proof counters. `approved` counts admitted
local units, including build-script execution units and retained variants.
`reusable` counts trusted units at command admission. `fresh` and `rebuilt` count
Cargo compiler-artifact frames; they are not compiler timing measurements. A
build-script-executed frame may describe cached script output and does not by itself
mean the script ran again.

| Step | Case | Profile | Exact output | Approved | Invalidated | Reusable | Fresh | Rebuilt |
| ---: | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | Initial A | dev | `one:11` | 4 | 4 | 0 | 0 | 3 |
| 2 | Warm A | dev | `one:11` | 4 | 0 | 4 | 3 | 0 |
| 3 | Primary library A | dev | Library only | 4 | 1 | 3 | 1 | 1 |
| 4 | Warm primary library A | dev | Library only | 4 | 0 | 4 | 2 | 0 |
| 5 | Alternate A | dev | `one:111` | 7 | 0 | 4 | 0 | 3 |
| 6 | Switch to B | dev | `two:22` | 4 | 7 | 0 | 0 | 3 |
| 7 | Alternate B | dev | `two:222` | 7 | 0 | 4 | 0 | 3 |
| 8 | Return to A | dev | `one:11` | 4 | 7 | 0 | 0 | 3 |
| 9 | Switch to C | dev | `tri:33` | 4 | 4 | 0 | 0 | 3 |
| 10 | Return to alternate A | dev | `one:111` | 4 | 4 | 0 | 0 | 3 |
| 11 | Invalid B source | dev | Expected Cargo exit 101 | — | — | — | — | — |
| 12 | Recover B | dev | `two:22` | 4 | 2 | 0 | 0 | 3 |
| 13 | Return to A | dev | `one:11` | 4 | 4 | 0 | 0 | 3 |
| 14 | Trusted warm A before overwrite | dev | `one:11` | 4 | 0 | 4 | 3 | 0 |
| 15 | Legacy writer C | dev | `tri:33` | — | — | — | — | — |
| 16 | Recover matching-root A | dev | `one:11` | 4 | 3 | 1 | 1 | 2 |
| 17 | Alternate A after recovery | dev | `one:111` | 7 | 0 | 4 | 0 | 3 |
| 18 | Release A | release | `one:11` | 11 | 0 | 7 | 0 | 3 |
| 19 | Release B | release | `two:22` | 4 | 11 | 0 | 0 | 3 |
| 20 | Return to release A | release | `one:11` | 4 | 4 | 0 | 0 | 3 |
| 21 | First release primary export | release | Library only | 4 | 0 | 4 | 2 | 0 |
| 22 | Warm release primary export | release | Library only | 4 | 0 | 4 | 2 | 0 |

Every guarded successful command had verified native completion and reported
`trusted:true`. Executable steps also matched their exact output tags. The expected
compiler failure was specifically Cargo exit 101, followed by a successful recovery;
missing proof counters for that failed command are not reported as zeros. The legacy
writer deliberately bypassed source-proof admission, so its output is not an approval.

## Two important correctness cases

Step 14 first established a trusted, warm A baseline with zero invalidations and
zero rebuilt compiler-artifact frames. A legacy helper then wrote C's artifacts
without updating the ledger. Returning to the same A source and canonical worktree
still detected the changed companions: step 16 invalidated three units, rebuilt two
compiler-artifact units and produced `one:11`. This tests more than a last-owner or
worktree-change check.

The release primary library export was absent immediately before step 21. Cargo
created it while reporting two fresh compiler artifacts and no rebuilt artifacts.
The guard accepted the new exact hardlink through bounded prior/post byte witnesses,
unchanged authoritative controls and verified completion. Step 22 then reused it.
The dev primary export already existed before step 3, which conservatively rebuilt
one unit; that dev case is not presented as first-export proof.

The [source-provenance contract](../../../development-source-provenance.md) describes
the signature and witness rules. Deterministic Node regressions separately cover
same-size writes with restored timestamps, independent copies, missing companions,
source changes during a command, unobserved transitions, bounded witness fallback,
forged/replayed completion and failed diagnostic publication. Those tests complement
this native fixture; they are not additional native-platform observations.

## Windows default ownership check

A subsequent native check at helper revision
`fa275da9e6f638dd2b7cca899a8cd01968d77001` omitted the ownership environment override.
Windows selected the owned Job path, verified its empty process tree, and reused all
four approved units: three fresh compiler-artifact frames and zero rebuilt frames.
The executable still printed `one:11`, and no checkout-local compiler directory was
created. The sanitized JSON records this separately from the 22-step fixture.
Explicit unowned mode and non-Windows activation have separate limits.

## Completion metadata ordering and application qualification

Helper `9ba9388c9784d383ea4ab1964dcc5618328993d1` corrects an ordering
failure observed during long application commands. Cargo metadata can flush its
global-home last-use bookkeeping. Capturing missing-config parent witnesses after
that completion metadata call incorrectly attributed the helper's maintenance to
the preceding compiler command.

The instrumented failing command showed the admission metadata result and the
pre-completion-metadata parent witness were identical: the parent stayed stable
through compilation. Only the completion metadata call changed it. The correction
captures completion policy and absence witnesses before fresh metadata, after
verified native ownership has settled. It does not reuse the admission package
graph. Actual-command config create/delete transitions and parent churn remain
refusals; deterministic regressions cover both.

All 22 native scenarios were rerun successfully with the corrected helper. These
reruns used existing primary library exports; the first-export creation evidence
above belongs to the original run and is not repeated as a new creation claim.
The rerun created no checkout-local compiler directories or new compiler namespace.

An actual `clippy --workspace --all-targets --locked -- -D warnings` command then
passed in 10 minutes 50 seconds with native Windows Job completion and trusted
admission. Application source was
`265c0bb39572b5ed8bb955009452dbe22f373ac9`; the helper was the revision above.
It approved 118 units, invalidated 118, reused zero, reported zero fresh and rebuilt
117 compiler-artifact frames. Its captured witnesses again remained stable from
post-admission metadata to pre-completion metadata; the subsequent metadata call
changed the parent. The helper accepted the command without suppressing native
interval checks. This is application Clippy qualification, not a whole-platform
test, release or performance acceptance result.

## Limits and remaining gates

This fixture establishes the observed Windows owned-completion behavior for a small
synthetic workspace in the existing shared lane. It does not establish Linux/macOS
native ownership, whole-platform correctness, hostile-local-user resistance, hermetic
build inputs, a hard cache-size bound, or compilation speed/memory improvements.
The scanner and witnesses deliberately refuse unsupported or unproved states.

Full latest application tests and matched original-release comparisons remain
pending. Earlier release measurements are not revalidated by these checks;
they require their own source-provenance qualification or guarded rebuilds.
No production-renderer, UI, runtime or release-performance acceptance follows from
these results.
