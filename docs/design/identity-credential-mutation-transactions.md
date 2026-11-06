# Transaction-owned credential changes

Status: dormant concrete store operations. No production route/service selects them;
no writer-v2 mode, session reader, account creation or recovery adapter is activated.
Migration0063 is the queued global allocator (0061 writer compatibility, 0062 locale).

The explicit APIs are:

- `ProfileStore::set_password_hash_if_current_with_intent`: bind replacement to the
  exact original PHC and epoch, unsuspended account and absent current-target pending
  intent. Allocate, conditional password/epoch update, intent insertion and reset-link
  revocation share one owned SQLite transaction. A losing CAS rolls back allocation.
- `ProfileStore::set_password_hash_with_intent`: an already-authorized administrator
  supplies the fixed originally accepted target epoch. It can supersede a pending
  earlier reset but never follows a deleted/recreated target with a different epoch.
  The caller still authorizes before password work; the store does not grant admin
  rights. Original-target capture in the service remains to be wired.
- `RecoveryStore::reset_password_with_intent`: use the SAME eligibility/one-use claim
  as `reset_password`, then allocate and commit the replacement epoch and intent with
  password flags, address verification and reset-link revocation. Bad/expired/changed
  address/suspended proofs return before allocation. Failure after claim rolls it back.

Each returns its committed fixed intent only after successful COMMIT. That is database
state, not acknowledgment of redb cleanup or session issuance authority. None calls
Argon2, looks up a fresh epoch to restamp an old proof, logs plaintext/PHCs, commits
part of a caller transaction, or spawns finalization work.

The allocator's tracked adapter and explicit caller-transaction adapter share one SQL
query implementation and bounded failure diagnostics. New store writes retain the
instrumented pool. On valid state profile mutation performs four statements and one
acquisition; reset performs five statements and one acquisition. Invalid reset proof
still performs one statement/acquisition. Allocation/intent work is a real added cost
compared with the legacy two/three-statement writes; statement counts do not establish
latency. There is no added default read-path query or allocator access.

The legacy profile methods are unchanged. Reset has a private static mode so both
public methods share one-use eligibility/claim, rollback and final token revocation.
The legacy mode retains its original three SQL statements and bind order. Tests must
verify the current reset suite as well as new mode; future source drift must not fork
proof policy across independently maintained reset implementations.

Assumptions before activation remain substantive: every user creation uses committed
global allocation, startup raises the accepted floor above complete SQL/redb history,
old/current-binary legacy writers are interlocked, the finalizer/recovery protocol
uses the fixed returned intent, and all session operations/issuers use epoch authority.
Without those, mixing these methods with legacy password writers can change a PHC
without advancing its epoch. A regression test deliberately demonstrates that gap;
this change does not disguise dormant APIs as a security guarantee.

Temporary-store tests prepare: committed visibility/reopen; original CAS/pending/
suspension refusal; fixed administrator target and deletion/recreation with globally
allocated creation; superseding older pending intents without clearing them; concurrent
reset one-winner behavior; reset eligibility; exhausted/missing/lagging counter; and
password/counter/intent/link-write/deferred-COMMIT failures for both profile and reset.
All failure schedules assert complete unchanged authority and a successful later retry.
These fixtures use synthetic PHCs and closed temporary databases only.

Required shared-lane acceptance (not executed during source preparation):
`cargo test -p rbit-storage --test integration credential_mutations`, allocator tests,
all existing recovery/profile/upgrade/sealing tests, then strict Clippy and the full
workspace. See [the closed activation bundle](identity-activation-bundle.md) for exact
remaining production switch and restart/rollback gates.
