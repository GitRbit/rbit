# Closed authority writer coordination

The authority fence coordinates policy mutations, user account creation, external
identity bindings, personal access tokens, SSH keys and administrator role changes,
suspension and account deletion
within one `Database` and its clones. It stays closed: it cannot issue a generation,
authorize execution, reopen admission or activate a package. Managed execution remains
**Unavailable**.

The neutral `rbit-authority-fence` crate has no dependency on storage, plugin
implementations or the runtime manager. A domain provides writer registrations.
Writers register before waiting for the shared mutation permit; at most 128
mutations may be active or queued. Only the permit holder may begin its enrolled
work. An unused domain starts no task, timer or database query.

Policy initialization, epoch renewal, review/selection/clear, user account creation,
identity linking/unlinking, token and key creation/deletion and the three moderation
mutations acquire that permit
before their first database operation. The wrapper records settlement only after the complete storage result
succeeds, including historical
replays and committed policy seals. Cancellation while queued is clean because the
wrapper has not entered its SQL operation. Dropping an acquired mutation, including
after an error, leaves sticky uncertainty: an asynchronous SQLite operation or
rollback may still be settling. Later successful policy requests cannot erase it.
Uncertainty does not prevent further enrolled writes before permanent closure.

Moderation retains its existing SQL predicates, last-administrator protection and
outcome classification. Each mutation now has an explicit outer transaction with
an awaited commit before the existing classification read. This adds two transaction
control statements to those mutations. It keeps the ordinary NORMAL-synchronous
pool; policy retains its separate FULL writer. Policy and moderation read methods
remain outside mutation serialization. Allocation and latency parity have not been
measured.

Account creation covers `create_user`, `create_user_with_initial_password`,
`register_user` and external-provider `register`. Their shared insertion helper
stays inside the outer guard and transaction. Existing first-administrator,
registration, email-verification and initial-password rules are preserved. An
external-identity conflict explicitly awaits rollback before returning its business
conflict, so no partially created user is published. Failed or cancelled rollback
remains uncertain; a returned row alone cannot certify transaction settlement.

External-identity linking waits for commit or an explicit conflict rollback.
Unlinking adds an outer transaction and awaited commit before its unchanged
Removed/LastMethod/NotFound classification, retaining the provider list and the
last-sign-in-method protection. This adds two transaction control statements to
unlinking. Informational `touch` updates only reported email, username and usage
time; authentication selects the stable provider/subject binding and rechecks the
account. Verified-email matching reads `user_logins`, not this reported metadata.
Those informational updates remain outside authority serialization.

Personal access token creation and deletion await an explicit outer commit before
reporting success. The existing atomic count-limit insertion and ownership-filtered
deletion predicates remain unchanged, including clean limit refusals and missing or
foreign-owner deletion results. Each public call enrolls once and retains its full
result conversion inside that guard. Usage timestamps remain informational and do
not renew expiry or alter scope. These token writes add two transaction control
statements; this does not establish latency or query-count parity.

The four direct personal/deploy SSH key insertion and deletion methods use the same
domain and explicit outer commit. Fingerprint uniqueness, owner/repository binding,
deploy read-only permission and filtered deletion results remain unchanged. Usage
timestamps stay outside authority coordination. These writes add two transaction
control statements each. Account/repository cascades are separate writers, and this
does not make the existing service count-then-insert resource checks atomic or stop
an already running Git command.

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

Calling `Database::close()` stops its enrolled writers synchronously, even
if the returned future is dropped without being polled. Awaiting that future closes
the pools as before. This does not stop other stores' writers or prove that SQLite
workers have exited.

## Coverage before execution

This increment covers policy writes, the four user-creation methods, external
identity linking/unlinking, the three token methods, four direct key methods and
the three moderation methods obtained from the same
database handle or its clones. It does not coordinate independently opened SQLite
pools, another process, password changes, other credentials, KV sessions or other
authority writers. It does not prove SQLite worker termination or physical store
ownership. No runtime may use this
partial coverage to issue execution authority.

Before execution can become available, supported store opens need enforced shared
identity and ownership, and every relevant authority writer must join the same
domain. Cross-store changes need one guard spanning their actual settlement.
Cancellation, uncertain commits and uncertain shutdown must retain closed
ownership. In particular, `SqlitePool::close()` alone is insufficient evidence:
the pinned SQLx implementation discards individual connection-close errors.

A future cooperative authority owner could instead rely on permanent writer
closure and actual settlement of every possible authority change. That contract
must exclude raw mutable SQL, untracked constructor callbacks and work escaping
its guard. It is distinct from proving that driver threads have stopped, and the
current partial enrollment does not establish it.

Native process cleanup and the pure worker protocol are separate prerequisites;
see [process feasibility](native-process-feasibility.md). They do not fill these
store coverage gaps or provide an OS sandbox.
