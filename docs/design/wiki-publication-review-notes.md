# Wiki publication questions before an active writer

The current W0/W1 contracts and published repository-wiki plan remain authoritative.
This reviews unmerged alternative designs in commits 8a6250a2 and 6a5dbe55, without
adopting their storage format, budgets or claimed guarantees. Their documents have
not been merged or modified by this implementation. No publisher exists yet.

## Required protocol distinctions

Durable pending intent acceptance and visible completion are different events. A
write cannot be described as completed at intent insertion if reconciliation may
still abort it for missing objects and readers still select the previous catalog.
Define acknowledgement and visibility points explicitly before network writes.

A target ref already equal to the proposed commit is not sufficient to finalize.
Reconciliation must verify the required commit/tree/blob closure, identity and
durability/pinning conditions, including the already-updated-ref case. Missing
objects after a crash or mismatched restore must fail closed. Current prepare only
returns a syntactic OID, without fsync, reachability or publication evidence.

The accepted model uses a repository-wide head expectation plus wiki incarnation.
Unrelated page writes currently conflict. Per-page concurrency with rebase onto a
newer wiki head is a separate policy change requiring tests; a server must not
silently replace the user's submitted head with a freshly read one.

After durable intent acceptance or an uncertain result, retries must retain the
original command identity and reconcile it. Replacing every token on 503 can lose
an already-accepted mutation's identity. Pre-admission failures and accepted work
require distinct handling.

A backup that captures SQL pending targets then mirrors only Git refs can omit an
unreferenced target. Require a closed writer/drain barrier and root pin/copy, or
explicitly copy every pending, visible and historical root. GC grace alone does not
put unreachable objects into a ref-based mirror.

## Explicit choices still required

The alternative format uses hexadecimal page keys and nested content/meta/media
trees. Current contracts use positive integer IDs and a strict flat numeric blob
tree; the native validator refuses the alternative tree. Choose a versioned format
before a writer, with an explicit conversion decision if needed. Never infer/adopt
another format from existing directories. Native SHA256 support is not implemented.

Alternative title/path/body/history/page limits differ from current typed constants.
Reconcile each budget explicitly; proposed defaults are neither implemented limits
nor measured latency targets. Current bodies preserve raw bytes; newline
normalization would also change the contract.

Rendering depends on the full document base, including directory and repository
URLs. A body-OID-only cache key is unsafe if relative links/images are already
rewritten. Retain the full base in the key or defer every base-dependent rewrite,
not just wiki-link slots. Locale may be omitted only for a user-content subtree
without localized presentation. Raw URL decoding, wiki-specific document bases,
media ACL/cache revocation and authorization before 304 remain future gates.

Current preparation/read uses native Git without child processes. External object/
ref commands need verified lifetime and durability contracts; legacy output helpers
do not establish awaited descendant reaping on arbitrary cancellation. Native ref
CAS and crash/backup behavior need pinned-API and filesystem evidence before adoption.

## Incarnation correction

An AUTOINCREMENT WikiId now distinguishes catalog incarnations from reusable repo
IDs while preserving the independent store-identity binding. Exact object markers
bind all three. Prepared fixtures cover actual repository deletion/recreation and
copied foreign scopes; they are not executed native evidence. This adds no publisher,
route, UI, tombstone, alias, recovery, backup or durability guarantee. W1-W5 remain.
