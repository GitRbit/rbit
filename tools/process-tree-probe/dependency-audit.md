# Published dependency source audit

Audited 2026-11-05 before compiling the experiment. Archives were fetched directly
from crates.io, archive entry paths checked before extraction, and SHA-256 values
matched registry metadata. The reviewed source is the published release, not a
moving repository branch. This is source evidence, not runtime proof.

| Package | Version | Registry archive SHA-256 | License | Published VCS commit |
| --- | --- | --- | --- | --- |
| win32job | 2.0.3 | `8c6a6724ccfbf34154a8691bd868b0fcd2be2ca3f7b47b32614654f1a01b191c` | MIT OR Apache-2.0 | `29913d5f6acd47a491f73819ff6d54b63fb851b2` |
| processkit | 3.3.4 | `1549b59abbe4fc78aa39860e36c81da89262c1278679b8f7299ef4ad7668c004` | MIT | `ba1a6fe77cedad7e1ceeb9f30b158c94b5dc1bb6` |

[win32job registry metadata](https://crates.io/api/v1/crates/win32job/2.0.3)
and [processkit registry metadata](https://crates.io/api/v1/crates/processkit/3.3.4)
provide the release/checksum records. License files are included in both archives.

`win32job`'s published `src/job.rs` creates an anonymous Job with null security
attributes, configures kill-on-close and exposes safe current-process assignment.
Its root handle must live until OS process termination: closing it while this
process runs would terminate the process too. [Exact source](https://github.com/ohadravid/win32job-rs/blob/29913d5f6acd47a491f73819ff6d54b63fb851b2/src/job.rs)
The release's process-list helper has a fixed 1,024-entry buffer; these small
fixtures stay below it. A production census must treat overflow/query failure as
unproved state, not silently interpret the result as empty.

The published `processkit` Windows spawn uses Tokio Command/Child directly, creates
the child suspended, assigns its per-operation Job and then resumes. Its rollback
guard only requests `start_kill`; it does not await reaping. Therefore inherited
root containment and the fail-closed error classification are essential to this
experiment. `kill_all` requests Job termination; an explicit successful empty
`members` query is required separately. [Exact source](https://github.com/ZelAnton/ProcessKit-rs/blob/ba1a6fe77cedad7e1ceeb9f30b158c94b5dc1bb6/src/sys/windows.rs)

Microsoft specifies that CreateProcess children inherit their parent's Job chain
unless breakaway is enabled, and that null Job security attributes create a
noninheritable handle. These are separate properties: inherited membership is
required, inherited ownership handles would undermine kill-on-close.
[Job inheritance](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[handle inheritance](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-createjobobjectw).

Normal dependency footprint from the normalized published manifests:

- `win32job`: `thiserror ^1.0`, `windows ^0.61` with Foundation, Security, JobObjects,
  Threading and ProcessStatus. The existing workspace has `windows 0.62.2`, so a
  second bindings family is expected; the resolved count and size are recorded in [validation](validation.md).
- `processkit`: mandatory `async-trait ^0.1.9`, `encoding_rs ^0.8`, `mutants ^0.0.4`,
  `thiserror ^2`, `tokio ^1.39`, `tokio-stream ^0.1.3`, `tokio-util ^0.7` and Windows
  `windows-sys ^0.61`. `mutants` is a normal dependency, not a dev-only omission.
  The other ordinary names already occur in the workspace lockfile.
- Request only `process-control`, with defaults disabled. No PTY, limits/stats,
  mock, recording, JSON, tracing or metrics feature is requested. Workspace feature
  unification must still be recorded after resolving/building.
- Both platform candidates are dependencies of this unpublished tool only. Existing
  workspace Tokio, anyhow, serde/json and tempfile support the proof harness.

The complete nontruncated RustSec database tree at
[`26fd2d43f7280a1fb925a3743ec463017afe7119`](https://github.com/RustSec/advisory-db/tree/26fd2d43f7280a1fb925a3743ec463017afe7119)
had no `crates/win32job/` or `crates/processkit/` advisory directory. This narrowly
records absence of a direct package advisory at that snapshot; it is not a claim
that these dependencies are safe or that the future resolved transitive graph has
passed an advisory audit. Resolve and inspect that graph before production review.

The resolved experiment added fourteen registry packages without replacing an
existing locked package version or checksum. The complete new-package license,
feature and advisory assessment is recorded in
[evidence/resolved-dependencies.json](evidence/resolved-dependencies.json). The
Windows binding advisory RUSTSEC-2022-0008 is patched in the resolved 0.61.3. This
review covers newly added packages; it is not a full workspace vulnerability scan.
