# Owned Windows filesystem primitives

Windows-first boundary, registered as a workspace member with a path alias and a
single new lock entry. It builds, passes strict all-target Clippy and runs its
tests on Windows; see [Qualification run](#qualification-run) for exactly what ran.
No independent unsafe audit is claimed yet. Existing locked windows-sys 0.61.2 and
workspace getrandom are the only Windows production dependencies; no SQL, HTTP,
business-domain, runtime or task registry is introduced.

`OwnedDirectory` classifies an actual held synchronous directory as writable NTFS
and nonremote. Opening a directory is **not** a durable-base, Wiki-head or visible
publication receipt. The host's established durable storage base and every newly
created bootstrap/restore binding still need the closed startup protocol described
in [the adapter contract](../../docs/design/wiki-windows-platform-adapter.md).
Ordinary existing directories/markers do not acquire retroactive durability here.

The API creates generated-name child directories, streams create-new staging files,
publishes immutable names without replacement, replaces explicitly mutable slots,
and retires refs into an owned trash namespace. Successful namespace movement uses
write-through and never copy or delayed-reboot flags. File contents are synced before
movement. Own staging handles share delete and are closed before the move; real
critical parent handles continue denying rename/delete. Every child and published
result retains the actual root owner and its parent-handle lineage.

Only `src/windows/native.rs` has the reviewed authored-unsafe exception. All other
workspace lints are copied unchanged; the workspace's forbid is untouched. The
source guard checks this single private exception, other manifests' inheritance,
private module/export rules and retained lint sets. It is not a substitute for
independent unsafe review or the Rust compiler's macro/token lint enforcement.

## Four native calls to audit

1. GetFileInformationByHandle: live borrowed File handle, initialized binding-defined
   integer fields, check BOOL before consuming outputs. No ownership transfer.
2. GetVolumeInformationByHandleW: fixed initialized WCHAR buffer/flags, bounded
   declared size, null optional outputs with zero lengths. Require exact NTFS and
   writable flags. The synchronous Win32 call retains no pointer.
3. NtQueryInformationFile(FileIsRemoteDeviceInformation): initialized HEAP integer
   words and byte backing. Compile-time size/alignment/offset checks match the exact
   binding without constructing/referencing its Rust bool or raw-pointer union.
   Actual subject and exclusive adjacent lease move into the request. On ANY
   nonzero status, including PENDING, do not inspect the outputs: intentionally
   retain buffers AND real handles until process exit. On STATUS_SUCCESS only,
   require zero final status, exact returned byte count and remote byte=0. A
   contradictory completion retains everything; completed remote/unknown values
   refuse. Serialized consumed ownership gives no concurrent query/buffer access.
4. MoveFileExW: private constant flags, lossless bounded NUL-terminated UTF-16 inputs
   live throughout the synchronous call. Immutable moves omit REPLACE_EXISTING;
   COPY_ALLOWED and DELAY_UNTIL_REBOOT are never used. Mutable-slot CAS policy belongs
   to the actual Git writer lock/expected-value check above this filesystem primitive.

The three additional unsafe memory blocks also require review: completed integer
status read, completed remote-byte read, and exact one-time allocation reclaim.
Buffers become raw allocations with Box::into_raw BEFORE submission, so moving the
private request cannot retag a Box still accessed by the kernel. Unproved Drop
reconstructs no Box and retains the actual subject. Box::from_raw is reachable only
after verified completion/length, and consumes precisely those original allocations.
The private synchronous request may be !Send; returned directory owners contain
only standard Files, integers and Arc lineage, without unsafe auto-trait promises.

A quarantined request retains only fixed small buffers plus one consumed real
subject/lease; the real OS lock prevents repeated constructors for the same subject
from allocating another quarantined request. No unsafe Send/Sync, public raw handle,
raw pointer, user-supplied flags, reaper loop or global registry is used. A process
restart is required for this conservative unknown-completion case.

The supported threat model is a controlled local subtree with cooperating writers
and stable topology outside held lineage. Observed reparse/symlink/alias conditions
refuse. This is not a sandbox against a malicious same-user process/driver changing
topology concurrently or hardware falsely acknowledging flush. Native API success
does not prove power-loss behavior beyond the documented OS/filesystem/device contract.

## Prepared tests and qualification

Pure generated-name tests cover traversal, NUL, ADS, separators, percent text,
Windows device aliases, Unicode rejection for generated components and bounds.

Windows native fixtures use ordinary unprivileged temporary directories to exercise
the actual NTFS/nonremote queries, real subject exclusion, Unicode root paths,
new directory lineage, WT file publication/no replacement, unchanged existing bytes/
identity/timestamp, mutable replacement, live-name retirement, own staging movement,
real ancestor rename denial and release after all results are destroyed. These
temporary bases are not a full Wiki bootstrap/durability acceptance fixture.

Controlled subprocess models force pending/unknown/malformed status without issuing
a pending kernel request. Their buffers are poisoned deliberately; no invalid bool
is constructed. The retained real lease must make repeated constructors Busy.
Parent-owned temporary directories clean only after each child exits. These test
retention branches, not actual Windows asynchronous-completion behavior.

Two ignored native fixtures require explicitly supplied disposable scratch roots:
another NTFS volume for actual no-copy failure, and UNC/mapped-share scratch for
actual remote-mode refusal. Unknown native completion in the remote fixture stays
within a child process. Neither fixture may use ordinary user data or silently pass
without its required environment input. Separate mapped and UNC runs are required.

## Qualification run

Run through the shared build helper on Windows 11 x64 (rustc 1.99.1), local NTFS:

- Strict all-target Clippy (`-D warnings`) for x86_64-pc-windows-msvc and, as a
  cross-target check, x86_64-unknown-linux-gnu. The first Windows run found four
  pedantic findings (two `borrow_as_ptr` in native.rs, one each elsewhere); they are
  fixed without changing the reviewed unsafe policy line or any native call.
- Package tests: 6 passed, 2 ignored. These are the name test and the five Windows
  native fixtures above, including the pending/unknown/malformed subprocess models.
- Both ignored fixtures, with explicit disposable scratch roots: cross-volume no-copy
  (C: temp to a D: NTFS scratch directory) and UNC remote refusal through the
  loopback admin share `\\localhost\C$`. Both passed. No mapped-drive run was made,
  and the UNC run does not record whether refusal was Unsupported or Quarantined.
- The source/exception policy (scripts/tests/platform-boundary.test.mjs) is
  unchanged and passes.

Hosted CI runs the package tests on windows-latest and ubuntu-latest through the
workspace nextest job. The rewrite-contracts matrix also runs strict platform Clippy
and the policy tests on both. Linux tests cover only the name surface. No hosted run
of these steps has been observed yet.

Independent per-block unsafe audit is mandatory before integration and has NOT been
done. Linux compiles only the pure name/error surface; the release's Unix durability
adapter remains a separate required implementation. No positive Wiki cap/route or
platform-wide durability claim is enabled by this crate.
