# Closed authority writer coordination

The authority fence coordinates policy mutations, user account creation, external
identity bindings, personal access tokens, SSH keys, SQL recovery, authentication
factors, profile credentials, account renames and administrator role changes,
suspension, account deletion and bound session mutations
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
mutations, together with SQL recovery, factor and profile credential mutations,
and account renames, acquire that permit
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

The five SQL recovery mutations share the domain. Token insertion retains its
expired-token sweep transaction; consumption, revocation and email verification
await an outer commit, including no-op results. A reset that cannot redeem its
token explicitly awaits rollback before returning `None`. Successful resets retain
their existing atomic password, verification and reset-token changes. Purpose,
expiry, current-email and suspension predicates remain unchanged. The three newly
transactional methods add two control statements each. The later KV session
revocation and owned service task are outside this SQL settlement claim; complete
cross-store coordination remains required before execution authority can be issued.

All ten direct factor mutations enroll, including TOTP replay steps, recovery-code
consumption and passkey sign counters. Existing sealed-secret comparisons and
first/last-factor recovery-code maintenance remain unchanged. Transactional early
refusals explicitly await rollback. The existing read-only missing/wrong-secret
refusal in `claim_step` stays before its DML transaction. Five formerly autocommit
write paths add two control statements each when they reach their write. Later KV
challenge/session changes and full service-task settlement remain outside this
SQL-only coverage.

The four profile credential APIs each enroll once around their complete result,
calling existing private helpers directly. Email changes retain their atomic
verification and old-address token invalidation behavior. Password changes retain
their current-hash predicate and reset-token revocation; a missing account or stale
hash explicitly awaits rollback before its clean no-op result. Informational
profile fields and later KV session revocation remain separate.

Both account rename methods enroll and await an outer commit, including missing
accounts and conditional refusals. User and organisation IDs, username conflict
mapping, current-password/suspension predicates and the existing redirect trigger
remain unchanged. Case-only changes do not create redirects, and old names are not
sign-in aliases. Each rename adds two transaction control statements.

`Database::open_kv` creates session-capable KV handles with the same immutable
in-process registration. Services reject a mismatched SQL/KV pair before their
first query or bootstrap action; independently opened SQL handles stay different
domains even when their file path is the same. The public unbound KV constructor
is removed, and cache consumers retain their existing cache handle. This binding
alone does not establish physical ownership or enroll constructor transactions.
Those remain prerequisites below.

Session put, renewal, deletion and account-wide revocation acquire admission before
submitting blocking work. The actual worker owns the mutation through its complete
synchronous result: losing its async waiter does not release the guard. A successful
commit or explicit no-op abort settles inside that worker; error, panic or destruction
of an acquired guard remains uncertain. Expired-session lookup enrolls only its lazy
cleanup branch and preserves the transaction recheck against replacement or renewal.
Live reads and account session listings remain outside authority serialization.

Expiry purge keeps one read snapshot and timestamp through bounded worker handoffs.
It examines at most 256 rows per read chunk, accumulating expired candidates until
the existing 256-candidate batch limit or snapshot end. Pure scanning holds no
authority permit or redb writer. A shared fail-fast reservation bounds concurrent
purges to one per KV handle and its clones, retained with actual worker resources
even after waiter loss. Each write batch separately owns and settles its mutation.
Earlier committed batches remain known progress if a later scan fails or admission
stops; non-success does not imply rollback. A failed actual write batch is uncertain.
These checks do not make blocking IO cancellable or certify constructor writes,
physical resource release or whole SQL-plus-KV service task settlement.

Forced administrator password replacement uses a typed, database-owned command
paired with its already-bound KV handle. Two storage reservations bound admission
before hashing; two Recovery task slots retain hashing and the audit/event tail.
Once validated and admitted, losing the request no longer cancels that continuation,
including its pre-SQL hash phase. One mutation spans the acknowledged SQL password
transaction and actual redb revoke-all transaction, with no nested public enrollment.
The actual KV worker retains both the mutation and a command lease. An async owner
lost before acknowledgment records sticky uncertainty even while that worker lives;
its later success cannot erase the unknown outcome.

Terminal admission and registration share a mutex, released before task spawning:
a stopped runtime may synchronously destroy the captured task. Stale unstarted
reservations reject before SQL; active commands finish their raw KV phase after
writer stop. Shutdown drains retained command leases and Recovery task outcomes,
which are distinct from physical SQLx worker termination. Known pre-write refusal
and normally returned service failures are distinguished from unknown settlement.
The existing missing-account behavior, reset-link invalidation, session index and
post-revocation audit wording remain intact. SQL and KV commits are sequential:
a failure after SQL can leave a changed password and old sessions. This is not
crash-safe revocation or a certificate for self-service, reset-link, deferred
password-change guards, independently opened stores or physical ownership.

A snapshot describes enrolled work at one instant. It grants no authority. A
successful drain retains the shared permit in an owned guard, preventing enrolled
writers from starting until that guard is dropped. It cannot certify an uncertain
domain. Dropping the drain guard releases serialization and opens no execution
gate.

## Permanent writer closure

Stopping writers is irreversible within the domain. It rejects later requests and
prevents already queued requests from starting enrolled work. An acquired mutation may still
finish; dropping it remains uncertain. Cloned registrations observe the same stop.

Terminal writer settlement requires stopped admission, no active mutations and no
uncertainty. Queued tickets may remain owned, but can never start enrolled work.
This is deliberately narrower than draining every future or resource: a forgotten
queued future must not prevent proof that permanently stopped writers cannot issue
new authority changes. Queue counters are retained until their tickets are dropped.

Calling `Database::close()` stops its enrolled writers synchronously, even
if the returned future is dropped without being polled. Awaiting that future closes
the pools after draining retained forced-password command leases. Unknown outcomes
are logged as unresolved. Bound session workers share this stop; unrelated domains
remain independent. This does not prove that SQLite or redb resources have exited.

## Coverage before execution

This increment covers policy writes, the four user-creation methods, external
identity linking/unlinking, the three token methods, four direct key methods and
five SQL recovery methods, ten factor methods, four profile credential methods,
two account rename methods, the three moderation methods and bound session workers
obtained from the same database handle or its clones. It also covers the narrow
forced administrator password command described above. It does not coordinate
independently opened SQLite
pools, another process, the remaining SQL-plus-KV credential changes, constructor writes
or other authority writers. It does not prove SQLite worker termination or physical store
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
