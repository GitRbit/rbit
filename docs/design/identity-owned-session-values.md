# Owned session mutation results and measurement

`SessionMutationPermit::submit` keeps its existing unit-returning signature and
semantics. It delegates to `submit_value`, used for completed session snapshots,
booleans and counts. Both retain exactly the same admission token, task tracker,
preconstructed completion RAII, late-after-close handoff, error/panic accounting and
shutdown drain. Returning a value is not permission to return an unfinished worker
handle instead of awaiting effects. No extra supervisor or task is created.

`credentials::finish_update` retains its API and delegates to the used generic
`finish_value`. Tasks capture narrow stores/data rather than their controller, and
dropping a request's waiter detaches rather than cancels the admitted finalization.
Forced runtime termination/explicit task abort remains an abnormal or unproved
outcome; an admitted task is not durable database recovery.

Storage work counters are task-local. Explicit `capture_work` carries only their
private Arc counters into an owned finalization; it carries no service, credentials,
request state or business ownership. Unrelated spawned tasks remain excluded. Nested
count_work still measures its own future. Normal work outside a measurement performs
no counter allocation; common request reads do not call this mutation helper.

As a result, SQL/audit executed by owned mutation workers is now visible in a caller's
explicit count_work measurement. Existing mutation assertions must count those real
statements, not reduce expectations to preserve an instrumentation blind spot.
BEGIN/COMMIT, trigger execution and redb session transactions are still outside Work's
existing statement/cache-write definitions; this does not measure total latency.

Prepared tests cover value/error accounting, an admitted value worker crossing close
and waiter drop, explicit versus unrelated task counter inheritance, and a real SQLite
query through finish_value. Rust tests await the shared lane; no runtime result is
claimed by source preparation.
