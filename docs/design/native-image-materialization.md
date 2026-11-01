# Verified non-executable native image preparation

`rbit-plugin-packages::NativeImageStore` prepares owned payload files for trusted
host composition on Unix. It is a prerequisite for a future native backend and
performs no launch, interpreter invocation, grant, policy transition or host call.
Current package execution availability remains Unavailable. Passing an
InstalledPackageRef supplies an exact comparison target, not installation, trust,
review, selection or durable-authority proof.

## Stream and output ownership

The host provisions a private stable image root separate from its CAS root. The
helper selects the archive solely by digest under PackageStore; callers cannot
supply an arbitrary archive source or per-request extraction destination. One
O_NOFOLLOW/O_NONBLOCK open and regular-file size check precede reads, including
rejection of FIFO objects without waiting for a writer. The same opened descriptor
drives complete archive hashing, the existing strict USTAR parser and tentative
payload writes. There is no validate-then-reopen copy.

A private static sink receives only parser-validated declarations/path syntax and
bounded chunks. Chunk bytes remain tentative until each file digest, padding,
complete membership, two-zero-block terminator, actual EOF and the whole archive
hash succeed. Full package ID/SemVer/build metadata and expected wire size must
also match. A valid early entrypoint cannot escape a corrupt later payload/trailer.
Nonnative kinds are rejected before payload writes. The public validate_archive
uses NoOutput; its original traversal is identical apart from four private hooks.
All existing malformed archive tests still apply.

Every image has a fresh owned TempDir with exact `0700` directories and `0600` regular
files created through create_new. Archive `0755` modes are never copied. Restrictive
umasks fail without chmod repair. No executable permission or writable File handle
is returned. PreparedNativeImage is non-Clone and owns the directory until explicit
discard or synchronous best-effort Drop. Borrowed paths and metadata are for trusted
host code only. The helper verifies bytes/declarations; it does not validate native
executable format, target compatibility, dynamic linkage or future spawn safety.

Unix type/mode checks do not verify root ownership, inherited ACL policy or ancestor
ownership. Private provisioning, exclusive write authority, ancestor stability and
finite host admission remain caller preconditions. Same-UID code can alter these
files and has the service OS user's authority; this is not OS immutability or a
sandbox. Non-Unix hosts return Unsupported before root I/O. Windows ACL/native
backend validation remains unfinished.

## Bounds, control and failure

Existing manifest file/count/payload and archive wire limits still bound one image.
The expected wire size must leave room for the real EOF probe under StageControl's
wire quota. Shared cancellation and deadline state is checked around bounded reads,
writes and file/directory creation, and after final validation. Specific source,
control and output errors survive the private parser adapter. Public failure
Display/Debug is bounded static metadata; retained paths are never error prose.

Byte/control checks cannot interrupt a blocked filesystem syscall. A host must
retain finite concurrency/disk admission through actual blocking work and cleanup,
even after losing an async waiter. Per-image limits do not cap global concurrent
disk usage. No hard filesystem deadline or detached cleanup task is introduced.

Failure returns no completed image. Payload/source handles close before cleanup;
cleanup continues after cancellation. Explicit failure/discard reports Complete or
Pending cleanup, preserving a host-only residual path on Pending. Ordinary Drop
cannot report cleanup failure. The later coordinator must retain/reconcile that
ownership rather than release capacity or forget residual disk work.

Images are ephemeral derivatives of durable CAS. No image fsync/publication receipt
or new durable installation ledger is added. Restart reconciliation must discard or
rebuild stale images under host ownership while launch admission remains closed.
CAS objects and unrelated image owners are not cleanup targets.

## Validation and open work

Local package validation passes 36 top-level tests: five unit, eleven strict archive,
nine Unix image and eleven store tests. Two image helper tests are ignored by the
outer harness and actually executed/reaped by bounded, acknowledgement-checked FIFO
and restrictive-umask parent tests. Real-filesystem cases include exact bytes/modes,
full identity/kind/size/hash mismatches, later corruption, unsafe paths/types,
concurrent owners, same-open inode replacement, Interrupted short reads, actual
partial File writes, injected write/read faults and cancellation. The writer fault
wrapper is private/static; production uses exactly File. Injected faults are not
real disk-full or crash evidence.

Separate integration checks pass nine Services package tests in each default/core
profile, seven core Web upload tests (three existing helper cases remain ignored
by the outer harness), and all sixteen backup tests. Strict package/all-target
Clippy passes on Rust 1.95, and full workspace/all-target Clippy passes on Rust 1.99.
Native Windows Unsupported behavior and full hosted exact-head
validation remain required; Unix success makes no Windows ACL claim. Actual
cleanup-Pending OS failures, crash/restart/disk-full recovery and uninterruptible
syscall deadlines remain unproven. There is no performance/allocation certification.

Before execution, the host still needs exact live ledger authority, explicit new
execution-policy/re-review, revocation across policy writers, owned durable attempts,
coarse host data/scratch, safe supported-platform process backends and real owned
cancellable pipe/deadline/reaping tests. This helper and the pure worker codec do
not complete an executable runtime or independently packaged Actions/i18n.
