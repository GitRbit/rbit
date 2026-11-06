# First native validation

Validated on November 5, 2026, on Windows 11 build 26200, x86-64 MSVC,
with rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1). This is development evidence,
not approval to add global Job ownership to production.

## Executed checks

- Shared-helper package tests: three passed, none skipped.
- Shared-helper all-target Clippy with `-D warnings`: passed after source fixes,
  without lint suppressions.
- Shared-helper package build: passed; verification used its exact copied binary
  through the owned runtime launcher, not the generic server run command.
- All eight native scenarios passed; the owned launcher subsequently verified
  termination. The unrelated sibling remained alive in every scenario.
- Six dependency-policy unit tests and whitespace checks passed.

[Native JSON evidence](evidence/windows-2026-11-05.json) distinguishes successful
native observations from simulated fault policy. The suspended-gap case observed
inherited root membership before per-operation assignment. Abrupt parent death
emptied the still-open outer Job without its controller requesting a Job kill;
this behaviorally establishes root handle noninheritance for this configuration.
The live-tree case exercised native helpers and grandchildren under nested Jobs.

The root-exits case observed helpers and an inherited stdout pipe outlive the
direct child. Admission stayed occupied until membership emptiness, direct wait
and pump completion. Normal cancellation observed those same completion facts.
The invalid-handle assignment failure was a real native error with a suspended,
root-contained child. Its report correctly claimed quarantine, not a drain.
Wait, query and resume faults were simulated contract failures after native
cleanup; this does not exercise failures inside dependency OS calls.

Abrupt parent death cannot report Rust pump destruction in the dead process.
Those cases explicitly leave that evidence false and report OS cleanup rather
than normal verified completion. Direct-child waits, Job member queries, process
identities and sibling checks are separate observations, not interchangeable proof.

## Resolved dependency review

Resolution added fourteen registry packages and the unpublished tool. No existing
locked package version or checksum was removed or changed. Additional binding
families are real overhead, not hidden production dependencies.

| Packages | License declaration |
| --- | --- |
| processkit 3.3.4; mutants 0.0.4 | MIT |
| win32job 2.0.3; thiserror and thiserror-impl 1.0.69 | MIT OR Apache-2.0 |
| windows 0.61.3; windows-core 0.61.2; windows-result 0.3.4; windows-strings 0.4.2 | MIT OR Apache-2.0 |
| windows-collections 0.2.0; windows-future 0.2.1; windows-link 0.1.3; windows-numerics 0.2.0; windows-threading 0.1.0 | MIT OR Apache-2.0 |

License files were present in the downloaded releases except mutants 0.0.4,
which declares MIT in its manifest and includes a copyright notice but no
standalone license file. Its only no-dependency procedural macro returns its
input unchanged; it is an ordinary processkit dependency, not test-only code.
That packaging caveat remains visible for any production dependency review.

Resolved processkit features are exactly `process-control`, without defaults;
no PTY, resource statistics, mocks or metrics are enabled. The windows 0.61
family enables std and Foundation, Security, JobObjects, ProcessStatus and
Threading. Existing workspace Tokio contributes its runtime/process/I/O features.
The complete new-package feature lists, checksums and source sizes are in
[resolved dependency evidence](evidence/resolved-dependencies.json).

The complete, nontruncated RustSec tree at
[`26fd2d43f7280a1fb925a3743ec463017afe7119`](https://github.com/RustSec/advisory-db/tree/26fd2d43f7280a1fb925a3743ec463017afe7119)
contained no advisory markdown for the new package names except windows.
[RUSTSEC-2022-0008](https://github.com/RustSec/advisory-db/blob/26fd2d43f7280a1fb925a3743ec463017afe7119/crates/windows/RUSTSEC-2022-0008.md)
is patched at >=0.32.0, so resolved windows 0.61.3 is patched. Absence from that
snapshot is not a safety guarantee or an audit of all pre-existing workspace code.

## Measured footprint

The first package test build reported 18.67 seconds, including newly required
common dependency variants in an already warm repository cache. A later fresh
build reported 0.13 seconds. Neither is a clean production build comparison.

The copied dev executable is 1,307,648 bytes. Cargo-reported build outputs for
the new packages and probe, plus adjacent executable/DLL PDBs, total 47,443,501
bytes across 36 files. That excludes test/check variants, build-script outputs,
shared existing packages and additional runtime copies; it is not the entire
incremental cache cost. The new registry source trees total 120,831,354 bytes,
of which windows 0.61.3 alone accounts for 114,341,351 bytes.

The completed shared cache report retained 18,724,561,180 bytes, with
1,544,691,996 bytes above the 16 GiB soft limit. The current namespace remained
protected and no eviction was performed. A pre-experiment atomic cache baseline
was not captured, so a precise marginal cache increase cannot be inferred.

Further review must cover other supported Windows hosts and nested host policies,
native failure injection, source package licensing completeness, release/binary
overhead, and production admission/quarantine/health ownership. Unix process
groups and hostile `setsid` escape are outside this Windows experiment.
