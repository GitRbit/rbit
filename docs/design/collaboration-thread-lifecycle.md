# Thread locking and privileged closure

Collaboration owns `ThreadLocking` and `SystemThreadClosure` applications. A static
repository port carries the caller's original close event as an associated borrowed
type, so both `Closed` commit/pull references and `Merged` strategy/commit payloads
remain unchanged without depending on legacy core or cloning a new event DTO.

Locking recomputes `can_lock` for the actor through the existing permission table,
then rejects archive writes. Neither rejected path constructs a store or reads the
clock. Admitted calls construct one store and read one clock before the original
conditional lock transaction. Already locked/unlocked or missing rows remain empty
successes with one statement; only the SQL winner records and publishes a change.
Publication retains the supplied context's issue snapshot. No throttle is introduced.

System closure remains privileged: existing merge and pushed-commit callers supply
authority, reason, actor and full event. There is no new user/archive/validation gate
or throttle. One store and clock precede the original conditional close transaction.
Only a returned updated row publishes `Closed`; missing/already-closed rows return
false without the fallback lookup of human state changes. The winner's update,
history and thread touch retain one acquisition/three statements; a loser or missing
row retains one acquisition/one statement. Original merge/commit caller sequencing,
clock counts, event payloads and rollback boundaries are unchanged.

No SQL, schema, migration, notification, background task or durable outbox changes.
A failed transaction never publishes. Publication remains transient after commit;
there is no new guarantee against process death or cancellation of pre-commit work.

Source-prepared tests cover construction/clock order, authorization/error precedence,
no-op/error results, borrowed event forwarding, actual lock/close work counts and
concurrent conditional winners. The original privileged closure payload/event test
moves intact to a bounded lifecycle test module. Source and Node checks pass; Rust
execution remains pending. Acceptance requires collaboration application tests,
service library `timeline::lifecycle::tests`, real-store
`timeline::lifecycle_port`, complete timeline/issues/pulls tests and strict all-target
Clippy. This slice does not retire timeline reads, issue reference processing or the
remaining collaboration domain, and makes no measured latency claim.
