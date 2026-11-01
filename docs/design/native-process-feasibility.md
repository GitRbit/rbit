# Controlled Unix process and pipe feasibility

`rbit-plugin-native/tests/process_ownership.rs` exercises fixed OS helpers. The
production crate remains a pure codec, no server profile links it, and package
execution remains Unavailable. These tests are prerequisites for an owned runtime;
they are not an independently installed worker or a production supervisor.

The helper enum selects fixed `/bin/sh`, `/bin/cat` and `/bin/sleep` commands,
an empty environment and cwd `/`. Acknowledgements prevent missing/wrong helpers
from silently passing. No package path, administrator command, credential or
network input enters a helper. Tests never invoke Cargo/rustc or discover hashed
test executables.

## Ownership and I/O

One exclusively owned `std::process::Child` supplies the process-group ID, checked
to exceed one. The handle is never converted to Tokio Child or queued for Tokio
orphan reaping. All real group signals precede the first reap. ECHILD disarms
further signals and reports unresolved cleanup. After reap, a separate type can
only make signal-zero presence observations; reused numeric IDs never authorize
another kill.

This assumes a fresh dedicated test executable without foreign caught handlers
that reap these children. Tokio SIGCHLD registration occurs before spawn. Pinned
signal-hook-registry replaces inherited ignore/auto-reap flags but chains prior
caught handlers; it cannot establish arbitrary-process reaper ownership. The
installed Tokio orphan queue polls only explicitly queued child handles. Neither
observation proves compatibility with an unknown foreign signal handler.

Tokio's safe owned Sender/Receiver constructors verify pipe type/access and set
nonblocking mode. Readiness plus try-read/write loops retain absolute deadlines,
explicit elapsed checks and cooperative progress. No unsafe source or deprecated
AsyncFd constructor is used. A timeout after a successful syscall can include
transferred bytes; it grants no replay guarantee. Spawn and scheduler/kernel
stalls are not hard real-time bounded.

Successful tests require explicit direct-child reap, bounded group absence and
actual stdout/stderr EOF. Group signal success alone is insufficient. On unwind,
Drop signals the still-owned group and polls the leader for at most two seconds;
this best-effort fallback cannot certify descendant cleanup or turn failure into
success. Orphan zombies or numeric-ID reuse can conservatively fail the absence
gate. Same-user native code can escape groups or transfer descriptors; these
checks establish no hostile-code containment. See Linux [wait(2)](https://man7.org/linux/man-pages/man2/wait.2.html)
and [kill(2)](https://man7.org/linux/man-pages/man2/kill.2.html) for the narrow OS semantics.

## Evidence and remaining gates

Two tests pass locally on macOS: acknowledged one-byte framed pipe roundtrip, and
stalled partial-prefix/backpressure deadlines while retaining the child owner.
Both finish with explicit group/pipe settlement. All ten codec tests also pass;
strict Rust 1.99 crate/all-target lint and six actual dependency guards pass.
The first compile failed before helper execution on result inference and the
deprecated AsyncFd API; both were corrected without suppressions. An initial
strict lint failure was corrected by removing redundant continues and fixing a
documentation identifier.

A Linux-only test uses WNOWAIT to observe an exited but unreaped leader,
then verifies descendant-held pipe blocking before force/reap/absence/EOF. It
passed in the Linux contract job for `469307c` in
[run 41320476868](https://github.com/GitRbit/rbit/actions/runs/41320476868);
the complete exact-head matrix was still pending at that checkpoint. The locked nix
safe waitid wrapper is unavailable on macOS, so exited-leader ownership remains
unproven there. Windows has no backend or feasibility harness. Only existing
locked Unix dev dependencies are added; production dependency graphs stay separate.

## Waiter loss and admission ordering

Two additional test-only cases exercise a supervisor that owns its child, pipes,
permit and retained JoinHandle independently of the caller's response receiver.
Dropping the caller requests cancellation. A checkpoint follows an actual Pending
pipe-read poll; a finite, explicitly injected pre-cleanup gate makes retained
admission observable. Both occupied checkpoints call the actual start function
and require rejection before spawn. Two fixed cycles release that gate, then
require real group force/reap/absence/EOF, task completion and renewed admission.

A separate test closes the injected gate. Cleanup errors take precedence over
the coordination error, so its expected gate-closed result follows successful
real settlement. Admission nevertheless stays closed, and another start fails.
The RAII guard closes the semaphore before its owned permit drops on uncertainty.
The caller can neither abort the supervisor nor release its permit.

Join deadlines borrow the retained handle. Failure-only abort attempts a bounded
join and always reports unresolved cleanup; it cannot count as settlement.
Assertion-unwind Drop cannot await: it closes admission and aborts, relying on
the existing bounded child fallback when the task drops. This is not certified
descendant cleanup. Early spawn/ack errors also use that fallback. No database or
durable commit is tested, and the small fixed cycles are not general FD leak proof.

All four macOS OS tests and ten codec tests pass locally, along with strict full
workspace/all-target Rust 1.99 lint. The waiter tests have not yet run on Linux;
the earlier `469307c` Linux result covers only its original three OS tests. This
increment adds no dependency, production runtime or executable availability.

## Codec-aware fixed child

Two more cases use the actual Session over pipes. The parent dispatches one exact
ignored test through current_exe with a fixed good/bad-Ready mode and a mandatory
raw acknowledgement. Wrong filters or zero selected tests cannot satisfy that
acknowledgement. No compiler or binary discovery runs inside the tests.

This fixture explicitly reverses its response channel: SDK frames use stderr and
stdin carries host requests; stdout contains only separately drained libtest text.
Neither channel scans noise for a frame prefix. The stdout drain has a 16 KiB
output cap plus one EOF probe, fixed 512-byte buffers and a 20-second absolute deadline. Its charge
and deadline survive cancellation by the protocol branch; in-flight reservations
stay conservatively charged rather than resetting after an uncertain transfer.

The good case shares one five-second deadline across acknowledgement/Hello/Ready,
then separate five-second deadlines across Ping/Pong and Stop/Stopped/SDK EOF.
Only after both actual pipe EOFs and Session::finish does it reap the child
naturally, require successful exit and observe group absence. The bad case requires
the specific terminal identity error from a canonical Ready with the wrong nonce;
it then forces the still-owned group and requires reap, absence and both EOFs.
Noise, wrong dispatch or a timeout cannot pass as the expected identity rejection.

The child uses the real canonical decoder/encoder for exactly three fixed requests.
Its blocking IO has no independent timer; the parent owns deadlines and cleanup.
This is a controlled SDK fixture, not an installed package, general worker session
implementation or worker-side deadline guarantee. It adds no graceful TERM test,
partial-frame session timeout, flood supervisor or production launcher.

All six macOS OS tests and ten codec tests pass locally, with one child helper
ignored by the outer harness and actually dispatched by its two parents. Strict
full workspace/all-target Rust 1.99 lint passes. The new waiter/protocol cases still
need Linux hosted execution; no Windows backend or performance result is claimed.

## Permission-unknown group observations

Two expanded parallel runs exposed post-reap EPERM in the waiter tests; stage
diagnostics distinguished it from force signalling, child reap and pipe EOF.
Both failures are retained. The matching Darwin 25.5 kernel's
[group-signal implementation](https://github.com/apple-oss-distributions/xnu/blob/xnu-12377.121.6/bsd/kern/kern_sig.c#L1613)
can return EPERM for an existing group with no eligible members. This is a
plausible mechanism, not proof of the failed groups' exact original state.

The presence observer now treats EPERM as unknown and continues only its bounded
signal-zero observations. The original five-second deadline and ten-millisecond
poll interval remain; only actual ESRCH before the deadline establishes absence.
Persistent permission denial still fails, and no real signal is authorized after
reap. Deadline checks precede and follow each probe. Three injected observation
tests verify unknown followed by absence, permanent denial, and unknown followed
by presence then another error. These are branch tests, separate from OS evidence.

## Partial-session deadlines and simultaneous raw output

The last compact increment adds two fixed modes. Partial Ready validates Hello,
writes exactly two encoded Ready-prefix bytes and blocks. The actual Session
consumes that partial input without completing a message, then a shared 300 ms
write/read deadline expires. A fixed shell bootstrap preserves ignored TERM
through quoted positional exec; a separate 300 ms grace performs no reap.
The parent then requires SIGKILL status, group absence and both pipe EOFs before
asserting the expected timeouts and terminal session. This tests controlled
escalation, not a production signal policy or a macOS exited-leader observation.

The other mode produces concurrent raw stdout/stderr bursts, 256 KiB each,
after an acknowledged fixed start byte. It is explicitly not an SDK dialogue.
Both independent drains must observe more than their 16 KiB cap and return cap
refusal; timeout, EOF or only one noisy stream cannot substitute. After force/reap,
cleanup retains original counters and a 20-second deadline, using a separately
reserved 256 KiB per channel plus an EOF probe. Fixed 512-byte buffers avoid output
queues. Missing EOF or exhausted reservation fails; no generic pipe-size bound
or sustained-rate guarantee is inferred.

The combined macOS suite passes eight actual OS tests, three injected observation
tests and ten codec tests, with one outer ignored helper actually dispatched by
four parent cases. Strict full workspace/all-target Rust 1.99 lint passes. The
separate observation correction was also exported and tested independently before
its commit. Failed parallel runs and strict-lint attempts remain retained.
The newer waiter/protocol/limit cases still require Linux hosted execution.

Still required: production Hello/Ready and partial-frame session deadlines, graceful
TERM policy, production waiter-loss ownership/admission, bounded flood queues,
repeated FD/task/process leak checks, supported platform backends, durable attempt
settlement/restart recovery, executable image preparation, explicit execution-policy
review and enforced precommit revocation across every relevant writer. These two
local successes do not complete those gates or establish performance parity.
