# Kernel blocking-work contract

The first kernel primitive is `BlockingBudget`, a concrete admission boundary for one
class of synchronous CPU or filesystem work. The composition root constructs nonzero
limits and injects cloned handles. There is no process-global configuration, forge
entity, HTTP status mapping, persistence access or dynamically registered service.

Admission is immediate: `Busy` means all permits are already held; `Closed` means the
owner stopped accepting work. Neither rejection schedules a closure. Protocol adapters
choose their overload/shutdown responses; neither condition is a worker failure.
The returned Tokio join handle separates panics/cancelled queued tasks from the
operation's own typed result.

The permit moves inside the blocking closure before submission. It remains held while
Tokio queues the job and while it executes. Dropping or timing out a requesting future
cannot admit replacement work while the abandoned operation still consumes resources.
Aborting a running blocking job does not stop it. A queued aborted closure releases its
permit when Tokio disposes it; reclamation need not happen at the instant of abort.

Deadlines bound response waiting, not synchronous execution. Callers reject already
expired requests before submitting work and may use `timeout_at` on the returned handle.
Long operations need their own cooperative cancellation points. Never release capacity
on response timeout; never pretend that dropping a join handle stopped a worker.

`close()` rejects future submissions across clones, including while saturated. A job
that acquired its permit concurrently with closing is already admitted. Composition
must track/join outstanding jobs before tearing down dependencies. This primitive does
not invent a task supervisor; future lifecycle work must build on demonstrated needs.
Panic unwind releases the slot; a release build using panic=abort terminates the process.

Tests synchronize real blocking workers to verify immediate overload, request abort,
response timeout, panic cleanup, queued abort and close/drain semantics without relying
on sleep timing. Limits must be measured per workload: Argon2 memory admission and Git
process admission should not compete for one undifferentiated permit pool.

For password integration, inject the budget at the application service boundary so all
hash/verify/dummy verification operations share the same configured capacity. Preserve
Argon2 parameters, PHC format and username-enumeration defenses. Do not nest an admitted
blocking closure around an async password facade that spawns a second blocking task;
provide an explicit synchronous crypto primitive or make the existing facade accept the
budget. Map Busy and Closed distinctly from invalid credentials and crypto failures.
