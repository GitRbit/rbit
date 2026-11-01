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
