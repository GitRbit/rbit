# Original provider credential-addition fences (inactive)

Rejecting the final cookie is insufficient when a stale callback has already added
an external identity: that identity could authenticate again later. The original
provider path therefore fences the identity INSERT itself. Normal/default public
routes, wire Flow bytes, legacy linking policy and startup remain unchanged.

## Auto-email addition

`Mode::link_accepted` routes only the original verified-email branch to
`ExternalIdentityStore::link_at_epoch`. It carries the epoch/pending captured by
the FIRST verified-email acceptance query, not a later User/current epoch. Captured
pending rejects without a write. The INSERT SELECT checks the same original account,
epoch, unsuspended login and current-target pending exclusion in the SAME SQLite
write transaction. Reset/suspension/new pending that wins before INSERT prevents
both the link and subsequent success. Unique-conflict classification remains the
original follow-up query, only when a valid insertion attempted a conflicting key.

The legacy VALUES INSERT retains its SQL/bind order; both branches share row/error
mapping. New valid original INSERTs have one acquisition/statement, like the legacy
insert. A stale/pending attempt returns Unavailable and creates no method. These
are source/work contracts, not measured timing. No credential epoch is restamped.

## Manual original-session context

The existing stateless Flow seals only an account ID; it cannot become an ACTIVE
link receipt. `begin_original_link` requires an opaque resolved original session,
checks its exact live digest/account/epoch/incarnation against the actual KV owner,
and retains that fixed binding in a clone-shared original-flow owner keyed by the
random state. No User/bare ID/public bool can construct OriginalLinkCallback.

Reservations plus pending flows share a hard 10,000-entry limit, acquired BEFORE
state/PKCE/discovery work. Cleanup scans at most this limit; a reservation dropped
before publication releases its slot. Nonce collisions refuse without replacement.
The nonce, digest and binding are absent from Debug. The original-only context is
transient: expiry or restart requires a FRESH flow; the legacy stateless flow's
restart behavior is not silently adopted as proof. This operational difference is
explicit and mandatory for the security upgrade.

`callback_original` uses the SAME remote verification worker as legacy callback,
with sealed static output hooks. Cookie/state/provider/error/PKCE/OIDC/GitHub and
throttle/audit behavior stay in that worker. The original hook checks retained link
context BEFORE code exchange and creates either SignIn(Callback) or opaque
Link(OriginalLinkCallback). No repeated token exchange or downgrade fallback.
A legacy bare-ID flow cannot produce the Link variant; bare-ID callbacks also
refuse at `complete_external_sign_in_original` before SQL.

`complete_original_link` requires the SAME captured authenticated session binding,
not a fresh session after reset. It reserves the existing tracked mutation scope
synchronously; unpolled/closed admission never claims context. The actual owned
worker rechecks live binding, claims context once, reloads fixed-epoch SQL flags,
and uses the atomic INSERT fence. Caller cancellation after dispatch does not
abandon that tracked SQL/audit worker. A new link retains the existing four SQL
statements/acquisitions (lookup, fixed user, INSERT, audit). Already-linked no-op
completion deliberately adds one fixed-epoch authorization query (two vs legacy
one); no authorization is inferred from the no-op result. SQL errors after accepted
dispatch may require a fresh flow; no exactly-once remote OAuth guarantee is claimed.

Logout racing an already-dispatched privileged action is not cross-store atomic.
The exact live-binding check and SQL epoch fence are explicit linearization points;
full ACTIVE operation admission/shutdown/recovery remains a composition gate.
No nonce-map/controller cycle is introduced: stored pending values contain only
bounded data, while verified contexts hold Weak ownership and cannot keep a
restart-lost map alive or transfer authority to another owner.

## Prepared evidence and mandatory activation work

Prepared actual temporary-store tests exercise manual flow HMAC construction,
opaque post-verification context, unpolled and closed admission retention, single
claim, current-session reset/fresh-session rejection, legacy bare-flow and forged
cookie refusal, fixed-epoch pending/reset writes, successful retry and exact new/
no-op SQL work. They model the private post-provider-verification seam explicitly;
this is not evidence of a real remote-provider/browser run. Separate pure-owner
fixtures prove combined capacity, dropped reservation release, deterministic expiry,
nonce collision refusal and owner separation. None were compiled/run here.

```text
cargo test --locked -p rbit-services --lib external_auth::links::
cargo test --locked -p rbit-services --test integration provider_original
cargo test --locked -p rbit-services --test integration external_auth
cargo test --locked -p rbit-storage --test integration external_identities
cargo clippy --workspace --all-targets -- -D warnings
```

Before ACTIVE selection, wire these typed callbacks into the real shared callback
route and preserve native link-CSRF checks. Account creation/provisioning and
credential-method writes still require the accepted runtime witness before any
externally acknowledged effect; a SQL return or pending map is not durable KV
proof. Complete private ActiveServices/accepted startup, every producer/resolver/
settings/finalizer, bounded recovery/drain, current-store restore/rollback and
fault/HTTP gates together. No current fencing activation, crash recovery or full
identity completion is claimed by these inactive APIs.
