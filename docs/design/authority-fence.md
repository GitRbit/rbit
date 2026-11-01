# Closed policy writer coordination

The first authority fence increment coordinates policy mutations within one
`Database` and its clones. It stays closed: it cannot issue a generation, authorize
execution, reopen admission or activate a package. Managed execution remains
**Unavailable**.

The neutral `rbit-authority-fence` crate has no dependency on storage, plugin
implementations or the runtime manager. A domain provides writer registrations.
Writers register before waiting for the shared mutation permit; at most 128
mutations may be active or queued. Only the permit holder may begin its enrolled
work. An unused domain starts no task, timer or database query.

Policy initialization, epoch renewal and review/selection/clear mutations acquire
that permit before their first database operation. The wrapper records successful
settlement only after the complete storage result succeeds, including historical
replays and committed policy seals. Cancellation while queued is clean because the
wrapper has not entered its SQL operation. Dropping an acquired mutation, including
after an error, leaves sticky uncertainty: an asynchronous SQLite operation or
rollback may still be settling. Later successful policy requests cannot erase it.
Uncertainty does not prevent further passive administrator policy writes.

A snapshot describes enrolled work at one instant. It grants no authority. A
successful drain retains the shared permit in an owned guard, preventing enrolled
writers from starting until that guard is dropped. It cannot certify an uncertain
domain. Dropping the drain guard releases serialization and opens no execution
gate.

## Permanent writer closure

Stopping writers is irreversible within the domain. It rejects later requests and
prevents already queued requests from starting SQL. An acquired mutation may still
finish; dropping it remains uncertain. Cloned registrations observe the same stop.

Terminal writer settlement requires stopped admission, no active mutations and no
uncertainty. Queued tickets may remain owned, but can never start enrolled work.
This is deliberately narrower than draining every future or resource: a forgotten
queued future must not prevent proof that permanently stopped writers cannot issue
new authority changes. Queue counters are retained until their tickets are dropped.

Calling `Database::close()` stops its enrolled policy writers synchronously, even
if the returned future is dropped without being polled. Awaiting that future closes
the pools as before. This does not stop other stores' writers or prove that SQLite
workers have exited.

## Coverage before execution

This increment covers policy stores obtained from the same database handle or its
clones. It does not coordinate independently opened SQLite pools, another process,
KV sessions, credentials, account authority or other writers. It does not prove
SQLite worker termination or physical store ownership. No runtime may use this
partial coverage to issue execution authority.

Before execution can become available, supported store opens need enforced shared
identity and ownership, and every relevant authority writer must join the same
domain. Cross-store changes need one guard spanning their actual settlement.
Cancellation, uncertain commits and uncertain shutdown must retain closed
ownership. In particular, `SqlitePool::close()` alone is insufficient evidence:
the pinned SQLx implementation discards individual connection-close errors.

Native process cleanup and the pure worker protocol are separate prerequisites;
see [process feasibility](native-process-feasibility.md). They do not fill these
store coverage gaps or provide an OS sandbox.
